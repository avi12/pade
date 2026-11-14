//! Safe Windows process-tree reads for PADE's task tracking.
//!
//! [`processes`] lists every running process with its parent, from one Toolhelp
//! snapshot; [`command_line`] reads one process's command line. A caller walks
//! the tree from the snapshot and asks for command lines only where it needs
//! them, so a question about one agent's few descendants never reads the whole
//! system's.

#[cfg(windows)]
use std::ffi::c_void;

#[cfg(windows)]
use windows::Wdk::System::Threading::{NtQueryInformationProcess, ProcessCommandLineInformation};
#[cfg(windows)]
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, STATUS_INFO_LENGTH_MISMATCH, UNICODE_STRING,
};
#[cfg(windows)]
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

/// One running process and the process that started it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessEntry {
    /// The process id.
    pub process_id: u32,
    /// The id of the process that started it.
    pub parent_process_id: u32,
}

/// A handle closed when dropped.
#[cfg(windows)]
struct OwnedHandle(HANDLE);

#[cfg(windows)]
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was returned open by the call that created this
        // wrapper and is closed exactly once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Every running process with its parent, from one system snapshot.
#[cfg(windows)]
pub fn processes() -> windows::core::Result<Vec<ProcessEntry>> {
    // SAFETY: a process snapshot takes no process id; the handle is owned and
    // closed by `OwnedHandle`.
    let snapshot = OwnedHandle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? });
    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(size_of::<PROCESSENTRY32W>()).unwrap_or(u32::MAX),
        ..Default::default()
    };
    let mut found = Vec::new();
    // SAFETY: `entry` is a live, correctly sized PROCESSENTRY32W for the call to
    // fill, and `snapshot` stays open for the whole walk.
    let mut has_entry = unsafe { Process32FirstW(snapshot.0, &raw mut entry) }.is_ok();
    while has_entry {
        found.push(ProcessEntry {
            process_id: entry.th32ProcessID,
            parent_process_id: entry.th32ParentProcessID,
        });
        // SAFETY: as above.
        has_entry = unsafe { Process32NextW(snapshot.0, &raw mut entry) }.is_ok();
    }
    Ok(found)
}

/// A process's command line, `None` when the process is gone or not ours to
/// read.
#[cfg(windows)]
#[must_use]
pub fn command_line(process_id: u32) -> Option<String> {
    // SAFETY: opening a process for limited queries has no preconditions; the
    // handle is owned and closed by `OwnedHandle`.
    let process = OwnedHandle(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }.ok()?,
    );
    let mut length = 0u32;
    // SAFETY: a zero-length query writes nothing but the length it needs.
    let status = unsafe {
        NtQueryInformationProcess(
            process.0,
            ProcessCommandLineInformation,
            std::ptr::null_mut(),
            0,
            &raw mut length,
        )
    };
    if status != STATUS_INFO_LENGTH_MISMATCH {
        return None;
    }

    // u64 cells keep the UNICODE_STRING header at the start correctly aligned.
    let cells = usize::try_from(length).ok()?.div_ceil(size_of::<u64>());
    let mut buffer = vec![0u64; cells];
    // SAFETY: `buffer` holds at least `length` writable bytes.
    let status = unsafe {
        NtQueryInformationProcess(
            process.0,
            ProcessCommandLineInformation,
            buffer.as_mut_ptr().cast::<c_void>(),
            length,
            &raw mut length,
        )
    };
    if status.is_err() {
        return None;
    }

    // SAFETY: on success the buffer starts with a UNICODE_STRING whose `Buffer`
    // points `Length` bytes of UTF-16 further into this same buffer.
    let text = unsafe {
        let header = &*buffer.as_ptr().cast::<UNICODE_STRING>();
        std::slice::from_raw_parts(
            header.Buffer.0,
            usize::from(header.Length) / size_of::<u16>(),
        )
    };
    Some(String::from_utf16_lossy(text))
}

#[cfg(all(test, windows))]
mod tests {
    use super::{command_line, processes};

    #[test]
    fn the_snapshot_holds_this_process_and_its_parent() {
        let own = std::process::id();
        let entry = processes()
            .expect("a snapshot")
            .into_iter()
            .find(|entry| entry.process_id == own)
            .expect("this process is listed");
        assert_ne!(entry.parent_process_id, 0);
    }

    #[test]
    fn reads_this_process_command_line() {
        let line = command_line(std::process::id()).expect("our own command line");
        assert!(line.contains("process_snapshot"));
    }

    #[test]
    fn a_process_that_is_gone_has_no_command_line() {
        assert_eq!(command_line(u32::MAX), None);
    }
}
