//! Per-window, per-project watch state and the registry that owns it.

use super::ignore::IgnoreRules;
use super::languages::LanguageRoute;
use notify::RecommendedWatcher;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use tauri::AppHandle;
use tauri::Manager;

/// One held project's Change Feed watch and its per-file bookkeeping. A window
/// holds sessions of several projects, and an agent edits whichever project its
/// session lives in - front or not - so a window watches every project it holds
/// (see [`watch_projects`]), one `ProjectWatch` each. The roots are fixed for
/// the watch's lifetime: a project the window lets go of drops its whole
/// `ProjectWatch` (watchers, baselines, counters), and one it picks up again
/// starts fresh.
pub(super) struct ProjectWatch {
    /// The owning window's pub(super) label: every event this watch detects is routed back
    /// to that window alone (`emit_to`), never broadcast to its siblings.
    pub(super) label: String,
    /// The root exactly as the frontend spelled it in `watch_projects` - what
    /// every per-project event carries, so the frontend keys by its own spelling.
    pub(super) requested_root: String,
    /// The normalized root the watch is armed on (see [`resolve_watch_root`]).
    pub(super) root: PathBuf,
    /// The canonical filesystem target of `root`, which preview reads are
    /// confined to and a folder release matches against.
    pub(super) canonical_root: PathBuf,
    /// When this watch was armed (epoch ms) - the "watching since" the feed
    /// states in its empty panel, since the feed only ever holds changes made
    /// after that moment (nothing replays what happened before it).
    pub(super) armed_at: u64,
    /// The recursive notify watcher, installed once right after the watch is
    /// built (its callback holds a `Weak` back to this watch). Held only so
    /// dropping it stops the watch.
    pub(super) watcher: Mutex<Option<RecommendedWatcher>>,
    /// The project's live-git-state watcher - see `arm_git_state`. Separate
    /// from the Change Feed's recursive watcher on purpose: it watches only the
    /// dir(s) holding `.git/HEAD` + `.git/config`, non-recursively, so git's
    /// object churn never reaches it. Dropping it drops its channel sender,
    /// which retires the paired coalescer thread once that thread drains.
    pub(super) git_state: Mutex<Option<RecommendedWatcher>>,
    pub(super) line_counts: Mutex<HashMap<PathBuf, usize>>,
    pub(super) last_seen: Mutex<HashMap<PathBuf, Instant>>,
    /// First-touch baseline snapshots for the git-free preview, keyed by absolute
    /// path. `Some(text)` is the content the path held the first time it changed
    /// this watch session (empty for a file created this session, so its diff is a
    /// full addition); `None` records a path seen but not snapshottable (binary,
    /// over the size cap, or already gone), so it is never re-read and the preview
    /// falls back to "none".
    pub(super) baselines: Mutex<HashMap<PathBuf, Option<String>>>,
    /// How this root decides a path is ignored, computed when the watch is built
    /// and recomputed whenever the rules change (see [`IgnoreRules`]).
    pub(super) ignore_rules: IgnoreRules,
    /// The last parseable state of each registered MCP config. Existence and
    /// server membership drive automatic handoff; same-membership content edits
    /// arm frontend reload-failure recovery.
    pub(super) mcp_servers: Mutex<HashMap<PathBuf, crate::workspace::mcp::McpSnapshot>>,
    /// Changes surfaced to the window since the watch was armed, and changes the
    /// ignore rules dropped. An empty feed is otherwise ambiguous - a quiet
    /// project and a watch whose every event is being filtered look identical -
    /// so [`watch_status`] reports both and the panel says which it is. Atomics,
    /// not a lock: the notify callback bumps them on every event it handles.
    pub(super) surfaced: AtomicU64,
    pub(super) ignored: AtomicU64,
}

/// One window's Change Feed watches: one [`ProjectWatch`] per held project root,
/// keyed by the root as the frontend spelled it. Every PADE window opens its own
/// workspace, but all windows share one backend process, so this state is held
/// per window (keyed by window label in [`WatcherState`]); each window's events
/// are emitted back only to it.
#[derive(Default)]
pub(super) struct WindowWatches {
    pub(super) projects: Mutex<HashMap<String, Arc<ProjectWatch>>>,
    /// The ticket of the newest `watch_projects` request. `watch_projects` asks
    /// git about new roots before it takes the projects lock, so two requests
    /// can finish out of order; only the holder of the newest ticket may apply
    /// its set, which keeps "the latest set wins" on a quick switch.
    latest_arm_request: AtomicU64,
}

impl WindowWatches {
    /// Take a ticket for a new arming request, superseding every earlier one.
    pub(super) fn begin_arm_request(&self) -> u64 {
        self.latest_arm_request.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Whether no newer arming request has started since `ticket` was taken.
    pub(super) fn is_latest_arm_request(&self, ticket: u64) -> bool {
        self.latest_arm_request.load(Ordering::SeqCst) == ticket
    }

    /// A snapshot of this window's project watches, cloned out under a brief
    /// lock so callers never hold it across I/O. Empty when the lock is poisoned.
    pub(super) fn snapshot(&self) -> Vec<Arc<ProjectWatch>> {
        self.projects
            .lock()
            .map(|projects| projects.values().cloned().collect())
            .unwrap_or_default()
    }

    /// The watch on `root`: the one keyed by that exact spelling, else the one
    /// armed on the same normalized root.
    pub(super) fn find(&self, root: &str) -> Option<Arc<ProjectWatch>> {
        let projects = self.projects.lock().ok()?;
        if let Some(project) = projects.get(root) {
            return Some(Arc::clone(project));
        }
        let normalized: PathBuf = Path::new(root).components().collect();
        projects
            .values()
            .find(|project| project.root == normalized)
            .cloned()
    }

    /// The watch whose root contains `path` - the deepest one when held roots
    /// nest - which is the watch that owns that path's previews and ignore rules.
    pub(super) fn owner_of(&self, path: &Path) -> Option<Arc<ProjectWatch>> {
        owning_project(&self.snapshot(), path).cloned()
    }
}

/// The project among `projects` that owns `path`: the deepest whose root
/// contains it.
pub(super) fn owning_project<'a>(
    projects: &'a [Arc<ProjectWatch>],
    path: &Path,
) -> Option<&'a Arc<ProjectWatch>> {
    deepest_containing(
        projects
            .iter()
            .map(|project| (project.root.as_path(), project)),
        path,
    )
}

/// The value paired with the deepest root that contains `path`, or `None` when
/// no root does. Nested held projects (a package inside a monorepo) both
/// contain a file of the inner one; the inner, more specific project owns it.
pub(super) fn deepest_containing<'a, T>(
    candidates: impl IntoIterator<Item = (&'a Path, T)>,
    path: &Path,
) -> Option<T> {
    candidates
        .into_iter()
        .filter(|(root, _)| path.starts_with(root))
        .max_by_key(|(root, _)| root.components().count())
        .map(|(_, value)| value)
}

/// What a declarative `watch_projects` set asks of the current watches: the
/// requested roots not yet watched (to arm, deduplicated, in request order) and
/// the watched roots no longer requested (to drop). A root in both sets is left
/// alone, so its baselines and counters survive.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct WatchSetChange {
    pub(super) to_arm: Vec<String>,
    pub(super) to_drop: Vec<String>,
}

pub(super) fn watch_set_change<'a>(
    current: impl IntoIterator<Item = &'a String>,
    requested: &[String],
) -> WatchSetChange {
    let current: HashSet<&String> = current.into_iter().collect();
    let wanted: HashSet<&String> = requested.iter().collect();
    let mut to_arm: Vec<String> = Vec::new();
    for root in requested {
        if !current.contains(root) && !to_arm.contains(root) {
            to_arm.push(root.clone());
        }
    }
    let mut to_drop: Vec<String> = current
        .into_iter()
        .filter(|root| !wanted.contains(root))
        .cloned()
        .collect();
    to_drop.sort();
    WatchSetChange { to_arm, to_drop }
}

/// The payload of the per-project events (`feed://ignore-changed`,
/// `git://state`): which held project the news is for, as the frontend spelled it.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProjectRootPayload {
    pub(super) root: String,
}

#[derive(Default)]
pub struct WatcherState {
    /// Each window's Change Feed watches, keyed by window label. Shared behind
    /// an `Arc` so a command can lift its window's watches out under a brief map
    /// lock, then work the fine-grained inner mutexes without holding it.
    windows: Mutex<HashMap<String, Arc<WindowWatches>>>,
    /// The picker's folder watcher - see `watch_dirs`. Also per window: each
    /// window has its own picker, watching its own folders, and its
    /// appearance/disappearance events route back only to it.
    pub(super) directory_watchers: Mutex<HashMap<String, RecommendedWatcher>>,
    /// The language-bar watch - see `watch_languages`. Per window, like the
    /// picker's: each window watches the projects it follows.
    pub(super) language_watchers: Mutex<HashMap<String, LanguageWatch>>,
}

/// One window's language watch: a single notify watcher over the folders of
/// the projects it follows, re-aimed folder by folder as they come and go.
pub(super) struct LanguageWatch {
    pub(super) watcher: RecommendedWatcher,
    /// What each watched folder reports for, shared with the notify callback.
    pub(super) routes: Arc<Mutex<HashMap<PathBuf, LanguageRoute>>>,
    /// Each watched folder's canonical spelling, resolved once when it is first
    /// watched, so a rename or delete can let go of it first.
    pub(super) canonical_folders: HashMap<PathBuf, PathBuf>,
    /// The projects the window last asked to follow, as it spelled them - kept
    /// so a change to the window's held projects can re-decide which of them
    /// the Change Feed covers.
    pub(super) requested: Vec<String>,
}

/// This window's watch bookkeeping, created empty on first use. `None` only when
/// the state lock is poisoned - a fault resolves to "no watch" so a caller bails
/// rather than panicking on a background thread.
pub(super) fn window_watches(state: &WatcherState, label: &str) -> Option<Arc<WindowWatches>> {
    let mut windows = state.windows.lock().ok()?;
    Some(Arc::clone(windows.entry(label.to_string()).or_default()))
}

/// Let go of every window's Change Feed and git-state watch on `folder` or
/// inside it. On Windows a recursive watch holds the folder open, so it must go
/// before the folder can be renamed; the feed re-arms on the new path the next
/// time its window calls `watch_projects` with it.
pub fn release_watches_under(app: &AppHandle, folder: &Path) {
    let Ok(folder) = std::fs::canonicalize(folder) else {
        return;
    };
    let state = app.state::<WatcherState>();
    let Ok(windows) = state.windows.lock() else {
        return;
    };
    let window_watch_sets: Vec<Arc<WindowWatches>> = windows.values().cloned().collect();
    drop(windows);
    for watches in window_watch_sets {
        let released: Vec<Arc<ProjectWatch>> = {
            let Ok(mut projects) = watches.projects.lock() else {
                continue;
            };
            let inside: Vec<String> = projects
                .iter()
                .filter(|(_, project)| project.canonical_root.starts_with(&folder))
                .map(|(root, _)| root.clone())
                .collect();
            inside
                .iter()
                .filter_map(|root| projects.remove(root))
                .collect()
        };
        // Dropped outside the projects lock: tearing a watcher down waits on
        // its OS handle, which must never stall a status read.
        drop(released);
    }
    // A language watch on a project inside the folder holds it open too; it
    // re-arms the next time its window's rows change.
    let Ok(mut language_watchers) = state.language_watchers.lock() else {
        return;
    };
    language_watchers.retain(|_, watch| {
        !watch
            .canonical_folders
            .values()
            .any(|watched| watched.starts_with(&folder))
    });
}

/// Drop the watch bookkeeping of every window that has since closed. There is no
/// per-window teardown hook (windows share one process), so `watch_projects` and
/// `watch_dirs` prune here on each call: dropping an entry drops its watchers,
/// which stops the OS watch and retires the paired git-state coalescer thread.
/// Keeps the maps bounded to live windows over a long multi-window session.
pub(super) fn prune_closed_windows(app: &AppHandle, state: &WatcherState) {
    let live: HashSet<String> = app.webview_windows().into_keys().collect();
    if let Ok(mut windows) = state.windows.lock() {
        windows.retain(|label, _| live.contains(label));
    }
    if let Ok(mut directory_watchers) = state.directory_watchers.lock() {
        directory_watchers.retain(|label, _| live.contains(label));
    }
    if let Ok(mut language_watchers) = state.language_watchers.lock() {
        language_watchers.retain(|label, _| live.contains(label));
    }
}
