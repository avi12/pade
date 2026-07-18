//! Filesystem watcher feeding the Change Feed.
//!
//! MVP: watches the opened project, ignores build/VCS noise, and turns each
//! save into a `ChangeEvent` with a line-count delta and a heuristic summary.
//! Later: real per-hunk diffs and agent-authored intent replace the heuristic.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// The Change Feed's live watcher plus the root it is armed on. The root is what
/// lets `watch_start` detect a project switch and re-arm; the watcher handle is
/// held only so dropping it stops the watch.
struct ProjectWatcher {
    root: PathBuf,
    _watcher: RecommendedWatcher,
}

#[derive(Default)]
pub struct WatcherState {
    watcher: Mutex<Option<ProjectWatcher>>,
    /// The picker's watcher — see `watch_dirs`. Separate from the Change Feed's:
    /// it watches other folders, for a different question, and is re-armed as the
    /// picker's list changes.
    dirs: Mutex<Option<RecommendedWatcher>>,
    line_counts: Mutex<HashMap<PathBuf, usize>>,
    last_seen: Mutex<HashMap<PathBuf, Instant>>,
    /// First-touch baseline snapshots for the git-free preview, keyed by absolute
    /// path. `Some(text)` is the content the path held the first time it changed
    /// this watch session (empty for a file created this session, so its diff is a
    /// full addition); `None` records a path seen but not snapshottable (binary,
    /// over the size cap, or already gone), so it is never re-read and the preview
    /// falls back to "none". Cleared and re-scoped to the new root on a project
    /// switch (see `watch_start`).
    baselines: Mutex<HashMap<PathBuf, Option<String>>>,
}

/// The kind of filesystem change, in the exact wire strings the frontend reads.
/// One authoritative home for the `"created"`/`"modified"`/`"deleted"` literals.
#[derive(Clone, Copy)]
enum ChangeKind {
    Created,
    Modified,
    Deleted,
}

impl ChangeKind {
    /// Map a `notify` event to a change kind, ignoring events we don't surface.
    fn from_event(kind: EventKind) -> Option<Self> {
        match kind {
            EventKind::Create(_) => Some(ChangeKind::Created),
            EventKind::Modify(_) => Some(ChangeKind::Modified),
            EventKind::Remove(_) => Some(ChangeKind::Deleted),
            _ => None,
        }
    }

    /// The serialized string for this kind — the only place the literals live.
    fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Created => "created",
            ChangeKind::Modified => "modified",
            ChangeKind::Deleted => "deleted",
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChangeEvent {
    id: String,
    path: String,
    kind: String,
    added: usize,
    removed: usize,
    summary: String,
    ts: u128,
}

const IGNORED: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".svelte-kit",
    ".ade",
    ".vite",
];

fn ignored(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().to_str().is_some_and(|s| IGNORED.contains(&s)))
}

/// Whether a change is worth surfacing, given whether its file still exists.
///
/// A created or modified path that is already gone was an atomic-write scratch
/// file: an editor or tool writes the new contents under a temporary name, then
/// renames it onto the real file. The scratch name only ever exists for an
/// instant, so by the time its diff is fetched the file is gone and the card can
/// only read "no preview available" — never a useful change. Detecting that by
/// its absence (rather than guessing temp-name shapes) catches every such file,
/// whatever a tool happens to call it. A deletion is the one case where the file
/// being gone *is* the event, so it always surfaces.
fn surfaces(kind: ChangeKind, path_exists: bool) -> bool {
    matches!(kind, ChangeKind::Deleted) || path_exists
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn line_count(path: &Path) -> Option<usize> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.lines().count())
}

/// Largest file the Change Feed will snapshot for a baseline or read for a live
/// preview. A few hundred KB covers real source files; past it the preview falls
/// back to "No preview available" rather than holding megabytes per touched path.
const MAX_PREVIEW_BYTES: u64 = 512 * 1024;

/// Read `path` as UTF-8 text for the Change Feed preview, or `None` when it is
/// missing, not a regular file, larger than [`MAX_PREVIEW_BYTES`], or binary (it
/// holds a NUL byte or isn't valid UTF-8). One helper for both the first-touch
/// baseline capture and the current-content read (DRY), so both honor the same
/// size and binary sensibilities.
fn read_preview_text(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_PREVIEW_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn summarize(kind: ChangeKind, path: &Path, added: usize, removed: usize) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    match kind {
        ChangeKind::Created => format!("New file {name}"),
        ChangeKind::Deleted => format!("Deleted {name}"),
        ChangeKind::Modified => {
            if added > 0 && removed == 0 {
                format!("Grew {name} by {added} line{}", plural(added))
            } else if removed > 0 && added == 0 {
                format!("Trimmed {removed} line{} from {name}", plural(removed))
            } else {
                format!("Edited {name}")
            }
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Start (or re-root) the Change Feed's watcher on the current workspace.
/// Idempotent per workspace: a repeat call for the same root keeps the live
/// watcher, while a call after a project switch drops the old project's watcher
/// and re-arms on the new root — the feed always follows the open workspace.
#[tauri::command]
pub fn watch_start(app: AppHandle, state: State<WatcherState>) -> Result<(), String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let mut guard = state.watcher.lock().map_err(|e| e.to_string())?;
    let already_watching_root = guard.as_ref().is_some_and(|active| active.root == root);
    if already_watching_root {
        return Ok(());
    }

    // Drop the previous project's watcher and its per-file bookkeeping before
    // re-arming, so its handles and stale line counts go with it.
    *guard = None;
    if let Ok(mut counts) = state.line_counts.lock() {
        counts.clear();
    }
    if let Ok(mut seen) = state.last_seen.lock() {
        seen.clear();
    }
    if let Ok(mut baselines) = state.baselines.lock() {
        baselines.clear();
    }

    let app_handle = app.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };
        handle_event(&app_handle, event);
    })
    .map_err(|e| e.to_string())?;

    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    *guard = Some(ProjectWatcher {
        root,
        _watcher: watcher,
    });
    Ok(())
}

/// Watch a set of folders (non-recursively) and report when anything inside one
/// appears or disappears — the project picker's eyes. It hands over the *parents*
/// of the rows it shows, never the rows themselves: a watch holds a handle on the
/// folder it watches, and a handle on a project would be the very thing stopping
/// that project from being deleted. Re-arming replaces the previous set.
#[tauri::command]
pub fn watch_dirs(
    app: AppHandle,
    state: State<WatcherState>,
    dirs: Vec<String>,
) -> Result<(), String> {
    let mut guard = state.dirs.lock().map_err(|e| e.to_string())?;
    // Drop the old watcher before opening the new one, so its handles go with it.
    *guard = None;
    if dirs.is_empty() {
        return Ok(());
    }

    let app_handle = app.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };
        // Only appearance/disappearance moves rows on the page; edits inside a
        // project are the Change Feed's business, not the picker's.
        if matches!(event.kind, EventKind::Create(_) | EventKind::Remove(_)) {
            let _ = app_handle.emit("dirs://changed", ());
        }
    })
    .map_err(|e| e.to_string())?;

    for dir in dirs {
        // A folder that is already gone isn't an error — that's precisely the news
        // the caller is listening for, and the next re-arm won't ask for it again.
        let _ = watcher.watch(Path::new(&dir), RecursiveMode::NonRecursive);
    }

    *guard = Some(watcher);
    Ok(())
}

fn handle_event(app: &AppHandle, event: Event) {
    let Some(kind) = ChangeKind::from_event(event.kind) else {
        return;
    };

    let state: State<WatcherState> = app.state();

    for path in event.paths {
        if ignored(&path) || path.is_dir() {
            continue;
        }
        // A created/modified file that has already vanished was an atomic-write
        // scratch file — skip it rather than surface a preview-less phantom card.
        if !surfaces(kind, path.exists()) {
            continue;
        }

        // Debounce: editors emit bursts per save.
        {
            let Ok(mut seen) = state.last_seen.lock() else {
                continue;
            };
            let now = Instant::now();
            let within_debounce = seen
                .get(&path)
                .is_some_and(|prev| now.duration_since(*prev) < Duration::from_millis(150));
            if within_debounce {
                continue;
            }
            seen.insert(path.clone(), now);
        }

        // First-touch baseline for the git-free preview: the content this path
        // held the first time it changed this session. A creation baselines as
        // empty (its whole content is new this session); any other first sighting
        // snapshots the current on-disk text — the accepted slightly-late baseline
        // (the edit that fired this very event is already on disk). Later edits,
        // and a later deletion, diff against this. `or_insert_with` runs only on
        // the first sighting, so a large file is read at most once.
        if let Ok(mut baselines) = state.baselines.lock() {
            baselines.entry(path.clone()).or_insert_with(|| match kind {
                ChangeKind::Created => Some(String::new()),
                ChangeKind::Modified | ChangeKind::Deleted => read_preview_text(&path),
            });
        }

        let new_count = line_count(&path);
        let (added, removed) = {
            let Ok(mut counts) = state.line_counts.lock() else {
                continue;
            };
            let old = counts.get(&path).copied();
            let (a, r) = match (old, new_count) {
                (Some(o), Some(n)) if n >= o => (n - o, 0),
                (Some(o), Some(n)) => (0, o - n),
                (None, Some(n)) => (n, 0),
                _ => (0, 0),
            };
            if let Some(n) = new_count {
                counts.insert(path.clone(), n);
            } else {
                counts.remove(&path);
            }
            (a, r)
        };

        let path_str = path.to_string_lossy().to_string();
        let id = format!("{}-{}", now_ms(), COUNTER.fetch_add(1, Ordering::Relaxed));
        let ev = ChangeEvent {
            id,
            summary: summarize(kind, &path, added, removed),
            path: path_str,
            kind: kind.as_str().to_string(),
            added,
            removed,
            ts: now_ms(),
        };
        let _ = app.emit("feed://change", ev);
    }
}

/// The two texts a Change Feed card diffs: the session's first-touch baseline and
/// the file's current content. The frontend renders the unified diff from these.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedDiff {
    /// First-touch baseline content (empty for a file created this session).
    before: String,
    /// Current on-disk content (empty when the file is now deleted).
    after: String,
}

/// The session-baseline diff inputs for `path`: its first-touch baseline
/// (`before`) and current on-disk content (`after`). `None` when no baseline was
/// captured this session, or when the baseline or the current file is binary /
/// over the size cap — the card then shows "No preview available". Git-independent
/// by construction, so an untracked or ignored file previews like any other, and a
/// file deleted after being touched shows its baseline as a full removal.
#[tauri::command]
pub fn feed_diff(state: State<WatcherState>, path: String) -> Result<Option<FeedDiff>, String> {
    let before = {
        let baselines = state.baselines.lock().map_err(|e| e.to_string())?;
        match baselines.get(Path::new(&path)) {
            Some(Some(text)) => text.clone(),
            _ => return Ok(None),
        }
    };

    let file = Path::new(&path);
    let after = if file.exists() {
        let Some(text) = read_preview_text(file) else {
            return Ok(None);
        };
        text
    } else {
        String::new()
    };

    Ok(Some(FeedDiff { before, after }))
}

pub fn init(app: &AppHandle) {
    app.manage(WatcherState::default());
}

#[cfg(test)]
mod tests {
    use super::{read_preview_text, surfaces, ChangeKind, MAX_PREVIEW_BYTES};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn a_created_or_modified_file_that_vanished_is_not_surfaced() {
        // An atomic-write scratch file is renamed away before it can be diffed.
        assert!(!surfaces(ChangeKind::Created, false));
        assert!(!surfaces(ChangeKind::Modified, false));
    }

    #[test]
    fn a_created_or_modified_file_that_still_exists_is_surfaced() {
        assert!(surfaces(ChangeKind::Created, true));
        assert!(surfaces(ChangeKind::Modified, true));
    }

    #[test]
    fn a_deletion_always_surfaces_since_the_file_being_gone_is_the_event() {
        assert!(surfaces(ChangeKind::Deleted, false));
    }

    /// A scratch directory unique to this test process; removed by the caller.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pade-watcher-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn read_preview_text_returns_utf8_file_contents() {
        let dir = scratch("text");
        let file = dir.join("a.txt");
        fs::write(&file, b"line one\nline two\n").expect("write file");
        assert_eq!(
            read_preview_text(&file).as_deref(),
            Some("line one\nline two\n")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_preview_text_skips_binary_content() {
        let dir = scratch("binary");
        let file = dir.join("a.bin");
        fs::write(&file, b"before\x00after").expect("write file");
        assert_eq!(read_preview_text(&file), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_preview_text_skips_files_over_the_cap() {
        let dir = scratch("large");
        let file = dir.join("a.txt");
        let over_cap = usize::try_from(MAX_PREVIEW_BYTES).expect("cap fits usize") + 1;
        fs::write(&file, vec![b'x'; over_cap]).expect("write file");
        assert_eq!(read_preview_text(&file), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_preview_text_is_none_for_a_missing_path() {
        let dir = scratch("missing");
        assert_eq!(read_preview_text(&dir.join("nope.txt")), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
