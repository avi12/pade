//! Safe Windows package-identity boundary for PADE.
//!
//! A copy installed from the release's `.msix` runs with package identity:
//! Windows owns its install folder (read-only), its updates (the
//! `.appinstaller` feed) and its Explorer context menu (declared in the
//! package manifest). PADE asks once here and stands its own updater and
//! menu registration down when the answer is yes.

#[cfg(windows)]
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, WIN32_ERROR};
#[cfg(windows)]
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

/// Whether the current process runs with MSIX package identity.
#[cfg(windows)]
#[must_use]
pub fn is_packaged() -> bool {
    let mut length = 0;
    // SAFETY: `length` is a valid, writable u32 for the call's duration, and no
    // buffer is passed - a zero length asks only whether there is a name (and
    // how long it is), so Windows writes nothing else.
    let answer = unsafe { GetCurrentPackageFullName(&raw mut length, None) };
    is_packaged_answer(answer)
}

/// Asked with an empty buffer, a packaged process is told the buffer is too
/// small for its package name; an unpackaged one is told there is no package
/// (`APPMODEL_ERROR_NO_PACKAGE`). Success cannot happen with an empty buffer
/// but means a name exists all the same. Every other answer is treated as
/// unpackaged, the state in which PADE manages itself.
#[cfg(windows)]
fn is_packaged_answer(answer: WIN32_ERROR) -> bool {
    answer == ERROR_INSUFFICIENT_BUFFER || answer == ERROR_SUCCESS
}

#[cfg(all(test, windows))]
mod tests {
    use super::is_packaged_answer;
    use windows::Win32::Foundation::{
        APPMODEL_ERROR_NO_PACKAGE, ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_PARAMETER,
        ERROR_SUCCESS,
    };

    #[test]
    fn a_package_name_that_does_not_fit_means_packaged() {
        assert!(is_packaged_answer(ERROR_INSUFFICIENT_BUFFER));
        assert!(is_packaged_answer(ERROR_SUCCESS));
    }

    #[test]
    fn no_package_or_any_failure_means_unpackaged() {
        assert!(!is_packaged_answer(APPMODEL_ERROR_NO_PACKAGE));
        assert!(!is_packaged_answer(ERROR_INVALID_PARAMETER));
    }

    #[test]
    fn a_test_process_is_unpackaged() {
        assert!(!super::is_packaged());
    }
}
