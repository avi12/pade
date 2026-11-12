//! Filesystem operations that retry through the sharing races Windows raises on a folder another process just released.

use std::path::Path;
use std::time::Duration;

/// How many times to re-try a delete or rename that lost a race with Windows,
/// and how long to wait between attempts.
pub(super) const SHARING_RACE_ATTEMPTS: u32 = 10;

pub(super) const SHARING_RACE_RETRY_DELAY: Duration = Duration::from_millis(100);

/// Run `operation` until it succeeds or the attempts run out, sleeping between
/// tries - the one retry loop behind every folder delete and rename that can
/// race Windows closing a killed process's last handle.
pub(super) fn retry_through_sharing_race(
    mut operation: impl FnMut() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let mut attempt = 1;
    loop {
        match operation() {
            Ok(()) => return Ok(()),
            Err(error) if attempt == SHARING_RACE_ATTEMPTS => return Err(error),
            Err(_) => std::thread::sleep(SHARING_RACE_RETRY_DELAY),
        }
        attempt += 1;
    }
}

/// Step the process out of a folder it is about to lose. Opening a project points
/// the whole process at it - `workspace_open` chdirs - and on Windows the current
/// directory is an open handle: the OS refuses to delete the folder a process is
/// standing in, however many agents have been killed. So walk out to its parent
/// first. (The project is on its way out; the frontend drops it at the same time.)
pub(super) fn leave_if_inside(path: &Path) {
    let Ok(cwd) = std::env::current_dir() else {
        return;
    };
    let is_inside = std::fs::canonicalize(cwd)
        .ok()
        .zip(std::fs::canonicalize(path).ok())
        .is_some_and(|(here, doomed)| here.starts_with(&doomed));
    if !is_inside {
        return;
    }

    if let Some(parent) = path.parent() {
        let _ = std::env::set_current_dir(parent);
    }
}

/// Remove a directory tree, riding out a Windows sharing violation. The agents
/// holding the folder are killed before this runs, but the OS closes their
/// handles asynchronously: a process can be gone while its last handle is still
/// open, and a delete in that window fails with "used by another process".
/// Retrying briefly turns that race into a wait instead of an error.
pub(super) fn remove_dir_all_patiently(path: &Path) -> std::io::Result<()> {
    retry_through_sharing_race(|| match std::fs::remove_dir_all(path) {
        // Already gone (deleted outside PADE): the folder is in the state the
        // caller asked for, so this is a success - and the entry still on the
        // Recent list gets forgotten instead of being stuck there forever.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        outcome => outcome,
    })
}

/// Rename a directory, riding out the same Windows sharing violation a delete
/// does. Moving or promoting a temp workspace hits it for the same reasons: the
/// folder is the process's own current directory (`workspace_open` chdirs into
/// it), or an agent PTY child just killed still has a handle the OS is closing
/// asynchronously. The caller must step the process out first (`leave_if_inside`);
/// retrying briefly then turns the lingering-handle race into a wait, not an error.
pub(super) fn rename_patiently(from: &Path, destination: &Path) -> std::io::Result<()> {
    retry_through_sharing_race(|| std::fs::rename(from, destination))
}
