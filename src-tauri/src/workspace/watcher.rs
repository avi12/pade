//! Filesystem watcher feeding the Change Feed.
//!
//! MVP: watches the opened project, ignores build/VCS noise, and turns each
//! save into a `ChangeEvent` with a line-count delta and a heuristic summary.
//! Later: real per-hunk diffs and agent-authored intent replace the heuristic.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::platform::preview::{
    image_mime_type, preview_text, video_preview, FeedImage, FeedVideo, MAXIMUM_IMAGE_BYTES,
    MAXIMUM_PREVIEW_BYTES,
};
use crate::util::now_milliseconds;
use crate::vcs::git_in;

static CHANGE_EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// How long after one event for a path its next one is dropped: editors emit a
/// burst of events per save.
const SAVE_BURST_DEBOUNCE: Duration = Duration::from_millis(150);

/// One held project's Change Feed watch and its per-file bookkeeping. A window
/// holds sessions of several projects, and an agent edits whichever project its
/// session lives in - front or not - so a window watches every project it holds
/// (see [`watch_projects`]), one `ProjectWatch` each. The roots are fixed for
/// the watch's lifetime: a project the window lets go of drops its whole
/// `ProjectWatch` (watchers, baselines, counters), and one it picks up again
/// starts fresh.
struct ProjectWatch {
    /// The owning window's label: every event this watch detects is routed back
    /// to that window alone (`emit_to`), never broadcast to its siblings.
    label: String,
    /// The root exactly as the frontend spelled it in `watch_projects` - what
    /// every per-project event carries, so the frontend keys by its own spelling.
    requested_root: String,
    /// The normalized root the watch is armed on (see [`resolve_watch_root`]).
    root: PathBuf,
    /// The canonical filesystem target of `root`, which preview reads are
    /// confined to and a folder release matches against.
    canonical_root: PathBuf,
    /// When this watch was armed (epoch ms) - the "watching since" the feed
    /// states in its empty panel, since the feed only ever holds changes made
    /// after that moment (nothing replays what happened before it).
    armed_at: u64,
    /// The recursive notify watcher, installed once right after the watch is
    /// built (its callback holds a `Weak` back to this watch). Held only so
    /// dropping it stops the watch.
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// The project's live-git-state watcher - see `arm_git_state`. Separate
    /// from the Change Feed's recursive watcher on purpose: it watches only the
    /// dir(s) holding `.git/HEAD` + `.git/config`, non-recursively, so git's
    /// object churn never reaches it. Dropping it drops its channel sender,
    /// which retires the paired coalescer thread once that thread drains.
    git_state: Mutex<Option<RecommendedWatcher>>,
    line_counts: Mutex<HashMap<PathBuf, usize>>,
    last_seen: Mutex<HashMap<PathBuf, Instant>>,
    /// First-touch baseline snapshots for the git-free preview, keyed by absolute
    /// path. `Some(text)` is the content the path held the first time it changed
    /// this watch session (empty for a file created this session, so its diff is a
    /// full addition); `None` records a path seen but not snapshottable (binary,
    /// over the size cap, or already gone), so it is never re-read and the preview
    /// falls back to "none".
    baselines: Mutex<HashMap<PathBuf, Option<String>>>,
    /// How this root decides a path is ignored, computed when the watch is built
    /// and recomputed whenever the rules change (see [`IgnoreRules`]).
    ignore_rules: IgnoreRules,
    /// The last parseable state of each registered MCP config. Existence and
    /// server membership drive automatic handoff; same-membership content edits
    /// arm frontend reload-failure recovery.
    mcp_servers: Mutex<HashMap<PathBuf, crate::workspace::mcp::McpSnapshot>>,
    /// Changes surfaced to the window since the watch was armed, and changes the
    /// ignore rules dropped. An empty feed is otherwise ambiguous - a quiet
    /// project and a watch whose every event is being filtered look identical -
    /// so [`watch_status`] reports both and the panel says which it is. Atomics,
    /// not a lock: the notify callback bumps them on every event it handles.
    surfaced: AtomicU64,
    ignored: AtomicU64,
}

/// One window's Change Feed watches: one [`ProjectWatch`] per held project root,
/// keyed by the root as the frontend spelled it. Every PADE window opens its own
/// workspace, but all windows share one backend process, so this state is held
/// per window (keyed by window label in [`WatcherState`]); each window's events
/// are emitted back only to it.
#[derive(Default)]
struct WindowWatches {
    projects: Mutex<HashMap<String, Arc<ProjectWatch>>>,
    /// The ticket of the newest `watch_projects` request. `watch_projects` asks
    /// git about new roots before it takes the projects lock, so two requests
    /// can finish out of order; only the holder of the newest ticket may apply
    /// its set, which keeps "the latest set wins" on a quick switch.
    latest_arm_request: AtomicU64,
}

impl WindowWatches {
    /// Take a ticket for a new arming request, superseding every earlier one.
    fn begin_arm_request(&self) -> u64 {
        self.latest_arm_request.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Whether no newer arming request has started since `ticket` was taken.
    fn is_latest_arm_request(&self, ticket: u64) -> bool {
        self.latest_arm_request.load(Ordering::SeqCst) == ticket
    }

    /// A snapshot of this window's project watches, cloned out under a brief
    /// lock so callers never hold it across I/O. Empty when the lock is poisoned.
    fn snapshot(&self) -> Vec<Arc<ProjectWatch>> {
        self.projects
            .lock()
            .map(|projects| projects.values().cloned().collect())
            .unwrap_or_default()
    }

    /// The watch on `root`: the one keyed by that exact spelling, else the one
    /// armed on the same normalized root.
    fn find(&self, root: &str) -> Option<Arc<ProjectWatch>> {
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
    fn owner_of(&self, path: &Path) -> Option<Arc<ProjectWatch>> {
        owning_project(&self.snapshot(), path).cloned()
    }
}

/// The project among `projects` that owns `path`: the deepest whose root
/// contains it.
fn owning_project<'a>(
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
fn deepest_containing<'a, T>(
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
struct WatchSetChange {
    to_arm: Vec<String>,
    to_drop: Vec<String>,
}

fn watch_set_change<'a>(
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
struct ProjectRootPayload {
    root: String,
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
    directory_watchers: Mutex<HashMap<String, RecommendedWatcher>>,
    /// The language-bar watch - see `watch_languages`. Per window, like the
    /// picker's: each window watches the projects it follows.
    language_watchers: Mutex<HashMap<String, LanguageWatch>>,
}

/// One window's language watch: a single notify watcher over the folders of
/// the projects it follows, re-aimed folder by folder as they come and go.
struct LanguageWatch {
    watcher: RecommendedWatcher,
    /// What each watched folder reports for, shared with the notify callback.
    routes: Arc<Mutex<HashMap<PathBuf, LanguageRoute>>>,
    /// Each watched folder's canonical spelling, resolved once when it is first
    /// watched, so a rename or delete can let go of it first.
    canonical_folders: HashMap<PathBuf, PathBuf>,
    /// The projects the window last asked to follow, as it spelled them - kept
    /// so a change to the window's held projects can re-decide which of them
    /// the Change Feed covers.
    requested: Vec<String>,
}

/// This window's watch bookkeeping, created empty on first use. `None` only when
/// the state lock is poisoned - a fault resolves to "no watch" so a caller bails
/// rather than panicking on a background thread.
fn window_watches(state: &WatcherState, label: &str) -> Option<Arc<WindowWatches>> {
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
fn prune_closed_windows(app: &AppHandle, state: &WatcherState) {
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

/// The kind of filesystem change. Serializes to the exact lowercase wire
/// strings the frontend reads - the one authoritative home for those literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
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
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChangeEvent {
    id: String,
    /// The held project root the change belongs to, exactly as the frontend
    /// spelled it in `watch_projects`.
    root: String,
    path: String,
    kind: ChangeKind,
    added: usize,
    removed: usize,
    summary: String,
    /// When the change surfaced, in epoch milliseconds.
    #[serde(rename = "ts")]
    timestamp: u64,
}

/// The git metadata entry a workspace gains when `git init` runs - the one
/// authoritative spelling of the `.git` name.
const GIT_DIRECTORY_NAME: &str = ".git";

const ALWAYS_IGNORED_DIRECTORIES: &[&str] = &[
    GIT_DIRECTORY_NAME,
    "node_modules",
    "target",
    "dist",
    "build",
    ".svelte-kit",
    ".ade",
    ".vite",
];

fn is_always_ignored(path: &Path) -> bool {
    path_component_matches(path, |name| ALWAYS_IGNORED_DIRECTORIES.contains(&name))
}

/// Whether any component of `path` names an ignored directory, decided by the
/// `is_ignored_name` predicate. One authoritative component scan shared by the
/// always-on baseline [`ignored`] pre-filter (against the [`ALWAYS_IGNORED_DIRECTORIES`] slice) and
/// static-mode exclusion (against a tech-inferred [`HashSet`]).
fn path_component_matches(path: &Path, is_ignored_name: impl Fn(&str) -> bool) -> bool {
    path.components()
        .filter_map(|component| component.as_os_str().to_str())
        .any(is_ignored_name)
}

/// Static-mode exclusion: whether `path` lies under any directory named in `directories`.
fn is_under_static_ignore_directory(path: &Path, directories: &HashSet<String>) -> bool {
    path_component_matches(path, |name| directories.contains(name))
}

/// How a watch decides a path is "ignored", fixed once per watch root (when
/// `watch_projects` arms the Change Feed on it) and held in its [`IgnoreRules`].
///
/// A git work tree defers to git itself via `git check-ignore`, which honors
/// nested `.gitignore` files, `.git/info/exclude`, the global `core.excludesFile`,
/// and negation rules - exactly what the user's git already ignores, rather than a
/// brittle hand-rolled parse. A folder that is not a git repo has none of those
/// tools, so it falls back to a set of directory names inferred from the manifest
/// files at the root (see [`manifest_ignore_directories`]) unioned with the [`ALWAYS_IGNORED_DIRECTORIES`]
/// baseline - plus the root `.gitignore`'s own rules when the folder has one (a
/// scaffolded project knows what it will generate before its `git init` runs).
///
/// The policy is NOT fixed for the watch's lifetime: any `.gitignore`
/// change/appearance/deletion, and a mid-session `git init`, recompute it (see
/// [`refresh_ignore_policy`]) and announce [`FEED_IGNORE_EVENT`] so the frontend
/// re-filters what it already shows.
enum IgnorePolicy {
    /// The root is a git work tree; ask `git check-ignore` per path (memoized).
    Git { root: PathBuf },
    /// The root is not a git repo; exclude any path whose component names one of
    /// these tech-inferred directories, or that the root `.gitignore` ignores.
    Static {
        root: PathBuf,
        directories: HashSet<String>,
        rules: crate::workspace::gitignore::Rules,
    },
}

/// One watch root's [`IgnorePolicy`] plus git's memoized answers under it - the
/// one ignore filter shared by the Change Feed and the language watch, so both
/// skip exactly what the project's own git skips.
#[derive(Default)]
struct IgnoreRules {
    /// `None` until a policy is installed. Git mode defers to git's own ignore
    /// rules; static mode carries a tech-inferred set of directory names.
    policy: Mutex<Option<IgnorePolicy>>,
    /// Memoized `git check-ignore` results per absolute path (git mode only), so
    /// a path shells git at most once. Cleared with every new policy, since
    /// editing the rules can flip any path's state.
    git_ignore_cache: Mutex<HashMap<PathBuf, bool>>,
}

impl IgnoreRules {
    /// Adopt a freshly computed policy and forget every memoized decision.
    fn install(&self, policy: IgnorePolicy) {
        if let Ok(mut guard) = self.policy.lock() {
            *guard = Some(policy);
        }
        if let Ok(mut cache) = self.git_ignore_cache.lock() {
            cache.clear();
        }
    }

    fn is_installed(&self) -> bool {
        self.policy.lock().is_ok_and(|guard| guard.is_some())
    }

    /// Whether the policy excludes `path`. Git mode asks git (memoized per
    /// path); static mode matches the path's component names against the
    /// tech-inferred ignore set. A poisoned lock, an unset policy, or a git
    /// failure all resolve to "not excluded" so a fault never silently hides
    /// real changes.
    fn excludes(&self, path: &Path) -> bool {
        let git_root = {
            let Ok(guard) = self.policy.lock() else {
                return false;
            };
            match guard.as_ref() {
                None => return false,
                Some(IgnorePolicy::Static {
                    root,
                    directories,
                    rules,
                }) => {
                    return is_under_static_ignore_directory(path, directories)
                        || gitignore_excludes(root, rules, path);
                }
                Some(IgnorePolicy::Git { root }) => root.clone(),
            }
        };
        self.git_excludes(&git_root, path)
    }

    /// Whether git considers `path` ignored under the work tree at `root`,
    /// memoized so each path shells git at most once.
    fn git_excludes(&self, root: &Path, path: &Path) -> bool {
        let memoized = self
            .git_ignore_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(path).copied());
        if let Some(is_ignored) = memoized {
            return is_ignored;
        }
        let is_ignored = git_check_ignore(root, path);
        if let Ok(mut cache) = self.git_ignore_cache.lock() {
            cache.insert(path.to_path_buf(), is_ignored);
        }
        is_ignored
    }
}

/// The ignore-rules file whose edits invalidate the git-check-ignore cache: when
/// its rules change, any path's ignored state can flip.
const GITIGNORE_FILE_NAME: &str = ".gitignore";

/// Announces that the ignore rules themselves changed (a `.gitignore` touched,
/// a `git init` flipping the policy). Carries only the affected project's root
/// ([`ProjectRootPayload`]): the frontend re-asks `feed_ignored` about the
/// events it holds, so the answer is never stale.
const FEED_IGNORE_EVENT: &str = "feed://ignore-changed";

/// Announces a single Change Feed entry (a file created/modified/deleted in a
/// watched project). This name is the frontend contract (`src/lib/bridge.ts`).
const FEED_CHANGE_EVENT: &str = "feed://change";

// ── Live MCP config ───────────────────────────────────────────────────────────
// A project's MCP servers live in a config file (`.mcp.json` for Claude, per
// config.rs). A running agent only picks up an **added or removed** server by
// restarting - Claude re-reads `.mcp.json` at launch, and there is no in-session
// reload - while a value-only edit to an existing server the user reconnects by
// hand. So the watcher restarts the affected sessions only when the *set of
// server names* changes; the frontend does the terminate-and-resume.

/// Announces that a project's declared MCP servers changed (a name added or
/// removed). Carries which agents the config governs so the frontend restarts
/// only their sessions, and the added/removed names for the toast.
const MCP_EVENT: &str = "mcp://changed";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct McpChange {
    /// The MCP config file that changed (absolute path).
    path: String,
    /// The watched project root. Nested configs cannot derive this from `path`.
    root: String,
    /// The agent ids whose servers this file declares (e.g. `["claude"]`).
    agents: Vec<String>,
    /// Server names gained since the last seen set.
    added: Vec<String>,
    /// Server names lost since the last seen set.
    removed: Vec<String>,
    /// Whether the file was created/deleted or server membership changed.
    membership_changed: bool,
}

/// The agents whose MCP servers `path` declares, if it is a known MCP config
/// file under `root` (e.g. `<root>/.mcp.json` → `["claude"]`). `None` for any
/// other path. Matches the full relative path, so a nested config
/// (`.cursor/mcp.json`) can't be confused with a root one.
fn mcp_config_for_path(root: &Path, path: &Path) -> Option<crate::workspace::config::McpConfig> {
    // Compared root-relative, so the per-event check joins no paths.
    let relative = path.strip_prefix(root).ok()?;
    crate::workspace::config::mcp_configs()
        .find(|config| relative == Path::new(config.relative_path))
}

fn mcp_membership_changed(
    previous: &crate::workspace::mcp::McpSnapshot,
    current: &crate::workspace::mcp::McpSnapshot,
) -> bool {
    previous.exists != current.exists || previous.names != current.names
}

/// Seed the MCP server-name baseline for `root`: record the current set for each
/// known MCP config file, so the first later change diffs against the truth on
/// disk rather than an empty set (which would restart on project open).
fn mcp_baseline(root: &Path) -> HashMap<PathBuf, crate::workspace::mcp::McpSnapshot> {
    crate::workspace::config::mcp_configs()
        .filter_map(|config| {
            let file = root.join(config.relative_path);
            crate::workspace::mcp::snapshot(&file, config.format).map(|state| (file, state))
        })
        .collect()
}

/// Detect an MCP server add/remove for a changed `path` and, when the set truly
/// changed, emit [`MCP_EVENT`] and advance the baseline. A value-only edit (same
/// names) and an unparseable half-written file both no-op, so a session restarts
/// only on a real membership change. Runs before the ignore filter, so a
/// `.gitignore`'d `.mcp.json` still triggers.
fn detect_mcp_change(app: &AppHandle, watch: &ProjectWatch, path: &Path) {
    let root = &watch.root;
    let Some(config) = mcp_config_for_path(root, path) else {
        return;
    };
    let Some(current) = crate::workspace::mcp::snapshot(path, config.format) else {
        return; // unparseable mid-write - wait for the complete save
    };

    let (added, removed, membership_changed) = {
        let Ok(mut baseline) = watch.mcp_servers.lock() else {
            return;
        };
        let previous = baseline.get(path).cloned().unwrap_or_default();
        if current == previous {
            return;
        }
        let added: Vec<String> = current.names.difference(&previous.names).cloned().collect();
        let removed: Vec<String> = previous.names.difference(&current.names).cloned().collect();
        let membership_changed = mcp_membership_changed(&previous, &current);
        baseline.insert(path.to_path_buf(), current);
        (added, removed, membership_changed)
    };

    let _ = app.emit_to(
        &watch.label,
        MCP_EVENT,
        McpChange {
            path: path.to_string_lossy().into_owned(),
            root: root.to_string_lossy().into_owned(),
            agents: config
                .agents
                .iter()
                .map(|agent| (*agent).to_string())
                .collect(),
            added,
            removed,
            membership_changed,
        },
    );
}

/// The build- and dependency-output directories a given manifest file implies
/// should be excluded when there is no git to consult. The single authoritative
/// home for the manifest→ignore-directories mapping; each name is a real
/// per-ecosystem generated directory. Matched by exact file name, or by extension
/// for the .NET project/solution manifests. Returns an empty slice for a file that
/// implies nothing.
fn manifest_ignore_directories(file_name: &str) -> &'static [&'static str] {
    const NODE: &[&str] = &[
        "node_modules",
        "dist",
        "build",
        "out",
        "coverage",
        ".next",
        ".nuxt",
        ".svelte-kit",
        ".turbo",
        ".cache",
        ".parcel-cache",
    ];
    const RUST: &[&str] = &["target"];
    const PYTHON: &[&str] = &[
        "__pycache__",
        ".venv",
        "venv",
        ".mypy_cache",
        ".pytest_cache",
        ".ruff_cache",
        ".tox",
        "build",
        "dist",
        ".eggs",
    ];
    const GO: &[&str] = &["vendor", "bin"];
    const JVM: &[&str] = &["target", "build", ".gradle"];
    const RUBY: &[&str] = &["vendor", ".bundle"];
    const PHP: &[&str] = &["vendor"];
    const DOTNET: &[&str] = &["bin", "obj"];
    const NONE: &[&str] = &[];

    let extension = Path::new(file_name)
        .extension()
        .and_then(|name| name.to_str());
    if matches!(extension, Some("csproj" | "sln" | "fsproj")) {
        return DOTNET;
    }

    match file_name {
        "package.json" | "pnpm-lock.yaml" | "yarn.lock" | "package-lock.json" => NODE,
        "Cargo.toml" => RUST,
        "pyproject.toml" | "requirements.txt" | "setup.py" | "Pipfile" => PYTHON,
        "go.mod" => GO,
        "pom.xml" | "build.gradle" | "build.gradle.kts" => JVM,
        "Gemfile" => RUBY,
        "composer.json" => PHP,
        _ => NONE,
    }
}

/// The set of directory names to exclude in static mode: the [`ALWAYS_IGNORED_DIRECTORIES`] baseline
/// (so static mode is never weaker than the always-on pre-filter) unioned with the
/// ignore directories implied by every manifest file sitting directly in `root`.
/// One cheap one-level directory scan, run once per armed root.
fn static_ignore_directories(root: &Path) -> HashSet<String> {
    let mut directories: HashSet<String> = ALWAYS_IGNORED_DIRECTORIES
        .iter()
        .map(|&name| name.to_string())
        .collect();
    let Ok(entries) = std::fs::read_dir(root) else {
        return directories;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        for &directory in manifest_ignore_directories(name) {
            directories.insert(directory.to_string());
        }
    }
    directories
}

fn is_gitignore_file(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(GITIGNORE_FILE_NAME)
}

/// Determine how to exclude ignored files for the watch `root`: git mode when
/// the root is a git work tree (defer to git's own ignore rules), else static
/// mode with a tech-inferred ignore-directory set plus the root `.gitignore`'s
/// rules when one exists. Runs git once (and, in static mode, one directory
/// scan + one file read); re-run whenever the rules could have changed.
fn compute_ignore_policy(root: &Path) -> IgnorePolicy {
    if is_git_work_tree(root) {
        return IgnorePolicy::Git {
            root: root.to_path_buf(),
        };
    }
    let rules = std::fs::read_to_string(root.join(GITIGNORE_FILE_NAME))
        .map(|content| crate::workspace::gitignore::Rules::parse(&content))
        .unwrap_or_default();
    IgnorePolicy::Static {
        root: root.to_path_buf(),
        directories: static_ignore_directories(root),
        rules,
    }
}

/// Recompute the ignore policy for the project's watch root and forget every
/// memoized git decision - run when the rules themselves may have changed (a
/// `.gitignore` edited/created/deleted anywhere under the root, or a
/// mid-session `git init` flipping the root from static to git mode). The
/// frontend hears [`FEED_IGNORE_EVENT`] and re-filters the events it already
/// shows for that project against the fresh policy (`feed_ignored`).
fn refresh_ignore_policy(app: &AppHandle, watch: &ProjectWatch) {
    watch
        .ignore_rules
        .install(compute_ignore_policy(&watch.root));
    let _ = app.emit_to(
        &watch.label,
        FEED_IGNORE_EVENT,
        ProjectRootPayload {
            root: watch.requested_root.clone(),
        },
    );
}

/// Whether `root` sits inside a git work tree, via `git -C <root> rev-parse
/// --is-inside-work-tree` (which prints `true` in a work tree). A missing git
/// binary, a non-repo folder, or a bare repo all resolve to `false`, routing
/// the watch to the static tech-inference fallback instead. Runs git against
/// `root` (not PADE's process cwd) so it reflects the watched project.
fn is_git_work_tree(root: &Path) -> bool {
    let Ok(output) = git_in(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
    else {
        return false;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
}

/// Whether the static policy's `.gitignore` rules ignore `path` - matched on
/// its root-relative, `/`-joined spelling. A path outside the root (or one that
/// isn't valid UTF-8) can't match any rule.
fn gitignore_excludes(
    root: &Path,
    rules: &crate::workspace::gitignore::Rules,
    path: &Path,
) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let Some(relative) = relative.to_str() else {
        return false;
    };
    rules.is_ignored(&relative.replace('\\', "/"))
}

/// Shell `git -C <root> check-ignore -q -- <path>` and report whether the path is
/// ignored: exit status 0 means git would ignore it. Because git evaluates the path
/// against every applicable ignore source itself, this correctly honors nested
/// `.gitignore` files, `.git/info/exclude`, `core.excludesFile`, and negations. Any
/// spawn failure resolves to `false` so a fault never hides a real change. Scoped to
/// `root` via `-C`, never PADE's process cwd.
fn git_check_ignore(root: &Path, path: &Path) -> bool {
    let Ok(output) = git_in(root)
        .args(["check-ignore", "-q", "--"])
        .arg(path)
        .output()
    else {
        return false;
    };
    output.status.success()
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
fn surfaces(kind: ChangeKind, path_exists: bool) -> bool {
    matches!(kind, ChangeKind::Deleted) || path_exists
}

/// The change kind the card should carry, reconciled against the disk: a
/// "deleted" path that exists again was an atomic-save replace (delete +
/// rename-over), so it is a modification. Every other kind stands as reported.
fn reclassify(kind: ChangeKind, path_exists: bool) -> ChangeKind {
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
fn line_delta(old: Option<usize>, new: Option<usize>) -> (usize, usize) {
    match (old, new) {
        (Some(old_count), Some(new_count)) if new_count >= old_count => (new_count - old_count, 0),
        (Some(old_count), Some(new_count)) => (0, old_count - new_count),
        (None, Some(new_count)) => (new_count, 0),
        (Some(old_count), None) => (0, old_count),
        (None, None) => (0, 0),
    }
}

fn line_count(path: &Path) -> Option<usize> {
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
fn authorize_path(root: &Path, path: &Path) -> Result<Option<PathBuf>, ()> {
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
fn read_authorized_file(
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
fn read_preview_text(root: &Path, path: &Path) -> Result<Option<String>, ()> {
    let Some(bytes) = read_authorized_file(root, path, MAXIMUM_PREVIEW_BYTES)? else {
        return Ok(None);
    };
    preview_text(bytes).map(Some).ok_or(())
}

/// What happened to the file, for the line under its name on a feed card. The
/// name is deliberately absent: the card prints it directly above, and a summary
/// that repeated it made every card say the same thing twice.
fn summarize(kind: ChangeKind, added: usize, removed: usize) -> String {
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

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

/// Normalize a project root the frontend hands `watch_projects` into a Change
/// Feed watch root: collapse separators and `.`/`..` to one spelling - but
/// without the Windows `\\?\` verbatim prefix `std::fs::canonicalize` would add,
/// which would leak into every emitted change path - and confirm it is an
/// existing directory, so a drifted or bad path fails loudly here rather than
/// silently watching the wrong tree. Mirrors workspace's `canonical_path`.
fn resolve_watch_root(root: &str) -> Result<PathBuf, String> {
    let normalized: PathBuf = Path::new(root).components().collect();
    if !normalized.is_dir() {
        return Err(format!(
            "Change Feed can't watch {}: not an existing directory",
            normalized.display()
        ));
    }
    Ok(normalized)
}

/// Build and arm a fresh watch on `requested_root` for the window `label`.
/// Everything that shells out to git or touches the disk happens here, before
/// the caller takes the projects lock: the synchronous `watch_status` reads
/// that lock and must never wait on a git process.
fn build_project_watch(
    app: &AppHandle,
    label: &str,
    requested_root: &str,
) -> Result<Arc<ProjectWatch>, String> {
    let root = resolve_watch_root(requested_root)?;
    let canonical_root = std::fs::canonicalize(&root).map_err(|error| {
        format!(
            "Change Feed can't resolve watch root {}: {error}",
            root.display()
        )
    })?;

    // Install the policy before arming the watcher, so the very first change
    // is already filtered.
    let ignore_rules = IgnoreRules::default();
    ignore_rules.install(compute_ignore_policy(&root));
    let git_state_plan = GitStatePlan::for_root(&root);

    let watch = Arc::new(ProjectWatch {
        label: label.to_string(),
        requested_root: requested_root.to_string(),
        // Seed the MCP server baseline from what's on disk now, so opening a
        // project that already declares servers doesn't read as "all added"
        // and restart.
        mcp_servers: Mutex::new(mcp_baseline(&root)),
        root,
        canonical_root,
        armed_at: now_milliseconds(),
        watcher: Mutex::default(),
        git_state: Mutex::default(),
        line_counts: Mutex::default(),
        last_seen: Mutex::default(),
        baselines: Mutex::default(),
        ignore_rules,
        surfaced: AtomicU64::new(0),
        ignored: AtomicU64::new(0),
    });

    // The live-git-state companion watch (`HEAD`/`config` → `git://state`)
    // shares the project watch's lifetime, but has its own tiny non-recursive
    // watcher.
    arm_git_state(app, &watch, &git_state_plan);

    // The notify callback runs on its own thread. It holds only a `Weak`: the
    // watch owns the watcher, so a strong handle would keep both alive forever.
    let app_handle = app.clone();
    let callback_watch = Arc::downgrade(&watch);
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        let Some(watch) = callback_watch.upgrade() else {
            return;
        };
        handle_event(&app_handle, &watch, event);
    })
    .map_err(|error| error.to_string())?;
    watcher
        .watch(&watch.root, RecursiveMode::Recursive)
        .map_err(|error| error.to_string())?;
    *watch.watcher.lock().map_err(|error| error.to_string())? = Some(watcher);
    Ok(watch)
}

/// Watch exactly the project roots a window holds sessions of - the Change
/// Feed must hear an agent's edits in a background project too, not only in
/// the front one. Declarative and idempotent: `roots` is the window's full set,
/// passed explicitly by the frontend (never derived from the process's current
/// directory). Roots not yet watched are armed, watched roots no longer listed
/// are dropped with all their bookkeeping, and unchanged roots are left alone so
/// their baselines survive. Calls can overlap; the newest call's set wins.
///
/// A root that can't be watched (gone, not a directory) fails on its own: every
/// other root is still applied, and the error names each failure.
#[tauri::command]
pub async fn watch_projects(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    roots: Vec<String>,
) -> Result<(), String> {
    let label = window.label().to_string();

    // Retire the bookkeeping of any window that has since closed before taking on
    // this one's - no per-window teardown hook fires otherwise (see
    // `prune_closed_windows`).
    prune_closed_windows(&app, &state);
    let Some(watches) = window_watches(&state, &label) else {
        return Err("watcher state unavailable".to_string());
    };

    let ticket = watches.begin_arm_request();
    let current_roots: Vec<String> = watches
        .projects
        .lock()
        .map_err(|error| error.to_string())?
        .keys()
        .cloned()
        .collect();

    let mut armed: Vec<Arc<ProjectWatch>> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for requested_root in watch_set_change(&current_roots, &roots).to_arm {
        match build_project_watch(&app, &label, &requested_root) {
            Ok(watch) => armed.push(watch),
            Err(error) => failures.push(error),
        }
    }

    let released: Vec<Arc<ProjectWatch>> = {
        let mut projects = watches.projects.lock().map_err(|error| error.to_string())?;
        if !watches.is_latest_arm_request(ticket) {
            // A newer `watch_projects` has begun; it owns the set. The watches
            // built here drop after the lock is released.
            drop(projects);
            return Ok(());
        }
        // Re-planned under the lock: the set may have moved since the snapshot.
        let change = watch_set_change(projects.keys(), &roots);
        let mut released: Vec<Arc<ProjectWatch>> = change
            .to_drop
            .iter()
            .filter_map(|root| projects.remove(root))
            .collect();
        for watch in armed {
            // A root armed meanwhile keeps its live watch; the duplicate goes.
            if projects.contains_key(&watch.requested_root) {
                released.push(watch);
            } else {
                projects.insert(watch.requested_root.clone(), watch);
            }
        }
        released
    };
    // Dropped outside the projects lock: tearing a watcher down waits on its
    // OS handle, which must never stall a status read.
    drop(released);

    rearm_language_watch_after_feed(&app, &state, &label);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

// ── Live git state ──────────────────────────────────────────────────────────
// The Change Feed's branch subtitle and the remote-gated actions read git state
// (`vcs_branch_of`, `vcs_remote_url`) that only changes when two files do:
// `.git/HEAD` (branch switches) and `.git/config` (remotes). This watcher tells
// the frontend the moment either changes - or the repo itself appears - so that
// state stays live instead of refreshing only on mount / project switch.

/// The event announcing a change to a held project's live git state. Carries
/// only which project ([`ProjectRootPayload`]) on purpose: listeners re-fetch
/// whatever git state they display, so the event can never carry stale or
/// partial data.
const GIT_STATE_EVENT: &str = "git://state";

/// How long a burst of git-state touches is coalesced before one event is
/// emitted. A checkout rewrites `HEAD` several times (lockfile, rename, reflog
/// bookkeeping); waiting for the burst to go quiet emits once, after git has
/// settled, so listeners re-fetch the final state.
const GIT_STATE_COALESCE: Duration = Duration::from_millis(250);

/// The files inside a git dir whose content IS the live state PADE displays:
/// `HEAD` names the checked-out branch, `config` holds the remotes.
const GIT_STATE_FILE_NAMES: &[&str] = &["HEAD", "config"];

/// What the git-state watcher's callback tells its coalescer thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitStateMessage {
    /// `HEAD` or `config` was touched - the displayed state changed.
    StateTouched,
    /// A `.git` entry appeared or disappeared, so the watcher must switch
    /// between the root sentinel and the repo's state directories.
    GitDirectoryChanged,
    /// A `.gitignore` in an ANCESTOR of the watch root was touched (the
    /// recursive feed watch only sees the root's own subtree, but a workspace
    /// deeper than the repo toplevel inherits every ancestor's rules) - the
    /// ignore policy must refresh.
    IgnoreRulesTouched,
}

/// What one coalesced burst of git-state messages asks for. Distinct effects
/// rather than one "strongest" message: a busy burst can legitimately need a
/// re-arm, an ignore refresh, and a state emit at once.
#[derive(Default, Clone, Copy)]
struct GitStateBurst {
    rearm: bool,
    state_touched: bool,
    ignore_touched: bool,
}

/// Whether `path` names one of the two git-state files. The watch is
/// non-recursive on the git dir itself, so a matching name is a top-level
/// entry, never something under `objects/` or `refs/`.
fn is_git_state_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| GIT_STATE_FILE_NAMES.contains(&name))
}

/// Whether `path` names a `.git` entry (the thing `git init` creates).
fn is_git_directory_entry(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(GIT_DIRECTORY_NAME)
}

fn git_directory_lifecycle_changed(kind: EventKind, paths: &[PathBuf]) -> bool {
    matches!(kind, EventKind::Create(_) | EventKind::Remove(_))
        && paths.iter().any(|path| is_git_directory_entry(path))
}

/// The directories holding the workspace's `HEAD` and `config`, each to be
/// watched non-recursively. A plain repo keeps both directly in `<root>/.git` -
/// one dir. A linked worktree or submodule keeps a `.git` *file* pointing
/// elsewhere: `HEAD` sits in the resolved git dir and `config` in the common
/// dir, so both are watched (deduplicated when they coincide). Empty when the
/// workspace has no git yet.
fn git_state_directories(root: &Path) -> Vec<PathBuf> {
    let git_entry = root.join(GIT_DIRECTORY_NAME);
    if git_entry.is_dir() {
        return vec![git_entry];
    }
    if !git_entry.is_file() {
        return Vec::new();
    }
    resolved_git_directories(root)
}

/// Ask git where a `.git`-file workspace's real git dir and common dir live
/// (`--path-format=absolute` keeps both absolute regardless of git's cwd). Any
/// failure resolves to no dirs - the state watcher then stays quiet rather than
/// watching a guessed location.
fn resolved_git_directories(root: &Path) -> Vec<PathBuf> {
    let Ok(output) = git_in(root)
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--git-dir",
            "--git-common-dir",
        ])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    unique_directories(String::from_utf8_lossy(&output.stdout).lines())
}

/// Distinct, order-preserving directories from git's line-per-dir output.
fn unique_directories<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<PathBuf> {
    let mut directories: Vec<PathBuf> = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let directory = PathBuf::from(trimmed);
        if !directories.contains(&directory) {
            directories.push(directory);
        }
    }
    directories
}

/// Arm (or re-arm) the live-git-state watch for `root`, replacing any previous
/// one. With a repo present it watches the dir(s) holding `HEAD` and `config`;
/// with none it watches the workspace root itself, non-recursively, purely to
/// see a `.git` entry appear - that costs one handle on a handful of direct
/// children (never the workspace tree, which the recursive feed watcher already
/// covers), and is what lets a `git init` run *after* the project opened gain
/// the real watch without any polling. Best-effort: a failure to arm just means
/// git state won't live-update until the project is watched afresh.
fn arm_git_state(app: &AppHandle, watch: &Arc<ProjectWatch>, plan: &GitStatePlan) {
    let Ok(mut guard) = watch.git_state.lock() else {
        return;
    };
    rearm_git_state_locked(app, watch, plan, &mut guard);
}

/// What arming a git-state watch needs to ask git, gathered up front so the
/// arming itself never shells out - `watch_projects` asks before it takes any
/// lock a synchronous command reads.
struct GitStatePlan {
    /// The dirs holding `HEAD` and `config`; empty when there is no repo yet.
    directories: Vec<PathBuf>,
    /// The ancestors whose `.gitignore` the workspace inherits (repo only).
    ancestor_directories: Vec<PathBuf>,
}

impl GitStatePlan {
    fn for_root(root: &Path) -> Self {
        let directories = git_state_directories(root);
        let ancestor_directories = if directories.is_empty() {
            Vec::new()
        } else {
            gitignore_ancestor_directories(root)
        };
        Self {
            directories,
            ancestor_directories,
        }
    }
}

/// The arming body, run under the `git_state` lock (shared by `arm_git_state`
/// and the coalescer's post-`git init` re-arm, which already holds it). The
/// coalescer routes `git://state` back to the watch's owning window alone.
fn rearm_git_state_locked(
    app: &AppHandle,
    watch: &Arc<ProjectWatch>,
    plan: &GitStatePlan,
    slot: &mut Option<RecommendedWatcher>,
) {
    // Drop the previous watch first, so a failed re-arm can't leave a stale
    // watcher reporting a repo layout that no longer holds.
    *slot = None;

    let root = &watch.root;
    let (sender, receiver) = channel::<GitStateMessage>();
    let armed = if plan.directories.is_empty() {
        watch_for_git_init(root, sender)
    } else {
        watch_git_state(root, &plan.directories, &plan.ancestor_directories, sender)
    };
    let Ok(watcher) = armed else {
        return;
    };
    spawn_git_state_coalescer(app.clone(), Arc::downgrade(watch), receiver);
    *slot = Some(watcher);
}

/// The no-repo-yet sentinel: report a `.git` entry appearing among the root's
/// direct children.
fn watch_for_git_init(
    root: &Path,
    sender: Sender<GitStateMessage>,
) -> notify::Result<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        let git_directory_appeared = git_directory_lifecycle_changed(event.kind, &event.paths);
        if git_directory_appeared {
            let _ = sender.send(GitStateMessage::GitDirectoryChanged);
        }
    })?;
    watcher.watch(root, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

/// The repo-present watch: report touches of `HEAD`/`config` in the state dirs,
/// and of `.gitignore` in the root's ancestor dirs (whose rules the workspace
/// inherits but whose events the recursive feed watch can't see). Never the
/// whole `.git` tree - `objects/` alone turns a busy repo into an event
/// firehose; a non-recursive watch on each dir sees exactly its top-level
/// entries, and the name filters drop the rest (`index`, lockfiles,
/// `FETCH_HEAD`, sibling files, …) before they cost anything.
fn watch_git_state(
    root: &Path,
    directories: &[PathBuf],
    ancestor_directories: &[PathBuf],
    sender: Sender<GitStateMessage>,
) -> notify::Result<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        if event.paths.iter().any(|path| is_git_state_file(path)) {
            let _ = sender.send(GitStateMessage::StateTouched);
        }
        if event.paths.iter().any(|path| is_gitignore_file(path)) {
            let _ = sender.send(GitStateMessage::IgnoreRulesTouched);
        }
        if git_directory_lifecycle_changed(event.kind, &event.paths) {
            let _ = sender.send(GitStateMessage::GitDirectoryChanged);
        }
    })?;
    for directory in directories {
        watcher.watch(directory, RecursiveMode::NonRecursive)?;
    }
    for directory in ancestor_directories {
        // Best-effort: an unwatchable ancestor only costs its rules' liveness.
        let _ = watcher.watch(directory, RecursiveMode::NonRecursive);
    }
    watcher.watch(root, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

/// The directories between the repo toplevel and the watch root (both ends'
/// dirs included, the root itself excluded - its own subtree is already
/// watched recursively) whose `.gitignore` files apply to the workspace. Empty
/// when the root IS the toplevel, or when there is no repo.
fn gitignore_ancestor_directories(root: &Path) -> Vec<PathBuf> {
    let Some(toplevel) = repository_toplevel(root) else {
        return Vec::new();
    };
    root.ancestors()
        .skip(1)
        .take_while(|ancestor| ancestor.starts_with(&toplevel))
        .map(Path::to_path_buf)
        .collect()
}

/// The repo work-tree toplevel for `root`, normalized to native components
/// (git prints forward slashes on Windows). `None` outside a repo.
fn repository_toplevel(root: &Path) -> Option<PathBuf> {
    let output = git_in(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let printed = String::from_utf8_lossy(&output.stdout);
    let trimmed = printed.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(Path::new(trimmed).components().collect())
}

/// One thread per armed git-state watch: collapse each burst of messages into a
/// single `git://state` emission, and, when the burst says `git init` just ran,
/// swap the root sentinel for the real `HEAD`/`config` watch first. The
/// re-arm happens on this thread, never inside the notify callback (a watcher
/// must not be dropped from its own event callback). The thread retires itself:
/// re-arming (or dropping the project watch) drops the watcher holding the
/// channel's sender, so `recv` errors and the loop ends. Because messages
/// buffered before that drop still arrive, each burst first confirms the
/// project watch is still alive and armed - a released or dropped project must
/// neither re-arm nor emit.
fn spawn_git_state_coalescer(
    app: AppHandle,
    project: Weak<ProjectWatch>,
    receiver: Receiver<GitStateMessage>,
) {
    thread::spawn(move || {
        while let Ok(first) = receiver.recv() {
            let burst = drain_burst(&receiver, first, GIT_STATE_COALESCE);
            // The window may have let go of this project (or closed and been
            // pruned); then its `ProjectWatch` is gone and this thread retires.
            let Some(watch) = project.upgrade() else {
                return;
            };
            {
                let Ok(mut guard) = watch.git_state.lock() else {
                    return;
                };
                if guard.is_none() {
                    return;
                }
                if burst.rearm {
                    let plan = GitStatePlan::for_root(&watch.root);
                    rearm_git_state_locked(&app, &watch, &plan, &mut guard);
                }
            }
            if burst.rearm || burst.ignore_touched {
                // A `git init` flips the feed's ignore policy from the static
                // fallback to git's own rules; a touched ancestor `.gitignore`
                // changes the rules in place. Both re-filter the events already
                // shown. Outside the `git_state` lock - the refresh takes the
                // policy locks instead.
                refresh_ignore_policy(&app, &watch);
            }
            if burst.rearm || burst.state_touched {
                let _ = app.emit_to(
                    &watch.label,
                    GIT_STATE_EVENT,
                    ProjectRootPayload {
                        root: watch.requested_root.clone(),
                    },
                );
            }
        }
    });
}

/// Keep receiving until the channel stays quiet for `window`, folding every
/// message into the burst's distinct effects (git init creates `.git`, `HEAD`,
/// and `config` in one quick flurry - one coalesced re-arm; a `.gitignore`
/// save alongside a checkout still refreshes the rules AND re-emits state).
fn drain_burst(
    receiver: &Receiver<GitStateMessage>,
    first: GitStateMessage,
    window: Duration,
) -> GitStateBurst {
    let mut burst = GitStateBurst::default();
    let mut fold = |message: GitStateMessage| match message {
        GitStateMessage::StateTouched => burst.state_touched = true,
        GitStateMessage::GitDirectoryChanged => burst.rearm = true,
        GitStateMessage::IgnoreRulesTouched => burst.ignore_touched = true,
    };
    fold(first);
    while let Ok(message) = receiver.recv_timeout(window) {
        fold(message);
    }
    burst
}

/// Announces that a picker-watched folder's contents changed (a project row
/// appeared or disappeared). Payload-free. This name is the frontend contract
/// (`src/lib/bridge.ts`).
const DIRECTORIES_CHANGED_EVENT: &str = "directories://changed";

/// Watch a set of folders (non-recursively) and report when anything inside one
/// appears or disappears - the project picker's eyes. It hands over the *parents*
/// of the rows it shows, never the rows themselves: a watch holds a handle on the
/// folder it watches, and a handle on a project would be the very thing stopping
/// that project from being deleted. Re-arming replaces the previous set.
#[tauri::command]
pub async fn watch_dirs(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    dirs: Vec<String>,
) -> Result<(), String> {
    let label = window.label().to_string();
    prune_closed_windows(&app, &state);
    let mut guard = state
        .directory_watchers
        .lock()
        .map_err(|error| error.to_string())?;
    // Drop this window's old watcher before opening the new one, so its handles
    // go with it; another window's picker watch is untouched.
    guard.remove(&label);
    if dirs.is_empty() {
        return Ok(());
    }

    let app_handle = app.clone();
    let callback_label = label.clone();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        // Only appearance/disappearance moves rows on the page; edits inside a
        // project are the Change Feed's business, not the picker's. A rename
        // counts - the old name is gone - and so does Explorer's delete, which
        // moves the folder into the Recycle Bin. Routed to the owning window
        // alone so a sibling's picker doesn't rescan on our news.
        let is_appearance_change = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
        );
        if is_appearance_change {
            let _ = app_handle.emit_to(&callback_label, DIRECTORIES_CHANGED_EVENT, ());
        }
    })
    .map_err(|error| error.to_string())?;

    for directory in dirs {
        // A folder that is already gone isn't an error - that's precisely the news
        // the caller is listening for, and the next re-arm won't ask for it again.
        let _ = watcher.watch(Path::new(&directory), RecursiveMode::NonRecursive);
    }

    guard.insert(label, watcher);
    Ok(())
}

/// Announces that a watched project's files changed, so its languages may
/// have. The payload is the project path, as the window spelled it.
const LANGUAGES_CHANGED_EVENT: &str = "languages://changed";

/// What one folder of a window's language watch reports for.
#[derive(Clone)]
enum LanguageRoute {
    /// A project's git folder, watched alone (non-recursively) because the
    /// window's Change Feed already watches the project's working tree: only
    /// its `index` or `HEAD` moving matters here.
    GitDirectory { project: String },
    /// A project's whole working tree, watched recursively: its git folders'
    /// `index`/`HEAD`, and any file its own ignore rules keep.
    WorkingTree {
        project: String,
        git_directories: Vec<PathBuf>,
        ignore_rules: Arc<IgnoreRules>,
    },
}

impl LanguageRoute {
    fn project(&self) -> &str {
        match self {
            Self::GitDirectory { project } | Self::WorkingTree { project, .. } => project,
        }
    }

    fn recursive_mode(&self) -> RecursiveMode {
        match self {
            Self::GitDirectory { .. } => RecursiveMode::NonRecursive,
            Self::WorkingTree { .. } => RecursiveMode::Recursive,
        }
    }
}

/// Why a watched path counts as news for a project's languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LanguageNews {
    /// The tracked set moved (a stage, commit or checkout): the index stamp
    /// already invalidates the count.
    TrackedSet,
    /// A file the census may count was written, created or removed: the index
    /// never sees it, so the project's working-tree generation must move.
    WorkingTree,
}

/// The `.git` files whose change means the tracked set (or what is checked
/// out) moved: the index on a stage or commit, `HEAD` on a checkout.
const GIT_TRACKED_SET_FILES: &[&str] = &["index", "HEAD"];

fn is_tracked_set_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| GIT_TRACKED_SET_FILES.contains(&name))
}

/// The git folders holding a project's index files - its own `.git` and each
/// submodule's - spelled under `root`, as a watch on `root` reports them. A
/// linked worktree (whose `.git` is a file) has none.
fn git_directories_of(root: &Path) -> Vec<PathBuf> {
    crate::vcs::tracked_set_indexes(root)
        .iter()
        .filter_map(|index| Some(index.parent()?.to_path_buf()))
        .collect()
}

/// Whether a change to `path`, seen under the folder `watched` that `route`
/// watches, is news for that project's languages. Build output, dependency
/// folders and git's own churn are not; neither is anything the project's
/// ignore rules skip - the census would never count it either.
fn language_news(watched: &Path, route: &LanguageRoute, path: &Path) -> Option<LanguageNews> {
    let is_in_git_directory = path.parent().is_some_and(|parent| match route {
        LanguageRoute::GitDirectory { .. } => parent == watched,
        LanguageRoute::WorkingTree {
            git_directories, ..
        } => git_directories.iter().any(|directory| directory == parent),
    });
    if is_in_git_directory && is_tracked_set_file(path) {
        return Some(LanguageNews::TrackedSet);
    }
    let LanguageRoute::WorkingTree { ignore_rules, .. } = route else {
        return None;
    };
    let relative = path.strip_prefix(watched).ok()?;
    if is_always_ignored(relative) {
        return None;
    }
    let is_rules_change = is_gitignore_file(path);
    if is_rules_change || !ignore_rules.is_installed() {
        ignore_rules.install(compute_ignore_policy(watched));
    }
    if !is_rules_change && ignore_rules.excludes(path) {
        return None;
    }
    Some(LanguageNews::WorkingTree)
}

/// The folders to watch for one followed project and what each reports for.
/// The window's Change Feed already watches each project it holds recursively
/// (with the same ignore rules) and reports its edits itself, so such a project
/// needs only its git folders; any other project gets one recursive watch on
/// its root. `feed_roots` holds the feed's roots as [`project_key`]s.
/// `reusable` hands back a route already live for the same root, so its
/// memoized ignore decisions survive a re-arm.
///
/// [`project_key`]: crate::workspace::ide::project_key
fn planned_language_routes(
    path: &str,
    feed_roots: &HashSet<String>,
    reusable: impl Fn(&Path) -> Option<LanguageRoute>,
) -> Vec<(PathBuf, LanguageRoute)> {
    let root: PathBuf = Path::new(path).components().collect();
    let is_covered_by_feed = feed_roots.contains(&crate::workspace::ide::project_key(&root));
    if is_covered_by_feed {
        return git_directories_of(&root)
            .into_iter()
            .map(|directory| {
                let route = LanguageRoute::GitDirectory {
                    project: path.to_string(),
                };
                (directory, route)
            })
            .collect();
    }
    if !root.is_dir() {
        return Vec::new();
    }
    let ignore_rules = match reusable(&root) {
        Some(LanguageRoute::WorkingTree { ignore_rules, .. }) => ignore_rules,
        _ => Arc::default(),
    };
    let route = LanguageRoute::WorkingTree {
        project: path.to_string(),
        git_directories: git_directories_of(&root),
        ignore_rules,
    };
    vec![(root, route)]
}

/// Report one language change to the window, first moving the project's
/// working-tree generation when the census's cache cannot see the change.
fn report_language_news(app: &AppHandle, label: &str, project: &str, news: LanguageNews) {
    if news == LanguageNews::WorkingTree {
        crate::workspace::ide::note_working_tree_change(Path::new(project));
    }
    let _ = app.emit_to(label, LANGUAGES_CHANGED_EVENT, project);
}

/// The notify callback of a window's language watch: route every path to each
/// watched folder above it (a nested project's change is news for its parent
/// too) and report what counts. The routes are cloned out first, so an ignore
/// check that shells git never holds up a re-arm.
fn handle_language_event(
    app: &AppHandle,
    label: &str,
    routes: &Mutex<HashMap<PathBuf, LanguageRoute>>,
    event: &Event,
) {
    if ChangeKind::from_event(event.kind).is_none() {
        return;
    }
    for path in &event.paths {
        let matches: Vec<(PathBuf, LanguageRoute)> = {
            let Ok(routes) = routes.lock() else {
                return;
            };
            path.ancestors()
                .skip(1)
                .filter_map(|folder| {
                    let route = routes.get(folder)?;
                    Some((folder.to_path_buf(), route.clone()))
                })
                .collect()
        };
        for (folder, route) in matches {
            if let Some(news) = language_news(&folder, &route, path) {
                report_language_news(app, label, route.project(), news);
            }
        }
    }
}

/// Aim the window's language watch at `requested` (the projects it follows, as
/// it spells them), re-deciding which of them the Change Feed covers. Folders
/// that stay keep their watch and memoized ignore rules, so a scroll that only
/// adds a row costs that row's watch alone. An empty list drops the watch.
fn rearm_language_watch(
    app: &AppHandle,
    state: &WatcherState,
    label: &str,
    requested: Vec<String>,
) -> Result<(), String> {
    let feed_roots: HashSet<String> = window_watches(state, label)
        .map(|watches| watches.snapshot())
        .unwrap_or_default()
        .iter()
        .map(|project| crate::workspace::ide::project_key(&project.root))
        .collect();
    let mut guard = state
        .language_watchers
        .lock()
        .map_err(|error| error.to_string())?;
    if requested.is_empty() {
        guard.remove(label);
        return Ok(());
    }
    if !guard.contains_key(label) {
        let routes: Arc<Mutex<HashMap<PathBuf, LanguageRoute>>> = Arc::default();
        let callback_routes = Arc::clone(&routes);
        let app_handle = app.clone();
        let callback_label = label.to_string();
        let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            let Ok(event) = result else { return };
            handle_language_event(&app_handle, &callback_label, &callback_routes, &event);
        })
        .map_err(|error| error.to_string())?;
        guard.insert(
            label.to_string(),
            LanguageWatch {
                watcher,
                routes,
                canonical_folders: HashMap::new(),
                requested: Vec::new(),
            },
        );
    }
    let Some(language_watch) = guard.get_mut(label) else {
        return Ok(());
    };

    // A snapshot, not the lock: notify delivers events on the thread that also
    // serves `watch`/`unwatch`, so holding the routes the callback waits on
    // across those calls could deadlock the two.
    let previous: HashMap<PathBuf, LanguageRoute> = language_watch
        .routes
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let planned: HashMap<PathBuf, LanguageRoute> = requested
        .iter()
        .flat_map(|path| {
            planned_language_routes(path, &feed_roots, |root| previous.get(root).cloned())
        })
        .collect();
    for folder in previous
        .keys()
        .filter(|folder| !planned.contains_key(*folder))
    {
        let _ = language_watch.watcher.unwatch(folder);
        language_watch.canonical_folders.remove(folder);
    }
    let mut live = HashMap::new();
    for (folder, route) in planned {
        // A folder that is already gone isn't an error: it simply reports nothing.
        let is_live = previous.contains_key(&folder)
            || language_watch
                .watcher
                .watch(&folder, route.recursive_mode())
                .is_ok();
        if !is_live {
            continue;
        }
        if !language_watch.canonical_folders.contains_key(&folder) {
            if let Ok(canonical) = std::fs::canonicalize(&folder) {
                language_watch
                    .canonical_folders
                    .insert(folder.clone(), canonical);
            }
        }
        live.insert(folder, route);
    }
    *language_watch
        .routes
        .lock()
        .map_err(|error| error.to_string())? = live;
    language_watch.requested = requested;
    Ok(())
}

/// Keep the language bars of the projects a window follows live - the rows on
/// screen plus the projects it is working in - by reporting each project whose
/// files changed: a stage, commit or checkout, or an edit, a new file or a
/// deletion its census would count. Each project the window holds rides its
/// Change Feed's recursive watch; every other project gets one recursive watch filtered
/// by the same ignore rules. The census itself runs only when the frontend asks.
/// Re-arming replaces the set; an empty list drops the watch.
#[tauri::command]
pub async fn watch_languages(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    paths: Vec<String>,
) -> Result<(), String> {
    prune_closed_windows(&app, &state);
    rearm_language_watch(&app, &state, window.label(), paths)
}

/// Re-decide which followed projects the window's Change Feed covers after its
/// set of held projects changes, so no project is left watched twice or not at all.
fn rearm_language_watch_after_feed(app: &AppHandle, state: &WatcherState, label: &str) {
    let requested = state
        .language_watchers
        .lock()
        .ok()
        .and_then(|guard| Some(guard.get(label)?.requested.clone()));
    if let Some(requested) = requested {
        if let Err(error) = rearm_language_watch(app, state, label, requested) {
            eprintln!("pade: could not re-aim the language watch: {error}");
        }
    }
}

/// One project watch's notify callback. Every surfaced change is routed back to
/// the owning window alone (`emit_to`), never broadcast to sibling windows, and
/// tagged with the project root as the frontend spelled it.
fn handle_event(app: &AppHandle, watch: &ProjectWatch, event: Event) {
    let Some(kind) = ChangeKind::from_event(event.kind) else {
        return;
    };
    let label = watch.label.as_str();

    for path in event.paths {
        if is_always_ignored(&path) {
            watch.ignored.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        // Both the directory test and the atomic-write check need metadata.
        // Resolve it once per path rather than issuing two filesystem queries for
        // every event in an editor's save burst.
        let metadata = std::fs::metadata(&path).ok();
        if metadata.as_ref().is_some_and(std::fs::Metadata::is_dir) {
            continue;
        }
        // A created/modified file that has already vanished was an atomic-write
        // scratch file - skip it rather than surface a preview-less phantom card.
        if !surfaces(kind, metadata.is_some()) {
            continue;
        }

        // The event kind describes the past; the disk describes now. An editor's
        // atomic save deletes the real path and renames a scratch file over it,
        // so a Remove event routinely loses the race with the recreation -
        // trusting it would card a live file as "Deleted" while its fresh
        // content produces a positive line delta. A file that exists is a
        // modification, whatever the event said.
        let kind = reclassify(kind, metadata.is_some());

        // A changed MCP config (`.mcp.json`) may have gained or lost a server;
        // if so, the affected agent's sessions restart to pick it up. Checked
        // before the ignore filter, since `.mcp.json` is often git-ignored.
        detect_mcp_change(app, watch, &path);

        // A changed .gitignore - edited, appearing for the first time, or
        // deleted - changes what counts as ignored, so rebuild the whole policy
        // (fresh rules in static mode, dropped memoized decisions in git mode)
        // and tell the feed to re-filter what it already shows. The .gitignore
        // itself is not ignored, so it still surfaces as its own change.
        if is_gitignore_file(&path) {
            refresh_ignore_policy(app, watch);
        }

        // Skip whatever the project itself would ignore: git's own ignore rules in
        // a repo, or the tech-inferred ignore directories when there is no git. The
        // baseline `ignored` pre-filter above already dropped the giant dirs, so a
        // repo never shells git for node_modules/target and each surviving path is
        // git-checked at most once (memoized).
        if watch.ignore_rules.excludes(&path) {
            watch.ignored.fetch_add(1, Ordering::Relaxed);
            continue;
        }

        // The language watch leaves this project's working tree to the feed
        // (see `planned_language_routes`), so a change the feed keeps is news
        // for its language bar too.
        report_language_news(
            app,
            label,
            &watch.root.to_string_lossy(),
            LanguageNews::WorkingTree,
        );

        // Debounce: editors emit bursts per save.
        {
            let Ok(mut seen) = watch.last_seen.lock() else {
                continue;
            };
            let now = Instant::now();
            let within_debounce = seen
                .get(&path)
                .is_some_and(|previous| now.duration_since(*previous) < SAVE_BURST_DEBOUNCE);
            if within_debounce {
                continue;
            }
            seen.insert(path.clone(), now);
        }

        // First-touch baseline for the git-free preview: the content this path
        // held the first time it changed this session. A creation baselines as
        // empty (its whole content is new this session); any other first sighting
        // snapshots the current on-disk text - the accepted slightly-late baseline
        // (the edit that fired this very event is already on disk). Later edits,
        // and a later deletion, diff against this. `or_insert_with` runs only on
        // the first sighting, so a large file is read at most once.
        record_first_touch_baseline(&watch.baselines, &path, || match kind {
            ChangeKind::Created => Some(String::new()),
            ChangeKind::Modified => read_preview_text(&watch.canonical_root, &path)
                .ok()
                .flatten(),
            ChangeKind::Deleted => read_preview_text(&watch.canonical_root, &path)
                .ok()
                .flatten()
                .or_else(|| crate::vcs::head_file_text(&watch.root, &path, MAXIMUM_PREVIEW_BYTES)),
        });

        let new_count = line_count(&path);
        let (added, removed) = {
            let Ok(mut counts) = watch.line_counts.lock() else {
                continue;
            };
            let old = counts.get(&path).copied();
            let delta = line_delta(old, new_count);
            if let Some(count) = new_count {
                counts.insert(path.clone(), count);
            } else {
                counts.remove(&path);
            }
            delta
        };

        let path_string = path.to_string_lossy().into_owned();
        let id = format!(
            "{}-{}",
            now_milliseconds(),
            CHANGE_EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let event = ChangeEvent {
            id,
            root: watch.requested_root.clone(),
            summary: summarize(kind, added, removed),
            path: path_string,
            kind,
            added,
            removed,
            timestamp: now_milliseconds(),
        };
        watch.surfaced.fetch_add(1, Ordering::Relaxed);
        let _ = app.emit_to(label, FEED_CHANGE_EVENT, event);
    }
}

/// Record `path`'s first-touch baseline unless it already has one. The
/// snapshot (a file read, or a `git show` for a deletion) runs outside the
/// lock, so a preview command reading the baselines never waits on that I/O;
/// the insert keeps any baseline recorded meanwhile, so the first one wins.
fn record_first_touch_baseline(
    baselines: &Mutex<HashMap<PathBuf, Option<String>>>,
    path: &Path,
    snapshot: impl FnOnce() -> Option<String>,
) {
    let is_recorded = baselines
        .lock()
        .map_or(true, |recorded| recorded.contains_key(path));
    if is_recorded {
        return;
    }
    let baseline = snapshot();
    if let Ok(mut recorded) = baselines.lock() {
        recorded.entry(path.to_path_buf()).or_insert(baseline);
    }
}

/// What one held project's Change Feed watch is doing right now - the panel states it
/// verbatim when the feed is empty, so "nothing has changed yet" is never
/// mistaken for "nothing is being watched".
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WatchStatus {
    /// The root the watch is armed on; `None` when this window doesn't watch it.
    root: Option<String>,
    /// Epoch ms the watch was armed - the feed holds nothing from before it.
    armed_at: Option<u64>,
    /// Changes surfaced to this window since arming.
    surfaced: u64,
    /// Changes the ignore rules dropped since arming. A climbing count under an
    /// empty feed says the watch is alive and the rules are eating everything.
    ignored: u64,
}

/// The live watch state of `root` in this window (see [`WatchStatus`]); the
/// default, empty status when the window doesn't watch that root. A pure
/// in-memory registry read, so it stays synchronous.
#[tauri::command]
pub fn watch_status(
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    root: String,
) -> WatchStatus {
    let Some(watch) =
        window_watches(&state, window.label()).and_then(|watches| watches.find(&root))
    else {
        return WatchStatus::default();
    };
    WatchStatus {
        root: Some(watch.root.to_string_lossy().into_owned()),
        armed_at: Some(watch.armed_at),
        surfaced: watch.surfaced.load(Ordering::Relaxed),
        ignored: watch.ignored.load(Ordering::Relaxed),
    }
}

/// The window's project watch that owns `path` (see [`WindowWatches::owner_of`]).
fn owning_watch(state: &WatcherState, label: &str, path: &Path) -> Option<Arc<ProjectWatch>> {
    window_watches(state, label)?.owner_of(path)
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
/// over the size cap - the card then shows "No preview available". Git-independent
/// by construction, so an untracked or ignored file previews like any other, and a
/// file deleted after being touched shows its baseline as a full removal.
#[tauri::command]
pub async fn feed_diff(
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    path: String,
) -> Result<Option<FeedDiff>, String> {
    let file = Path::new(&path);
    let Some(watch) = owning_watch(&state, window.label(), file) else {
        return Ok(None);
    };
    let before = {
        let baselines = watch.baselines.lock().map_err(|error| error.to_string())?;
        let Some(Some(text)) = baselines.get(file) else {
            return Ok(None);
        };
        text.clone()
    };

    let after = match read_preview_text(&watch.canonical_root, file) {
        Ok(Some(text)) => text,
        Ok(None) => String::new(),
        Err(()) => return Ok(None),
    };

    Ok(Some(FeedDiff { before, after }))
}

/// The current text of a watched `path`, for the Change Feed's markdown/HTML
/// Preview toggle: the same UTF-8, size-capped read [`feed_diff`] uses for its
/// `after`, but handed over on its own so the Preview renders the file as it is
/// now (the diff is baseline→current; the preview is just current). `None` when
/// the path was not surfaced by this window's watch this session (so the command
/// can't be turned into an arbitrary file reader), is gone, or is binary / over
/// [`MAXIMUM_PREVIEW_BYTES`] - the card then keeps its diff and offers no preview.
#[tauri::command]
pub async fn feed_text(
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    path: String,
) -> Result<Option<String>, String> {
    let file = Path::new(&path);
    let Some(watch) = owning_watch(&state, window.label(), file) else {
        return Ok(None);
    };
    if !was_surfaced(&watch, file)? {
        return Ok(None);
    }
    Ok(read_preview_text(&watch.canonical_root, file)
        .ok()
        .flatten())
}

/// Whether this window's watch surfaced `path` this session (it holds a
/// baseline entry for it). The gate every preview command passes first, so none
/// of them can be turned into a reader of an arbitrary file off disk.
fn was_surfaced(watch: &ProjectWatch, path: &Path) -> Result<bool, String> {
    let baselines = watch.baselines.lock().map_err(|error| error.to_string())?;
    Ok(baselines.contains_key(path))
}

/// The inline image preview for `path`: its bytes as a base64 `data:` URL tagged
/// with the extension's MIME type. `None` when `path` is not a previewable image,
/// was not surfaced by this window's watch this session, is missing / not a
/// regular file, or is larger than [`MAXIMUM_IMAGE_BYTES`] - the card then falls back
/// to its text summary. Reads the CURRENT bytes (what the image is now); a
/// before/after for a modified image is a deliberate non-goal here. Gated on a
/// captured baseline like `feed_diff`, so the command can't base64 an arbitrary
/// file off disk.
#[tauri::command]
pub async fn feed_image(
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    path: String,
) -> Result<Option<FeedImage>, String> {
    let file = Path::new(&path);
    let Some(watch) = owning_watch(&state, window.label(), file) else {
        return Ok(None);
    };
    if !was_surfaced(&watch, file)? {
        return Ok(None);
    }

    if image_mime_type(file).is_none() {
        return Ok(None);
    }
    let Ok(Some(bytes)) = read_authorized_file(&watch.canonical_root, file, MAXIMUM_IMAGE_BYTES)
    else {
        return Ok(None);
    };

    Ok(FeedImage::of(file, &bytes))
}

/// The video preview for `path`: its opening seconds transcoded by ffmpeg into a
/// small MP4 (see [`crate::platform::media`]). `None` when `path` is not a video, was not
/// surfaced by this window's watch this session, is gone, or ffmpeg couldn't
/// read it. Gated and root-confined exactly like [`feed_image`].
#[tauri::command]
pub async fn feed_video(
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    path: String,
) -> Result<Option<FeedVideo>, String> {
    let file = Path::new(&path);
    let Some(watch) = owning_watch(&state, window.label(), file) else {
        return Ok(None);
    };
    if !crate::platform::media::is_video(file) || !was_surfaced(&watch, file)? {
        return Ok(None);
    }
    let Ok(Some(source)) = authorize_path(&watch.canonical_root, file) else {
        return Ok(None);
    };

    video_preview(source).await
}

/// The subset of `paths` the current ignore policy excludes - how the frontend
/// re-filters the Change Feed it already rendered after `feed://ignore-changed`
/// (events for now-ignored paths are dropped; never-recorded ones can't be
/// resurrected, they simply surface again on their next real change). Each
/// path is judged by the rules of the held project that owns it.
#[tauri::command]
pub async fn feed_ignored(
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    paths: Vec<String>,
) -> Result<Vec<String>, String> {
    let projects = window_watches(&state, window.label())
        .map(|watches| watches.snapshot())
        .unwrap_or_default();
    Ok(paths
        .into_iter()
        .filter(|candidate| {
            let path = Path::new(candidate);
            is_always_ignored(path)
                || owning_project(&projects, path)
                    .is_some_and(|project| project.ignore_rules.excludes(path))
        })
        .collect())
}

pub fn init(app: &AppHandle) {
    app.manage(WatcherState::default());
}

#[cfg(test)]
mod tests {
    use super::{
        deepest_containing, language_news, planned_language_routes, watch_set_change, IgnorePolicy,
        IgnoreRules, LanguageNews, LanguageRoute, ProjectRootPayload, WatchSetChange,
    };
    use super::{
        drain_burst, git_directory_lifecycle_changed, git_state_directories,
        is_git_directory_entry, is_git_state_file, is_under_static_ignore_directory, line_count,
        line_delta, manifest_ignore_directories, mcp_config_for_path, mcp_membership_changed,
        read_authorized_file, read_preview_text, reclassify, record_first_touch_baseline,
        resolve_watch_root, static_ignore_directories, summarize, surfaces, unique_directories,
        ChangeKind, GitStateMessage, WindowWatches, MAXIMUM_IMAGE_BYTES, MAXIMUM_PREVIEW_BYTES,
    };
    use std::sync::Arc;

    /// `watch_projects` asks git before it locks, so a slow request can finish
    /// after a newer one; only the newest may apply its set, or a quick switch
    /// could leave the feed watching projects the window just let go of.
    #[test]
    fn only_the_newest_arm_request_may_arm() {
        let watches = WindowWatches::default();
        let older = watches.begin_arm_request();
        let newer = watches.begin_arm_request();
        assert!(!watches.is_latest_arm_request(older));
        assert!(watches.is_latest_arm_request(newer));
    }

    #[test]
    fn a_fresh_window_is_watching_nothing() {
        let watches = WindowWatches::default();
        assert!(watches.find("C:/anything").is_none());
        assert!(watches.owner_of(Path::new("C:/anything/file.rs")).is_none());
    }

    fn roots(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn a_new_set_arms_the_added_roots_and_drops_the_removed_ones() {
        let current = roots(&["C:/a", "C:/b"]);
        let change = watch_set_change(&current, &roots(&["C:/b", "C:/c"]));
        assert_eq!(
            change,
            WatchSetChange {
                to_arm: roots(&["C:/c"]),
                to_drop: roots(&["C:/a"]),
            }
        );
    }

    #[test]
    fn an_unchanged_set_neither_arms_nor_drops_anything() {
        let current = roots(&["C:/a", "C:/b"]);
        let change = watch_set_change(&current, &roots(&["C:/b", "C:/a"]));
        assert_eq!(change, WatchSetChange::default());
    }

    #[test]
    fn a_repeated_root_is_armed_once_and_an_empty_set_drops_everything() {
        let change = watch_set_change(&[], &roots(&["C:/a", "C:/a"]));
        assert_eq!(change.to_arm, roots(&["C:/a"]));

        let current = roots(&["C:/b", "C:/a"]);
        let change = watch_set_change(&current, &[]);
        assert_eq!(change.to_drop, roots(&["C:/a", "C:/b"]));
        assert_empty(&change.to_arm);
    }

    #[test]
    fn a_path_belongs_to_the_deepest_root_that_contains_it() {
        let monorepo = Path::new("C:/work/monorepo");
        let package = Path::new("C:/work/monorepo/packages/app");
        let other = Path::new("C:/work/other");
        let candidates = [
            (monorepo, "monorepo"),
            (package, "package"),
            (other, "other"),
        ];

        let owner = |path: &str| deepest_containing(candidates, Path::new(path));
        assert_eq!(
            owner("C:/work/monorepo/packages/app/src/main.ts"),
            Some("package")
        );
        assert_eq!(owner("C:/work/monorepo/README.md"), Some("monorepo"));
        assert_eq!(owner("C:/work/other/lib.rs"), Some("other"));
        assert_eq!(owner("C:/elsewhere/file.txt"), None);
    }

    #[test]
    fn per_project_events_carry_the_root_as_the_frontend_spelled_it() {
        let payload = ProjectRootPayload {
            root: "C:/work/app".to_string(),
        };
        assert_eq!(
            serde_json::to_string(&payload).expect("serialize payload"),
            r#"{"root":"C:/work/app"}"#
        );
    }

    #[test]
    fn a_sibling_with_a_shared_name_prefix_is_not_contained() {
        let candidates = [(Path::new("C:/work/app"), "app")];
        assert_eq!(
            deepest_containing(candidates, Path::new("C:/work/app-old/file.txt")),
            None
        );
    }
    use crate::test_support::assert_empty;
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::channel;
    use std::sync::Mutex;
    use std::time::Duration;

    #[test]
    fn a_summary_says_what_happened_without_repeating_the_file_name() {
        assert_eq!(summarize(ChangeKind::Created, 0, 0), "New file");
        assert_eq!(summarize(ChangeKind::Deleted, 0, 0), "Deleted");
        assert_eq!(summarize(ChangeKind::Modified, 42, 0), "Grew by 42 lines");
        assert_eq!(summarize(ChangeKind::Modified, 1, 0), "Grew by 1 line");
        assert_eq!(summarize(ChangeKind::Modified, 0, 3), "Trimmed 3 lines");
        assert_eq!(summarize(ChangeKind::Modified, 5, 2), "Edited");
    }

    #[test]
    fn the_first_baseline_wins_and_a_recorded_path_is_never_snapshotted_again() {
        let baselines = Mutex::new(HashMap::new());
        let path = Path::new("/project/src/app.ts");

        record_first_touch_baseline(&baselines, path, || Some("first".to_string()));
        record_first_touch_baseline(&baselines, path, || {
            panic!("a recorded path must not be snapshotted again")
        });

        let recorded = baselines.lock().expect("baselines");
        assert_eq!(recorded.get(path), Some(&Some("first".to_string())));
    }

    #[test]
    fn an_unsnapshottable_first_sighting_is_still_recorded() {
        let baselines = Mutex::new(HashMap::new());
        let path = Path::new("/project/assets/logo.png");
        record_first_touch_baseline(&baselines, path, || None);
        assert_eq!(baselines.lock().expect("baselines").get(path), Some(&None));
    }

    #[test]
    fn an_mcp_config_matches_only_its_exact_root_relative_path() {
        let root = Path::new("/project");
        let config = mcp_config_for_path(root, &root.join(".mcp.json")).expect("root config");
        assert_eq!(config.relative_path, ".mcp.json");
        let nested =
            mcp_config_for_path(root, &root.join(".cursor").join("mcp.json")).expect("nested");
        assert_eq!(nested.relative_path, ".cursor/mcp.json");
        assert!(mcp_config_for_path(root, &root.join("src").join(".mcp.json")).is_none());
        assert!(mcp_config_for_path(root, Path::new("/elsewhere/.mcp.json")).is_none());
    }

    #[test]
    fn change_kinds_serialize_to_the_lowercase_wire_strings() {
        let kinds = [
            ChangeKind::Created,
            ChangeKind::Modified,
            ChangeKind::Deleted,
        ];
        assert_eq!(
            serde_json::to_string(&kinds).expect("serialize kinds"),
            r#"["created","modified","deleted"]"#
        );
    }

    #[test]
    fn creating_or_deleting_an_empty_mcp_file_changes_membership_state() {
        let missing = crate::workspace::mcp::McpSnapshot::default();
        let empty_file = crate::workspace::mcp::McpSnapshot {
            exists: true,
            ..crate::workspace::mcp::McpSnapshot::default()
        };
        assert!(mcp_membership_changed(&missing, &empty_file));
        assert!(mcp_membership_changed(&empty_file, &missing));
    }

    #[test]
    fn editing_only_an_mcp_definition_does_not_change_membership_state() {
        let previous = crate::workspace::mcp::McpSnapshot {
            exists: true,
            names: ["github".to_string()].into_iter().collect(),
            content: "old".to_string(),
        };
        let current = crate::workspace::mcp::McpSnapshot {
            content: "new".to_string(),
            ..previous.clone()
        };
        assert!(!mcp_membership_changed(&previous, &current));
    }

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

    #[test]
    fn a_deleted_path_that_exists_again_was_an_atomic_save_and_reads_as_modified() {
        assert!(matches!(
            reclassify(ChangeKind::Deleted, true),
            ChangeKind::Modified
        ));
    }

    #[test]
    fn a_genuinely_gone_path_keeps_its_deleted_kind() {
        assert!(matches!(
            reclassify(ChangeKind::Deleted, false),
            ChangeKind::Deleted
        ));
        assert!(matches!(
            reclassify(ChangeKind::Modified, true),
            ChangeKind::Modified
        ));
        assert!(matches!(
            reclassify(ChangeKind::Created, true),
            ChangeKind::Created
        ));
    }

    #[test]
    fn a_deletion_of_a_tracked_file_removes_every_line_it_had() {
        assert_eq!(line_delta(Some(764), None), (0, 764));
    }

    #[test]
    fn line_deltas_split_growth_shrink_first_sighting_and_untracked_deletion() {
        assert_eq!(line_delta(Some(10), Some(14)), (4, 0));
        assert_eq!(line_delta(Some(14), Some(10)), (0, 4));
        assert_eq!(line_delta(None, Some(9)), (9, 0));
        assert_eq!(line_delta(None, None), (0, 0));
    }

    /// A scratch directory unique to this test process; removed by the caller.
    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("pade-watcher-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("create scratch directory");
        directory
    }

    #[test]
    fn read_preview_text_returns_utf8_file_contents() {
        let directory = scratch("text");
        let file = directory.join("a.txt");
        fs::write(&file, b"line one\nline two\n").expect("write file");
        let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");
        assert_eq!(
            read_preview_text(&canonical_root, &file)
                .expect("read preview")
                .as_deref(),
            Some("line one\nline two\n"),
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn read_preview_text_skips_binary_content() {
        let directory = scratch("binary");
        let file = directory.join("a.bin");
        fs::write(&file, b"before\x00after").expect("write file");
        let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");
        assert!(read_preview_text(&canonical_root, &file).is_err());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn read_preview_text_skips_files_over_the_cap() {
        let directory = scratch("large");
        let file = directory.join("a.txt");
        let over_capacity =
            usize::try_from(MAXIMUM_PREVIEW_BYTES).expect("capacity fits usize") + 1;
        fs::write(&file, vec![b'x'; over_capacity]).expect("write file");
        let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");
        assert!(read_preview_text(&canonical_root, &file).is_err());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn read_preview_text_is_none_for_a_missing_path() {
        let directory = scratch("missing");
        let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");
        assert_eq!(
            read_preview_text(&canonical_root, &directory.join("nope.txt")),
            Ok(None)
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn preview_reads_reject_a_target_outside_the_watch_root() {
        let directory = scratch("authorized-root");
        let outside = scratch("authorized-outside").join("secret.txt");
        fs::write(&outside, b"secret").expect("write outside file");
        let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");

        assert!(read_authorized_file(&canonical_root, &outside, MAXIMUM_PREVIEW_BYTES).is_err());

        let outside_directory = outside.parent().expect("outside file has parent");
        let _ = fs::remove_dir_all(outside_directory);
        let _ = fs::remove_dir_all(&directory);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn preview_reads_reject_a_symlink_or_reparse_point_escape() {
        let directory = scratch("symlink-root");
        let outside_directory = scratch("symlink-outside");
        let outside = outside_directory.join("secret.txt");
        let link = directory.join("escape.txt");
        fs::write(&outside, b"secret").expect("write outside file");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).expect("create symlink");
        #[cfg(windows)]
        if std::os::windows::fs::symlink_file(&outside, &link).is_err() {
            let _ = fs::remove_dir_all(&outside_directory);
            let _ = fs::remove_dir_all(&directory);
            return;
        }

        let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");
        assert!(read_authorized_file(&canonical_root, &link, MAXIMUM_PREVIEW_BYTES).is_err());

        let _ = fs::remove_dir_all(&outside_directory);
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn preview_reads_enforce_the_image_cap_from_the_open_handle() {
        let directory = scratch("image-cap");
        let file = directory.join("large.png");
        let opened = fs::File::create(&file).expect("create image");
        opened
            .set_len(MAXIMUM_IMAGE_BYTES + 1)
            .expect("extend image beyond cap");
        let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");

        assert!(read_authorized_file(&canonical_root, &file, MAXIMUM_IMAGE_BYTES).is_err());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn line_count_matches_string_lines_and_rejects_invalid_utf8() {
        let directory = scratch("line-count");
        let text = directory.join("source.txt");
        fs::write(&text, b"one\ntwo\n\n").expect("write text file");
        assert_eq!(line_count(&text), Some(3));

        let binary = directory.join("binary.bin");
        fs::write(&binary, b"before\xffafter").expect("write binary file");
        assert_eq!(line_count(&binary), None);
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn resolve_watch_root_accepts_an_existing_directory() {
        let directory = scratch("root");
        let path = directory.to_string_lossy();
        assert!(resolve_watch_root(&path).is_ok_and(|root| root.is_dir()));
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn resolve_watch_root_rejects_a_missing_path() {
        let directory = scratch("root-missing");
        let path = directory.join("nope").to_string_lossy().into_owned();
        assert!(resolve_watch_root(&path).is_err());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn resolve_watch_root_rejects_a_file() {
        let directory = scratch("root-file");
        let file = directory.join("a.txt");
        fs::write(&file, b"x").expect("write file");
        let path = file.to_string_lossy().into_owned();
        assert!(resolve_watch_root(&path).is_err());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_node_manifest_maps_to_node_modules_and_friends() {
        let directories = manifest_ignore_directories("package.json");
        assert!(directories.contains(&"node_modules"));
        assert!(directories.contains(&"dist"));
        // A lockfile alone implies the same node ignore set.
        assert!(manifest_ignore_directories("pnpm-lock.yaml").contains(&"node_modules"));
    }

    #[test]
    fn a_cargo_manifest_maps_to_target() {
        assert!(manifest_ignore_directories("Cargo.toml").contains(&"target"));
    }

    #[test]
    fn a_dotnet_project_file_maps_to_bin_and_obj_by_extension() {
        let directories = manifest_ignore_directories("MyApp.csproj");
        assert!(directories.contains(&"bin"));
        assert!(directories.contains(&"obj"));
    }

    #[test]
    fn an_unrecognized_file_implies_no_ignore_directories() {
        assert_empty(manifest_ignore_directories("README.md"));
    }

    #[test]
    fn static_ignore_directories_unions_manifest_directories_with_the_baseline() {
        let directory = scratch("static-node");
        fs::write(directory.join("package.json"), b"{}").expect("write manifest");
        let directories = static_ignore_directories(&directory);
        assert!(directories.contains("node_modules"));
        // The baseline is always present, so static mode never weakens the
        // always-on pre-filter.
        assert!(directories.contains(".git"));
        assert!(directories.contains("target"));
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn static_ignore_directories_for_a_bare_folder_is_just_the_baseline() {
        let directory = scratch("static-bare");
        let directories = static_ignore_directories(&directory);
        assert!(directories.contains(".git"));
        assert!(directories.contains("node_modules"));
        // No manifest present, so nothing beyond the baseline is added.
        assert!(!directories.contains(".gradle"));
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn head_and_config_are_the_git_state_files() {
        assert!(is_git_state_file(Path::new(r"C:\repo\.git\HEAD")));
        assert!(is_git_state_file(Path::new(r"C:\repo\.git\config")));
    }

    #[test]
    fn other_git_directory_entries_are_not_git_state_files() {
        assert!(!is_git_state_file(Path::new(r"C:\repo\.git\index")));
        assert!(!is_git_state_file(Path::new(r"C:\repo\.git\ORIG_HEAD")));
        assert!(!is_git_state_file(Path::new(r"C:\repo\.git\config.lock")));
        assert!(!is_git_state_file(Path::new(r"C:\repo\.git\packed-refs")));
    }

    #[test]
    fn only_a_dot_git_entry_announces_the_repository_appearing() {
        assert!(is_git_directory_entry(Path::new(r"C:\repo\.git")));
        assert!(!is_git_directory_entry(Path::new(r"C:\repo\.gitignore")));
        assert!(!is_git_directory_entry(Path::new(r"C:\repo\src")));
    }

    #[test]
    fn git_directory_creation_and_removal_both_rearm_the_detector() {
        let paths = vec![PathBuf::from(r"C:\repo\.git")];
        assert!(git_directory_lifecycle_changed(
            notify::EventKind::Create(notify::event::CreateKind::Any),
            &paths
        ));
        assert!(git_directory_lifecycle_changed(
            notify::EventKind::Remove(notify::event::RemoveKind::Any),
            &paths
        ));
        assert!(!git_directory_lifecycle_changed(
            notify::EventKind::Modify(notify::event::ModifyKind::Any),
            &paths
        ));
    }

    #[test]
    fn unique_directories_trims_dedupes_and_skips_empty_lines() {
        let directories = unique_directories(
            [" C:/repo/.git ", "", "C:/repo/.git", "C:/common/.git"].into_iter(),
        );
        assert_eq!(
            directories,
            vec![
                PathBuf::from("C:/repo/.git"),
                PathBuf::from("C:/common/.git")
            ]
        );
    }

    #[test]
    fn a_workspace_with_a_git_directory_watches_exactly_that_directory() {
        let directory = scratch("git-state-repo");
        let git_directory = directory.join(".git");
        fs::create_dir_all(&git_directory).expect("create .git directory");
        assert_eq!(git_state_directories(&directory), vec![git_directory]);
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_workspace_without_git_yields_no_state_directories() {
        let directory = scratch("git-state-bare");
        assert_empty(&git_state_directories(&directory));
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_quiet_channel_returns_the_first_message() {
        let (_sender, receiver) = channel::<GitStateMessage>();
        let burst = drain_burst(
            &receiver,
            GitStateMessage::StateTouched,
            Duration::from_millis(5),
        );
        assert!(burst.state_touched);
        assert!(!burst.rearm);
        assert!(!burst.ignore_touched);
    }

    /// A busy burst asks for each distinct effect it saw - a `git init` re-arm
    /// doesn't swallow a state emit, nor a `.gitignore` refresh.
    #[test]
    fn a_burst_coalesces_every_distinct_effect() {
        let (sender, receiver) = channel::<GitStateMessage>();
        sender
            .send(GitStateMessage::GitDirectoryChanged)
            .expect("send buffered message");
        sender
            .send(GitStateMessage::IgnoreRulesTouched)
            .expect("send buffered message");
        drop(sender);
        let burst = drain_burst(
            &receiver,
            GitStateMessage::StateTouched,
            Duration::from_millis(5),
        );
        assert!(burst.rearm);
        assert!(burst.state_touched);
        assert!(burst.ignore_touched);
    }

    #[test]
    fn a_path_under_an_ignored_directory_is_excluded_in_static_mode() {
        let mut directories = HashSet::new();
        directories.insert("node_modules".to_string());
        let path = Path::new("project/node_modules/react/index.js");
        assert!(is_under_static_ignore_directory(path, &directories));
    }

    #[test]
    fn a_normal_source_path_is_not_excluded_in_static_mode() {
        let mut directories = HashSet::new();
        directories.insert("node_modules".to_string());
        let path = Path::new("project/src/app.ts");
        assert!(!is_under_static_ignore_directory(path, &directories));
    }

    fn working_tree_route(root: &Path) -> LanguageRoute {
        let ignore_rules = IgnoreRules::default();
        ignore_rules.install(IgnorePolicy::Static {
            root: root.to_path_buf(),
            directories: static_ignore_directories(root),
            rules: crate::workspace::gitignore::Rules::parse("*.log\n"),
        });
        LanguageRoute::WorkingTree {
            project: root.to_string_lossy().into_owned(),
            git_directories: vec![root.join(".git")],
            ignore_rules: Arc::new(ignore_rules),
        }
    }

    #[test]
    fn a_working_tree_edit_is_language_news_but_ignored_output_is_not() {
        let root = std::env::temp_dir().join(format!("pade-language-news-{}", std::process::id()));
        let route = working_tree_route(&root);
        let news = |relative: &str| language_news(&root, &route, &root.join(relative));

        assert_eq!(news("src/main.rs"), Some(LanguageNews::WorkingTree));
        assert_eq!(news(".git/index"), Some(LanguageNews::TrackedSet));
        assert_eq!(news(".git/objects/ab"), None);
        assert_eq!(news("node_modules/left-pad/index.js"), None);
        assert_eq!(news("debug.log"), None);
    }

    #[test]
    fn a_git_folder_route_reports_only_its_tracked_set_files() {
        let git_directory = std::env::temp_dir().join("pade-language-git").join(".git");
        let route = LanguageRoute::GitDirectory {
            project: "project".to_string(),
        };
        let news = |name: &str| language_news(&git_directory, &route, &git_directory.join(name));

        assert_eq!(news("index"), Some(LanguageNews::TrackedSet));
        assert_eq!(news("HEAD"), Some(LanguageNews::TrackedSet));
        assert_eq!(news("config"), None);
    }

    #[test]
    fn the_feeds_project_needs_only_its_git_folder_and_any_other_a_recursive_watch() {
        let root = std::env::temp_dir().join(format!("pade-language-plan-{}", std::process::id()));
        fs::create_dir_all(root.join(".git")).expect("git dir");
        fs::write(root.join(".git").join("index"), "").expect("index");
        let path = root.to_string_lossy().into_owned();
        let other_project = std::env::temp_dir().join("pade-language-plan-other");
        let feed_roots: HashSet<String> = [&root, &other_project]
            .into_iter()
            .map(|project| crate::workspace::ide::project_key(project))
            .collect();

        let covered = planned_language_routes(&path, &feed_roots, |_| None);
        assert!(matches!(
            covered.as_slice(),
            [(folder, LanguageRoute::GitDirectory { .. })] if *folder == root.join(".git")
        ));

        let existing = working_tree_route(&root);
        let uncovered = planned_language_routes(&path, &HashSet::new(), |_| Some(existing.clone()));
        let [(folder, LanguageRoute::WorkingTree { ignore_rules, .. })] = uncovered.as_slice()
        else {
            panic!("expected one recursive working-tree watch");
        };
        assert_eq!(*folder, root);
        let LanguageRoute::WorkingTree {
            ignore_rules: existing_rules,
            ..
        } = &existing
        else {
            unreachable!();
        };
        assert!(
            Arc::ptr_eq(ignore_rules, existing_rules),
            "a re-arm keeps the memoized rules"
        );

        fs::remove_dir_all(&root).expect("cleanup");
    }
}
