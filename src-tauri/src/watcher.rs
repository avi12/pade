//! Filesystem watcher feeding the Change Feed.
//!
//! MVP: watches the opened project, ignores build/VCS noise, and turns each
//! save into a ChangeEvent with a line-count delta and a heuristic summary.
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

#[derive(Default)]
pub struct WatcherState {
    watcher: Mutex<Option<RecommendedWatcher>>,
    line_counts: Mutex<HashMap<PathBuf, usize>>,
    last_seen: Mutex<HashMap<PathBuf, Instant>>,
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
    ".git", "node_modules", "target", "dist", "build", ".svelte-kit", ".ade", ".vite",
];

fn ignored(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| IGNORED.contains(&s))
            .unwrap_or(false)
    })
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

fn summarize(kind: &str, path: &Path, added: usize, removed: usize) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    match kind {
        "created" => format!("New file {name}"),
        "deleted" => format!("Deleted {name}"),
        _ => {
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
    if n == 1 { "" } else { "s" }
}

#[tauri::command]
pub fn watch_start(app: AppHandle, state: State<WatcherState>) -> Result<(), String> {
    let mut guard = state.watcher.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Ok(());
    }

    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let app_handle = app.clone();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };
        handle_event(&app_handle, event);
    })
    .map_err(|e| e.to_string())?;

    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    *guard = Some(watcher);
    Ok(())
}

fn handle_event(app: &AppHandle, event: Event) {
    let kind = match event.kind {
        EventKind::Create(_) => "created",
        EventKind::Modify(_) => "modified",
        EventKind::Remove(_) => "deleted",
        _ => return,
    };

    let state: State<WatcherState> = app.state();

    for path in event.paths {
        if ignored(&path) || path.is_dir() {
            continue;
        }

        // Debounce: editors emit bursts per save.
        {
            let mut seen = match state.last_seen.lock() {
                Ok(g) => g,
                Err(_) => continue,
            };
            let now = Instant::now();
            if let Some(prev) = seen.get(&path) {
                if now.duration_since(*prev) < Duration::from_millis(150) {
                    continue;
                }
            }
            seen.insert(path.clone(), now);
        }

        let new_count = line_count(&path);
        let (added, removed) = {
            let mut counts = match state.line_counts.lock() {
                Ok(g) => g,
                Err(_) => continue,
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
            kind: kind.to_string(),
            added,
            removed,
            ts: now_ms(),
        };
        let _ = app.emit("feed://change", ev);
    }
}

pub fn init(app: &AppHandle) {
    app.manage(WatcherState::default());
}
