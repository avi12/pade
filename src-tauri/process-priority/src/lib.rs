//! Safe Windows process-priority boundary for PADE.
//!
//! PADE hosts agents that may launch compilers, dev servers, and other CPU-heavy
//! descendants. Lowering the parent before any child is created makes that whole
//! tree yield scheduler time to the user's foreground applications.

#[cfg(windows)]
use windows::Win32::System::Threading::{
    GetCurrentProcess, SetPriorityClass, BELOW_NORMAL_PRIORITY_CLASS,
};

/// Lower the current process to Windows' below-normal scheduling class.
/// Children created afterwards inherit the class unless they explicitly choose
/// another one.
#[cfg(windows)]
pub fn lower_current_process() -> windows::core::Result<()> {
    // SAFETY: `GetCurrentProcess` returns the caller's always-valid pseudo-handle;
    // `SetPriorityClass` does not retain it and accepts this documented class.
    unsafe { SetPriorityClass(GetCurrentProcess(), BELOW_NORMAL_PRIORITY_CLASS) }
}
