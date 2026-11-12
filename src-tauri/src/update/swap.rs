//! Swapping a downloaded exe in for the running one, and clearing what an earlier swap set aside.

use super::download::is_digest_of;
use std::path::{Path, PathBuf};

pub(super) const STAGED_SUFFIX: &str = ".new";

pub(super) const RETIRED_MARKER: &str = ".old-";

/// Swap `downloaded` in for `running`. The new exe is copied beside the running
/// one first, so the swap itself is a rename on one volume and a copy cut short
/// never leaves half an exe under PADE's name; on failure the running exe is
/// put back. A running exe already carrying the digest (another PADE window
/// swapped first) is left as it is.
pub(super) fn install(downloaded: &Path, running: &Path, sha256: &str) -> Result<(), String> {
    if is_digest_of(running, sha256) {
        return Ok(());
    }
    let staged = suffixed(running, STAGED_SUFFIX);
    std::fs::copy(downloaded, &staged).map_err(|error| error.to_string())?;
    if !is_digest_of(&staged, sha256) {
        let _ = std::fs::remove_file(&staged);
        return Err("the staged update does not match its digest".into());
    }
    let retired = suffixed(
        running,
        &format!("{RETIRED_MARKER}{}", crate::util::now_milliseconds()),
    );
    std::fs::rename(running, &retired).map_err(|error| error.to_string())?;
    if let Err(error) = std::fs::rename(&staged, running) {
        let _ = std::fs::rename(&retired, running);
        return Err(error.to_string());
    }
    Ok(())
}

pub(super) fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Delete what an earlier swap set aside. After a restart the retired exe no
/// longer runs; one another PADE window still runs stays until a later start.
pub(super) fn delete_leftovers(running: &Path) {
    let (Some(folder), Some(name)) = (running.parent(), running.file_name()) else {
        return;
    };
    let name = name.to_string_lossy();
    let retired_prefix = format!("{name}{RETIRED_MARKER}");
    let staged_name = format!("{name}{STAGED_SUFFIX}");
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let is_leftover = file_name.starts_with(&retired_prefix) || file_name == staged_name;
        if is_leftover {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
