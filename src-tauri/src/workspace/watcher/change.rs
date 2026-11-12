//! What a file event becomes: its kind, line delta, summary and preview reads.

use crate::platform::preview::preview_text;
use crate::platform::preview::MAXIMUM_PREVIEW_BYTES;
use notify::EventKind;
use serde::Serialize;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

/// The kind of filesystem change. Serializes to the exact lowercase wire
/// strings the frontend reads - the one authoritative home for those literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ChangeKind {
    Created,
    Modified,
    Deleted,
}

impl ChangeKind {
    /// Map a `notify` event to a change kind, ignoring events we don't surface.
    pub(super) fn from_event(kind: EventKind) -> Option<Self> {
        match kind {
            EventKind::Create(_) => Some(ChangeKind::Created),
            EventKind::Modify(_) => Some(ChangeKind::Modified),
            EventKind::Remove(_) => Some(ChangeKind::Deleted),
            _ => None,
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChangeEvent {
    pub(super) id: String,
    /// The held project root the change belongs to, exactly as the frontend
    /// spelled it in `watch_projects`.
    pub(super) root: String,
    pub(super) path: String,
    pub(super) kind: ChangeKind,
    pub(super) added: usize,
    pub(super) removed: usize,
    pub(super) summary: String,
    /// When the change surfaced, in epoch milliseconds.
    #[serde(rename = "ts")]
    pub(super) timestamp: u64,
}

/// Whether a change is worth surfacing, given whether its file still exists.
///
/// A created or modified path that is already gone was an atomic-write scratch
/// file: an editor or tool writes the new contents under a temporary name, then
/// renames it onto the real file. The scratch name only ever exists for an
/// instant, so by the time its diff is fetched the file is gone and the card can
/// only read "no preview available" - never a useful change. Detecting that by
/// its absence (rather than guessing temp-name shapes) catches every such file,
/// whatever a tool happens to call it. A deletion is the one case where the file
/// being gone *is* the event, so it always surfaces.
pub(super) fn surfaces(kind: ChangeKind, path_exists: bool) -> bool {
    matches!(kind, ChangeKind::Deleted) || path_exists
}

/// The change kind the card should carry, reconciled against the disk: a
/// "deleted" path that exists again was an atomic-save replace (delete +
/// rename-over), so it is a modification. Every other kind stands as reported.
pub(super) fn reclassify(kind: ChangeKind, path_exists: bool) -> ChangeKind {
    if matches!(kind, ChangeKind::Deleted) && path_exists {
        ChangeKind::Modified
    } else {
        kind
    }
}

/// The `(added, removed)` line delta between the last-seen count and the
/// current one. A file seen for the first time contributes all its lines as
/// added; a deletion of a tracked file removes every line it had; a deletion
/// never tracked contributes nothing.
pub(super) fn line_delta(old: Option<usize>, new: Option<usize>) -> (usize, usize) {
    match (old, new) {
        (Some(old_count), Some(new_count)) if new_count >= old_count => (new_count - old_count, 0),
        (Some(old_count), Some(new_count)) => (0, old_count - new_count),
        (None, Some(new_count)) => (new_count, 0),
        (Some(old_count), None) => (0, old_count),
        (None, None) => (0, 0),
    }
}

pub(super) fn line_count(path: &Path) -> Option<usize> {
    // `String::lines` was simple, but it read and allocated the complete file
    // for every watched write. Keep its UTF-8 failure semantics while streaming
    // through a reusable line buffer: source files with normal line lengths now
    // use bounded memory even when a formatter rewrites a large file.
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut count = 0;
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).ok()?;
        if bytes == 0 {
            return Some(count);
        }
        count += 1;
    }
}

/// Resolve `path` to its real target and require that target to remain under
/// `root`, so a symlink or reparse point can't turn a preview into a read of an
/// arbitrary file. `Ok(None)` when the path is gone.
pub(super) fn authorize_path(root: &Path, path: &Path) -> Result<Option<PathBuf>, ()> {
    let resolved = match std::fs::canonicalize(path) {
        Ok(resolved) => resolved,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    if !resolved.starts_with(root) {
        return Err(());
    }
    Ok(Some(resolved))
}

/// [`authorize_path`], then read through one handle. Metadata and content
/// therefore describe the same opened file, and a file that grows after the
/// metadata check is still bounded by the read cap.
pub(super) fn read_authorized_file(
    root: &Path,
    path: &Path,
    maximum_bytes: u64,
) -> Result<Option<Vec<u8>>, ()> {
    let Some(resolved) = authorize_path(root, path)? else {
        return Ok(None);
    };

    let mut file = std::fs::File::open(resolved).map_err(|_| ())?;
    let metadata = file.metadata().map_err(|_| ())?;
    if !metadata.is_file() || metadata.len() > maximum_bytes {
        return Err(());
    }

    let mut bytes = Vec::new();
    file.by_ref()
        .take(maximum_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if u64::try_from(bytes.len()).map_err(|_| ())? > maximum_bytes {
        return Err(());
    }
    Ok(Some(bytes))
}

/// Read an authorized `path` as UTF-8 preview text, rejecting binary content.
pub(super) fn read_preview_text(root: &Path, path: &Path) -> Result<Option<String>, ()> {
    let Some(bytes) = read_authorized_file(root, path, MAXIMUM_PREVIEW_BYTES)? else {
        return Ok(None);
    };
    preview_text(bytes).map(Some).ok_or(())
}

/// What happened to the file, for the line under its name on a feed card. The
/// name is deliberately absent: the card prints it directly above, and a summary
/// that repeated it made every card say the same thing twice.
pub(super) fn summarize(kind: ChangeKind, added: usize, removed: usize) -> String {
    match kind {
        ChangeKind::Created => "New file".to_string(),
        ChangeKind::Deleted => "Deleted".to_string(),
        ChangeKind::Modified => {
            if added > 0 && removed == 0 {
                format!("Grew by {added} line{}", plural(added))
            } else if removed > 0 && added == 0 {
                format!("Trimmed {removed} line{}", plural(removed))
            } else {
                "Edited".to_string()
            }
        }
    }
}

pub(super) fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}
