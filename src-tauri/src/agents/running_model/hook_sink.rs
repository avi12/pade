//! The executable an agent's hooks run to reach PADE's sink - kept at one
//! stable path the app refreshes to its own build, so a hook always runs the
//! current sink.
//!
//! An agent fixes its hook commands at launch, and the PTY host builds them -
//! a long-lived copy of whichever build started it. Pointing a hook at the
//! host's own executable froze the sink at that build for the whole life of
//! every session the host launched, even sessions launched after an update.
//! So hooks point here instead: `<PADE data>/hook-sink/pade-hook-sink.exe`, a
//! copy the app refreshes when it starts (`refresh`). A session launched last
//! week runs this week's sink logic, whatever host launched it.
//!
//! A copy, not the app's executable itself: hooks run on every tool call of
//! every session, and an executable being run is locked on Windows - a rebuild
//! (`cargo` linking `pade.exe`) or an update would fail whenever a hook ran at
//! that moment. A packaged (MSIX) install also moves its executable to a new
//! versioned folder on every update. The copy is replaced by renaming the old
//! one aside first, which Windows allows even while a hook runs it.
//!
//! The newest build wins: several PADE builds on one machine (an installed
//! release, a dev checkout) each refresh the copy only when theirs is newer.

use std::io;
use std::path::{Path, PathBuf};

/// PADE's data subfolder holding the stable sink.
const SINK_DIRECTORY_NAME: &str = "hook-sink";
/// The sink's file stem; the platform's executable suffix follows.
const SINK_FILE_STEM: &str = "pade-hook-sink";
/// A replaced copy is renamed to `<stem>.retired-<id>` until it can be deleted.
const RETIRED_MARKER: &str = ".retired-";
/// A copy in progress, renamed into place only once it is whole.
const PARTIAL_MARKER: &str = ".partial-";

/// The executable a hook command runs: the stable sink when the app has placed
/// it, else this process's own executable (a host started before any app
/// placed one, or a test).
pub(super) fn executable() -> Option<PathBuf> {
    stable_path()
        .filter(|path| path.is_file())
        .or_else(|| std::env::current_exe().ok())
}

/// Bring the stable sink up to this build, unless a newer build placed it.
/// Called by the app (never the host or the sink) once at startup, off the UI
/// thread: a debug build's copy takes a moment.
pub(super) fn refresh() {
    let Some(target) = stable_path() else {
        return;
    };
    let source = match std::env::current_exe() {
        Ok(source) => source,
        Err(error) => {
            eprintln!("pade: no executable to place as the hook sink: {error}");
            return;
        }
    };
    if let Err(error) = place(&source, &target) {
        eprintln!("pade: could not refresh the hook sink: {error}");
    }
    prune_retired(&target);
}

fn stable_path() -> Option<PathBuf> {
    let file_name = format!("{SINK_FILE_STEM}{}", std::env::consts::EXE_SUFFIX);
    Some(
        crate::util::pade_data_directory()?
            .join(SINK_DIRECTORY_NAME)
            .join(file_name),
    )
}

/// Copy `source` to `target` when `source` is newer, keeping its write time so
/// the next comparison sees the same build as current.
fn place(source: &Path, target: &Path) -> io::Result<()> {
    let source_metadata = std::fs::metadata(source)?;
    let source_written = source_metadata.modified()?;
    let target_metadata = std::fs::metadata(target).ok();
    let is_current = target_metadata.as_ref().is_some_and(|metadata| {
        metadata.len() == source_metadata.len()
            && metadata
                .modified()
                .is_ok_and(|written| written == source_written)
    });
    let is_newer_build_placed = target_metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .is_some_and(|written| written > source_written);
    if is_current || is_newer_build_placed {
        return Ok(());
    }

    let directory = target
        .parent()
        .ok_or_else(|| io::Error::other("the hook sink has no folder"))?;
    std::fs::create_dir_all(directory)?;
    let partial = marked(target, PARTIAL_MARKER);
    std::fs::copy(source, &partial)?;
    std::fs::File::options()
        .write(true)
        .open(&partial)?
        .set_modified(source_written)?;

    if target_metadata.is_some() {
        // A hook may be running the old copy: renaming it aside is allowed,
        // replacing it in place is not.
        std::fs::rename(target, marked(target, RETIRED_MARKER))?;
    }
    std::fs::rename(&partial, target)
}

/// `target` renamed with `marker` and a fresh id, beside it.
fn marked(target: &Path, marker: &str) -> PathBuf {
    target.with_file_name(format!(
        "{SINK_FILE_STEM}{marker}{}",
        uuid::Uuid::new_v4().simple()
    ))
}

/// Delete retired copies and abandoned partial ones. One a hook still runs is
/// locked, so its deletion fails and it waits for the next start.
fn prune_retired(target: &Path) {
    let Some(directory) = target.parent() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let is_leftover = name.to_str().is_some_and(|name| {
            name.starts_with(&format!("{SINK_FILE_STEM}{RETIRED_MARKER}"))
                || name.starts_with(&format!("{SINK_FILE_STEM}{PARTIAL_MARKER}"))
        });
        if is_leftover {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{marked, place, prune_retired, RETIRED_MARKER};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("pade-hook-sink-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("scratch folder");
        directory
    }

    fn written_at(path: &PathBuf, seconds: u64) {
        std::fs::File::options()
            .write(true)
            .open(path)
            .expect("open")
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .expect("dated");
    }

    #[test]
    fn a_newer_build_replaces_the_sink_and_retires_the_old_one() {
        let directory = scratch("newer");
        let source = directory.join("pade.exe");
        let target = directory.join("sink").join("pade-hook-sink.exe");
        std::fs::write(&source, "build one").expect("source");
        written_at(&source, 1_000);
        place(&source, &target).expect("placed");
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "build one");

        std::fs::write(&source, "build two!").expect("source");
        written_at(&source, 2_000);
        place(&source, &target).expect("replaced");
        assert_eq!(
            std::fs::read_to_string(&target).expect("read"),
            "build two!"
        );
        let retired = std::fs::read_dir(target.parent().expect("folder"))
            .expect("listed")
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains(RETIRED_MARKER))
            .count();
        assert_eq!(retired, 1);
        prune_retired(&target);
        assert_eq!(
            std::fs::read_dir(target.parent().expect("folder"))
                .expect("listed")
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_older_build_leaves_a_newer_sink_alone() {
        let directory = scratch("older");
        let newer = directory.join("newer.exe");
        let older = directory.join("older.exe");
        let target = directory.join("pade-hook-sink.exe");
        std::fs::write(&newer, "newer").expect("newer");
        written_at(&newer, 2_000);
        std::fs::write(&older, "older build").expect("older");
        written_at(&older, 1_000);
        place(&newer, &target).expect("placed");
        place(&older, &target).expect("skipped");
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "newer");
        assert!(marked(&target, RETIRED_MARKER)
            .to_string_lossy()
            .contains("pade-hook-sink.retired-"));
        let _ = std::fs::remove_dir_all(&directory);
    }
}
