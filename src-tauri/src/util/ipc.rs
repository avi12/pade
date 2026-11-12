//! Limits and ownership shared by the IPC registries (terminal sessions and task runners).

/// The longest session id an IPC request may carry. Shared by every registry
/// that validates one (terminal sessions and task runners), so an id one
/// accepts is never rejected by the other.
pub(crate) const MAXIMUM_SESSION_ID_BYTES: usize = 128;

/// The longest path or path-like argument an IPC request may carry, so a
/// malformed call can never hand the OS an unbounded command line.
pub(crate) const MAXIMUM_PATH_BYTES: usize = 4096;

/// Whether a caller may use an id in a process-global resource registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OwnerAccess {
    Vacant,
    Owned,
    Foreign,
}

/// Decide access without exposing one window's resource to another. Window
/// labels survive a `WebView` reload, so `Owned` also permits reload re-adoption.
pub(crate) fn owner_access(owner: Option<&str>, requester: &str) -> OwnerAccess {
    match owner {
        None => OwnerAccess::Vacant,
        Some(owner) if owner == requester => OwnerAccess::Owned,
        Some(_) => OwnerAccess::Foreign,
    }
}
