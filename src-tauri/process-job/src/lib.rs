//! Safe Windows job-object boundary for PADE's PTY host.
//!
//! The PTY host outlives the app on purpose, so the agents it runs survive an
//! app restart. The flip side is that a host which crashes, or is killed, must
//! not leave those agents running with nothing able to reach them. A job object
//! with `KILL_ON_JOB_CLOSE` gives exactly that: the host joins the job, every
//! process it starts afterwards joins with it, and the moment the host's handle
//! to the job closes - however the host ended - Windows ends them all.
//!
//! A deliberate exit is different: the host has already ended its sessions, and
//! whatever an agent launched to outlive it (a browser, an editor) must not be
//! killed on the way out. [`KillOnCloseJob::release`] drops the kill limit
//! first, the way cargo does for `cargo run` on a normal exit.

#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT,
    JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
#[cfg(windows)]
use windows::Win32::System::Threading::GetCurrentProcess;

/// A job object the current process belongs to, which ends every process in it
/// when its last handle closes. Breakaway stays allowed, so a launcher that
/// explicitly asks to leave the job (`CREATE_BREAKAWAY_FROM_JOB`) still can.
#[cfg(windows)]
#[derive(Debug)]
pub struct KillOnCloseJob {
    handle: HANDLE,
}

#[cfg(windows)]
impl KillOnCloseJob {
    /// Create the job and put the current process into it. Every child the
    /// process creates from now on is in the job too.
    pub fn adopt_current_process() -> windows::core::Result<Self> {
        // SAFETY: both arguments may be null (default security, unnamed job);
        // the returned handle is owned by `Self` and closed in `Drop`.
        let handle = unsafe { CreateJobObjectW(None, windows::core::PCWSTR::null())? };
        let job = Self { handle };
        job.set_limits(JOB_OBJECT_LIMIT(
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE.0 | JOB_OBJECT_LIMIT_BREAKAWAY_OK.0,
        ))?;
        // SAFETY: `handle` is the live job created above, and the current-process
        // pseudo-handle is always valid; neither call retains the pointers.
        unsafe { AssignProcessToJobObject(job.handle, GetCurrentProcess())? };
        Ok(job)
    }

    /// Stop ending the job's processes when it closes - for a deliberate exit,
    /// after the host has ended its own sessions.
    pub fn release(&self) -> windows::core::Result<()> {
        self.set_limits(JOB_OBJECT_LIMIT_BREAKAWAY_OK)
    }

    fn set_limits(&self, limits: JOB_OBJECT_LIMIT) -> windows::core::Result<()> {
        let mut information = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        information.BasicLimitInformation.LimitFlags = limits;
        let length = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .map_err(|_| windows::core::Error::empty())?;
        // SAFETY: the pointer and length describe `information`, a live local of
        // exactly the type `JobObjectExtendedLimitInformation` expects; the call
        // copies it and keeps nothing.
        unsafe {
            SetInformationJobObject(
                self.handle,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&information).cast(),
                length,
            )
        }
    }
}

#[cfg(windows)]
impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        // SAFETY: `handle` is the job handle this value owns, closed exactly once.
        // A failed close leaves nothing to recover: the process is ending anyway.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}
