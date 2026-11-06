//! Whether this copy of PADE was installed from the release's `.msix`.
//!
//! A packaged copy lives in a read-only folder Windows owns. Windows updates it
//! (the `.appinstaller` feed), so the self-updater stands down, and its package
//! manifest declares the Explorer context menu, so the runtime sparse-package
//! registration stands down too. Everything that branches on it asks here.

use std::sync::OnceLock;

/// Whether the running process has MSIX package identity. Asked once: a
/// process cannot gain or lose its package while it runs.
pub(crate) fn is_packaged() -> bool {
    static IS_PACKAGED: OnceLock<bool> = OnceLock::new();
    *IS_PACKAGED.get_or_init(ask_windows)
}

#[cfg(windows)]
fn ask_windows() -> bool {
    package_identity::is_packaged()
}

#[cfg(not(windows))]
fn ask_windows() -> bool {
    false
}
