//! Filesystem watcher feeding the Change Feed.
//!
//! MVP: watches the opened project, ignores build/VCS noise, and turns each
//! save into a `ChangeEvent` with a line-count delta and a heuristic summary.
//! Later: real per-hunk diffs and agent-authored intent replace the heuristic.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// The Change Feed's live watcher plus the root it is armed on. The root is what
/// lets `watch_start` detect a project switch and re-arm; the watcher handle is
/// held only so dropping it stops the watch.
struct ProjectWatcher {
    root: PathBuf,
    _watcher: RecommendedWatcher,
}

/// The live-git-state watcher plus the root it is armed on. The root lets a
/// coalescer thread confirm it still speaks for the current workspace (a
/// re-root leaves the old thread with buffered messages it must not act on);
/// dropping the watcher stops the watch, and dropping its channel sender with
/// it retires the paired coalescer thread once that thread drains.
struct GitStateWatcher {
    root: PathBuf,
    _watcher: RecommendedWatcher,
}

/// One window's Change Feed watch and its per-file bookkeeping. Every PADE window
/// opens its own workspace, but all windows share one backend process, so this
/// state is held per window (keyed by window label in [`WatcherState`]). A single
/// global slot could arm only one watcher at a time — a second window's
/// `watch_start` tore down the first window's watch (so its edits never surfaced)
/// and the one live watcher's changes broadcast into every window's feed. Keyed
/// per window, each window watches its own root and its events are emitted back
/// only to it (`emit_to` the owning label).
#[derive(Default)]
struct WindowWatch {
    watcher: Mutex<Option<ProjectWatcher>>,
    /// The workspace's live-git-state watcher — see `arm_git_state`. Separate
    /// from the Change Feed's recursive watcher on purpose: it watches only the
    /// dir(s) holding `.git/HEAD` + `.git/config`, non-recursively, so git's
    /// object churn never reaches it.
    git_state: Mutex<Option<GitStateWatcher>>,
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
    /// How the current watch root decides a path is ignored, computed once per
    /// `watch_start` and recomputed on a project switch. `None` before the first
    /// arm. Git mode defers to git's own ignore rules; static mode carries a
    /// tech-inferred set of directory names to exclude (see [`IgnorePolicy`]).
    ignore_policy: Mutex<Option<IgnorePolicy>>,
    /// Memoized `git check-ignore` results per absolute path (git mode only), so a
    /// path shells git at most once. Cleared on a re-root and whenever a
    /// `.gitignore` changes, since editing its rules can flip any path's state.
    git_ignore_cache: Mutex<HashMap<PathBuf, bool>>,
    /// The set of declared MCP server names last seen in each MCP config file
    /// (absolute path → server-name set), the baseline the change detector diffs
    /// against. An agent only picks up an added/removed server by restarting, so
    /// a change to this SET (not a value-only edit) drives `mcp://changed`. Seeded
    /// on `watch_start` and re-scoped to the new root on a project switch.
    mcp_servers: Mutex<HashMap<PathBuf, BTreeSet<String>>>,
}

#[derive(Default)]
pub struct WatcherState {
    /// One live Change Feed watch per window, keyed by window label. Shared behind
    /// an `Arc` so a callback can lift its window's bookkeeping out under a brief
    /// map lock, then work the fine-grained inner mutexes without holding it.
    windows: Mutex<HashMap<String, Arc<WindowWatch>>>,
    /// The picker's folder watcher — see `watch_dirs`. Also per window: each
    /// window has its own picker, watching its own folders, and its
    /// appearance/disappearance events route back only to it.
    dirs: Mutex<HashMap<String, RecommendedWatcher>>,
}

/// This window's watch bookkeeping, created empty on first use. `None` only when
/// the state lock is poisoned — a fault resolves to "no watch" so a caller bails
/// rather than panicking on a background thread.
fn window_watch(state: &WatcherState, label: &str) -> Option<Arc<WindowWatch>> {
    let mut windows = state.windows.lock().ok()?;
    Some(Arc::clone(windows.entry(label.to_string()).or_default()))
}

/// Drop the watch bookkeeping of every window that has since closed. There is no
/// per-window teardown hook (windows share one process), so `watch_start` and
/// `watch_dirs` prune here on each call: dropping an entry drops its watchers,
/// which stops the OS watch and retires the paired git-state coalescer thread.
/// Keeps the maps bounded to live windows over a long multi-window session.
fn prune_closed_windows(app: &AppHandle, state: &WatcherState) {
    let live: HashSet<String> = app.webview_windows().into_keys().collect();
    if let Ok(mut windows) = state.windows.lock() {
        windows.retain(|label, _| live.contains(label));
    }
    if let Ok(mut dirs) = state.dirs.lock() {
        dirs.retain(|label, _| live.contains(label));
    }
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

/// The git metadata entry a workspace gains when `git init` runs — the one
/// authoritative spelling of the `.git` name.
const GIT_DIR_NAME: &str = ".git";

const IGNORED: &[&str] = &[
    GIT_DIR_NAME,
    "node_modules",
    "target",
    "dist",
    "build",
    ".svelte-kit",
    ".ade",
    ".vite",
];

fn ignored(path: &Path) -> bool {
    path_component_matches(path, |name| IGNORED.contains(&name))
}

/// Whether any component of `path` names an ignored directory, decided by the
/// `is_ignored_name` predicate. One authoritative component scan shared by the
/// always-on baseline [`ignored`] pre-filter (against the [`IGNORED`] slice) and
/// static-mode exclusion (against a tech-inferred [`HashSet`]).
fn path_component_matches(path: &Path, is_ignored_name: impl Fn(&str) -> bool) -> bool {
    path.components()
        .filter_map(|component| component.as_os_str().to_str())
        .any(is_ignored_name)
}

/// Static-mode exclusion: whether `path` lies under any directory named in `dirs`.
fn ignored_by_static_dirs(path: &Path, dirs: &HashSet<String>) -> bool {
    path_component_matches(path, |name| dirs.contains(name))
}

/// How the Change Feed decides a path is "ignored", fixed once per watch root by
/// `watch_start` and stored in [`WatcherState::ignore_policy`].
///
/// A git work tree defers to git itself via `git check-ignore`, which honors
/// nested `.gitignore` files, `.git/info/exclude`, the global `core.excludesFile`,
/// and negation rules — exactly what the user's git already ignores, rather than a
/// brittle hand-rolled parse. A folder that is not a git repo has none of those
/// tools, so it falls back to a set of directory names inferred from the manifest
/// files at the root (see [`manifest_ignore_dirs`]) unioned with the [`IGNORED`]
/// baseline — plus the root `.gitignore`'s own rules when the folder has one (a
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
        dirs: HashSet<String>,
        rules: crate::gitignore::Rules,
    },
}

/// The ignore-rules file whose edits invalidate the git-check-ignore cache: when
/// its rules change, any path's ignored state can flip.
const GITIGNORE_FILE_NAME: &str = ".gitignore";

/// Announces that the ignore rules themselves changed (a `.gitignore` touched,
/// a `git init` flipping the policy). Payload-free: the frontend re-asks
/// `feed_ignored` about the events it holds, so the answer is never stale.
const FEED_IGNORE_EVENT: &str = "feed://ignore-changed";

// ── Live MCP config ───────────────────────────────────────────────────────────
// A project's MCP servers live in a config file (`.mcp.json` for Claude, per
// config.rs). A running agent only picks up an **added or removed** server by
// restarting — Claude re-reads `.mcp.json` at launch, and there is no in-session
// reload — while a value-only edit to an existing server the user reconnects by
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
    /// The agent commands whose servers this file declares (e.g. `["claude"]`).
    agents: Vec<String>,
    /// Server names gained since the last seen set.
    added: Vec<String>,
    /// Server names lost since the last seen set.
    removed: Vec<String>,
}

/// The agents whose MCP servers `path` declares, if it is a known MCP config
/// file under `root` (e.g. `<root>/.mcp.json` → `["claude"]`). `None` for any
/// other path. Matches the full relative path, so a nested config
/// (`.cursor/mcp.json`) can't be confused with a root one.
fn mcp_agents_for_path(root: &Path, path: &Path) -> Option<Vec<String>> {
    crate::config::mcp_configs()
        .find(|config| root.join(config.rel) == path)
        .map(|config| {
            config
                .agents
                .iter()
                .map(|agent| (*agent).to_string())
                .collect()
        })
}

/// Seed the MCP server-name baseline for `root`: record the current set for each
/// known MCP config file, so the first later change diffs against the truth on
/// disk rather than an empty set (which would restart on project open). Replaces
/// any previous root's baseline.
fn snapshot_mcp_baseline(watch: &WindowWatch, root: &Path) {
    let fresh: HashMap<PathBuf, BTreeSet<String>> = crate::config::mcp_configs()
        .filter_map(|config| {
            let file = root.join(config.rel);
            crate::mcp::server_names(&file).map(|names| (file, names))
        })
        .collect();
    if let Ok(mut baseline) = watch.mcp_servers.lock() {
        *baseline = fresh;
    }
}

/// Detect an MCP server add/remove for a changed `path` and, when the set truly
/// changed, emit [`MCP_EVENT`] and advance the baseline. A value-only edit (same
/// names) and an unparseable half-written file both no-op, so a session restarts
/// only on a real membership change. Runs before the ignore filter, so a
/// `.gitignore`'d `.mcp.json` still triggers.
fn detect_mcp_change(app: &AppHandle, watch: &WindowWatch, label: &str, root: &Path, path: &Path) {
    let Some(agents) = mcp_agents_for_path(root, path) else {
        return;
    };
    let Some(current) = crate::mcp::server_names(path) else {
        return; // unparseable mid-write — wait for the complete save
    };

    let (added, removed) = {
        let Ok(mut baseline) = watch.mcp_servers.lock() else {
            return;
        };
        let previous = baseline.get(path).cloned().unwrap_or_default();
        if current == previous {
            return; // value-only edit, or no change — nothing to restart for
        }
        let added: Vec<String> = current.difference(&previous).cloned().collect();
        let removed: Vec<String> = previous.difference(&current).cloned().collect();
        baseline.insert(path.to_path_buf(), current);
        (added, removed)
    };

    let _ = app.emit_to(
        label,
        MCP_EVENT,
        McpChange {
            path: path.to_string_lossy().into_owned(),
            agents,
            added,
            removed,
        },
    );
}

/// The build- and dependency-output directories a given manifest file implies
/// should be excluded when there is no git to consult. The single authoritative
/// home for the manifest→ignore-directories mapping; each name is a real
/// per-ecosystem generated directory. Matched by exact file name, or by extension
/// for the .NET project/solution manifests. Returns an empty slice for a file that
/// implies nothing.
fn manifest_ignore_dirs(file_name: &str) -> &'static [&'static str] {
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

/// The set of directory names to exclude in static mode: the [`IGNORED`] baseline
/// (so static mode is never weaker than the always-on pre-filter) unioned with the
/// ignore directories implied by every manifest file sitting directly in `root`.
/// One cheap one-level directory scan, run once per `watch_start`.
fn static_ignore_dirs(root: &Path) -> HashSet<String> {
    let mut dirs: HashSet<String> = IGNORED.iter().map(|&name| name.to_string()).collect();
    let Ok(entries) = std::fs::read_dir(root) else {
        return dirs;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        for &directory in manifest_ignore_dirs(name) {
            dirs.insert(directory.to_string());
        }
    }
    dirs
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
        .map(|content| crate::gitignore::Rules::parse(&content))
        .unwrap_or_default();
    IgnorePolicy::Static {
        root: root.to_path_buf(),
        dirs: static_ignore_dirs(root),
        rules,
    }
}

/// Recompute the ignore policy for the active watch root and forget every
/// memoized git decision — run when the rules themselves may have changed (a
/// `.gitignore` edited/created/deleted anywhere under the root, or a
/// mid-session `git init` flipping the root from static to git mode). The
/// frontend hears [`FEED_IGNORE_EVENT`] and re-filters the events it already
/// shows against the fresh policy (`feed_ignored`).
fn refresh_ignore_policy(app: &AppHandle, watch: &WindowWatch, label: &str) {
    let Some(root) = ({
        let Ok(guard) = watch.watcher.lock() else {
            return;
        };
        guard.as_ref().map(|active| active.root.clone())
    }) else {
        return;
    };
    let policy = compute_ignore_policy(&root);
    if let Ok(mut guard) = watch.ignore_policy.lock() {
        *guard = Some(policy);
    }
    if let Ok(mut cache) = watch.git_ignore_cache.lock() {
        cache.clear();
    }
    let _ = app.emit_to(label, FEED_IGNORE_EVENT, ());
}

/// Whether `root` sits inside a git work tree, via `git -C <root> rev-parse
/// --is-inside-work-tree` (which prints `true` in a work tree). A missing git
/// binary, a non-repo folder, or a bare repo all resolve to `false`, routing
/// `watch_start` to the static tech-inference fallback instead. Runs git against
/// `root` (not PADE's process cwd) so it reflects the watched project.
fn is_git_work_tree(root: &Path) -> bool {
    let Ok(output) = crate::util::command("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output()
    else {
        return false;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
}

/// Whether the watch root's ignore policy excludes `path` from the Change Feed.
/// Git mode asks git (memoized per path); static mode matches the path's component
/// names against the tech-inferred ignore set. A poisoned lock, an unset policy, or
/// a git failure all resolve to "not excluded" so a fault never silently hides real
/// changes.
fn excluded_by_ignore_policy(watch: &WindowWatch, path: &Path) -> bool {
    let git_root = {
        let Ok(guard) = watch.ignore_policy.lock() else {
            return false;
        };
        match guard.as_ref() {
            None => return false,
            Some(IgnorePolicy::Static { root, dirs, rules }) => {
                return ignored_by_static_dirs(path, dirs) || gitignore_excludes(root, rules, path);
            }
            Some(IgnorePolicy::Git { root }) => root.clone(),
        }
    };
    git_excludes_path(watch, &git_root, path)
}

/// Whether the static policy's `.gitignore` rules ignore `path` — matched on
/// its root-relative, `/`-joined spelling. A path outside the root (or one that
/// isn't valid UTF-8) can't match any rule.
fn gitignore_excludes(root: &Path, rules: &crate::gitignore::Rules, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let Some(relative) = relative.to_str() else {
        return false;
    };
    rules.is_ignored(&relative.replace('\\', "/"))
}

/// Whether git considers `path` ignored under the work tree at `root`, memoized in
/// [`WatcherState::git_ignore_cache`] so each path shells git at most once. The
/// cache is cleared on a re-root and whenever a `.gitignore` changes.
fn git_excludes_path(watch: &WindowWatch, root: &Path, path: &Path) -> bool {
    if let Ok(cache) = watch.git_ignore_cache.lock() {
        if let Some(&is_ignored) = cache.get(path) {
            return is_ignored;
        }
    }
    let is_ignored = git_check_ignore(root, path);
    if let Ok(mut cache) = watch.git_ignore_cache.lock() {
        cache.insert(path.to_path_buf(), is_ignored);
    }
    is_ignored
}

/// Shell `git -C <root> check-ignore -q -- <path>` and report whether the path is
/// ignored: exit status 0 means git would ignore it. Because git evaluates the path
/// against every applicable ignore source itself, this correctly honors nested
/// `.gitignore` files, `.git/info/exclude`, `core.excludesFile`, and negations. Any
/// spawn failure resolves to `false` so a fault never hides a real change. Scoped to
/// `root` via `-C`, never PADE's process cwd.
fn git_check_ignore(root: &Path, path: &Path) -> bool {
    let Ok(output) = crate::util::command("git")
        .arg("-C")
        .arg(root)
        .arg("check-ignore")
        .arg("-q")
        .arg("--")
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

/// Normalize the workspace path the frontend hands `watch_start` into the Change
/// Feed's watch root: collapse separators and `.`/`..` to one spelling — but
/// without the Windows `\\?\` verbatim prefix `std::fs::canonicalize` would add,
/// which would leak into every emitted change path — and confirm it is an
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

/// Start (or re-root) the Change Feed's watcher on `root` — the open workspace's
/// path, passed explicitly by the frontend so the feed follows the project on
/// screen rather than the process's current directory (the two normally match,
/// but the cwd can drift from the displayed workspace). Idempotent per workspace:
/// a repeat call for the same root keeps the live watcher, while a call after a
/// project switch drops the old project's watcher and re-arms on the new root —
/// the feed always follows the open workspace.
#[tauri::command]
pub fn watch_start(
    app: AppHandle,
    window: WebviewWindow,
    state: State<WatcherState>,
    root: String,
) -> Result<(), String> {
    let root = resolve_watch_root(&root)?;
    let label = window.label().to_string();

    // Retire the bookkeeping of any window that has since closed before taking on
    // this one's — no per-window teardown hook fires otherwise (see
    // `prune_closed_windows`).
    prune_closed_windows(&app, &state);
    let Some(watch) = window_watch(&state, &label) else {
        return Err("watcher state unavailable".to_string());
    };

    let mut guard = watch.watcher.lock().map_err(|e| e.to_string())?;
    let already_watching_root = guard.as_ref().is_some_and(|active| active.root == root);
    if already_watching_root {
        return Ok(());
    }

    // Drop the previous project's watcher and its per-file bookkeeping before
    // re-arming, so its handles and stale line counts go with it.
    *guard = None;
    if let Ok(mut counts) = watch.line_counts.lock() {
        counts.clear();
    }
    if let Ok(mut seen) = watch.last_seen.lock() {
        seen.clear();
    }
    if let Ok(mut baselines) = watch.baselines.lock() {
        baselines.clear();
    }
    if let Ok(mut cache) = watch.git_ignore_cache.lock() {
        cache.clear();
    }

    // Decide how this root's ignored files are recognized before arming the
    // watcher, so the very first change is already filtered. Computed while `root`
    // is still owned here, before it moves into the `ProjectWatcher` below.
    let policy = compute_ignore_policy(&root);
    if let Ok(mut guard) = watch.ignore_policy.lock() {
        *guard = Some(policy);
    }

    // Seed the MCP server baseline from what's on disk now, so opening a project
    // that already declares servers doesn't read as "all added" and restart.
    snapshot_mcp_baseline(&watch, &root);

    // The live-git-state companion watch (`HEAD`/`config` → `git://state`)
    // re-arms with the project: same lifetime and re-root cadence, but its own
    // tiny non-recursive watcher.
    arm_git_state(&app, &label, &watch, &root);

    // The notify callback runs on its own thread; it carries this window's label
    // so every change it detects is routed back to this window alone.
    let app_handle = app.clone();
    let callback_label = label.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };
        handle_event(&app_handle, &callback_label, event);
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

// ── Live git state ──────────────────────────────────────────────────────────
// The Change Feed's branch subtitle and the remote-gated actions read git state
// (`vcs_branch_of`, `vcs_remote_url`) that only changes when two files do:
// `.git/HEAD` (branch switches) and `.git/config` (remotes). This watcher tells
// the frontend the moment either changes — or the repo itself appears — so that
// state stays live instead of refreshing only on mount / project switch.

/// The event announcing a change to the workspace's live git state. Payload-free
/// on purpose: listeners re-fetch whatever git state they display, so the event
/// can never carry stale or partial data.
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
    /// `HEAD` or `config` was touched — the displayed state changed.
    StateTouched,
    /// A `.git` entry appeared under a root that had none — `git init` ran, so
    /// the watcher must re-arm onto the new repo's state files.
    GitDirAppeared,
    /// A `.gitignore` in an ANCESTOR of the watch root was touched (the
    /// recursive feed watch only sees the root's own subtree, but a workspace
    /// deeper than the repo toplevel inherits every ancestor's rules) — the
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
/// non-recursive on the git dir itself, so a matching name is a top-level entry
/// — never something under `objects/` or `refs/`.
fn is_git_state_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| GIT_STATE_FILE_NAMES.contains(&name))
}

/// Whether `path` names a `.git` entry (the thing `git init` creates).
fn is_git_dir_entry(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(GIT_DIR_NAME)
}

/// The directories holding the workspace's `HEAD` and `config`, each to be
/// watched non-recursively. A plain repo keeps both directly in `<root>/.git` —
/// one dir. A linked worktree or submodule keeps a `.git` *file* pointing
/// elsewhere: `HEAD` sits in the resolved git dir and `config` in the common
/// dir, so both are watched (deduplicated when they coincide). Empty when the
/// workspace has no git yet.
fn git_state_dirs(root: &Path) -> Vec<PathBuf> {
    let git_entry = root.join(GIT_DIR_NAME);
    if git_entry.is_dir() {
        return vec![git_entry];
    }
    if !git_entry.is_file() {
        return Vec::new();
    }
    resolved_git_dirs(root)
}

/// Ask git where a `.git`-file workspace's real git dir and common dir live
/// (`--path-format=absolute` keeps both absolute regardless of git's cwd). Any
/// failure resolves to no dirs — the state watcher then stays quiet rather than
/// watching a guessed location.
fn resolved_git_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(output) = crate::util::command("git")
        .arg("-C")
        .arg(root)
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
    unique_dirs(String::from_utf8_lossy(&output.stdout).lines())
}

/// Distinct, order-preserving directories from git's line-per-dir output.
fn unique_dirs<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let dir = PathBuf::from(trimmed);
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// Arm (or re-arm) the live-git-state watch for `root`, replacing any previous
/// one. With a repo present it watches the dir(s) holding `HEAD` and `config`;
/// with none it watches the workspace root itself, non-recursively, purely to
/// see a `.git` entry appear — that costs one handle on a handful of direct
/// children (never the workspace tree, which the recursive feed watcher already
/// covers), and is what lets a `git init` run *after* the project opened gain
/// the real watch without any polling. Best-effort: a failure to arm just means
/// git state won't live-update until the next re-root.
fn arm_git_state(app: &AppHandle, label: &str, watch: &WindowWatch, root: &Path) {
    let Ok(mut guard) = watch.git_state.lock() else {
        return;
    };
    rearm_git_state_locked(app, label, root, &mut guard);
}

/// The arming body, run under the `git_state` lock (shared by `arm_git_state`
/// and the coalescer's post-`git init` re-arm, which already holds it). `label`
/// is the owning window, so the coalescer routes `git://state` back to it alone.
fn rearm_git_state_locked(
    app: &AppHandle,
    label: &str,
    root: &Path,
    slot: &mut Option<GitStateWatcher>,
) {
    // Drop the previous watch first, so a failed re-arm can't leave a stale
    // watcher reporting another root's repo.
    *slot = None;

    let dirs = git_state_dirs(root);
    let (sender, receiver) = channel::<GitStateMessage>();
    let armed = if dirs.is_empty() {
        watch_for_git_init(root, sender)
    } else {
        watch_git_state(&dirs, &gitignore_ancestor_dirs(root), sender)
    };
    let Ok(watcher) = armed else {
        return;
    };
    spawn_git_state_coalescer(app.clone(), label.to_string(), root.to_path_buf(), receiver);
    *slot = Some(GitStateWatcher {
        root: root.to_path_buf(),
        _watcher: watcher,
    });
}

/// The no-repo-yet sentinel: report a `.git` entry appearing among the root's
/// direct children.
fn watch_for_git_init(
    root: &Path,
    sender: Sender<GitStateMessage>,
) -> notify::Result<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        let git_dir_appeared = matches!(event.kind, EventKind::Create(_))
            && event.paths.iter().any(|path| is_git_dir_entry(path));
        if git_dir_appeared {
            let _ = sender.send(GitStateMessage::GitDirAppeared);
        }
    })?;
    watcher.watch(root, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

/// The repo-present watch: report touches of `HEAD`/`config` in the state dirs,
/// and of `.gitignore` in the root's ancestor dirs (whose rules the workspace
/// inherits but whose events the recursive feed watch can't see). Never the
/// whole `.git` tree — `objects/` alone turns a busy repo into an event
/// firehose; a non-recursive watch on each dir sees exactly its top-level
/// entries, and the name filters drop the rest (`index`, lockfiles,
/// `FETCH_HEAD`, sibling files, …) before they cost anything.
fn watch_git_state(
    dirs: &[PathBuf],
    ancestor_dirs: &[PathBuf],
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
    })?;
    for dir in dirs {
        watcher.watch(dir, RecursiveMode::NonRecursive)?;
    }
    for dir in ancestor_dirs {
        // Best-effort: an unwatchable ancestor only costs its rules' liveness.
        let _ = watcher.watch(dir, RecursiveMode::NonRecursive);
    }
    Ok(watcher)
}

/// The directories between the repo toplevel and the watch root (both ends'
/// dirs included, the root itself excluded — its own subtree is already
/// watched recursively) whose `.gitignore` files apply to the workspace. Empty
/// when the root IS the toplevel, or when there is no repo.
fn gitignore_ancestor_dirs(root: &Path) -> Vec<PathBuf> {
    let Some(toplevel) = repo_toplevel(root) else {
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
fn repo_toplevel(root: &Path) -> Option<PathBuf> {
    let output = crate::util::command("git")
        .arg("-C")
        .arg(root)
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
/// single `git://state` emission, and — when the burst says `git init` just ran
/// — swap the root sentinel for the real `HEAD`/`config` watch first. The
/// re-arm happens on this thread, never inside the notify callback (a watcher
/// must not be dropped from its own event callback). The thread retires itself:
/// re-arming (or a re-root) drops the watcher holding the channel's sender, so
/// `recv` errors and the loop ends. Because messages buffered before that drop
/// still arrive, each burst first confirms this thread's root is still the
/// armed one — a superseded thread must neither re-arm nor emit for a
/// workspace the window has moved on from.
fn spawn_git_state_coalescer(
    app: AppHandle,
    label: String,
    root: PathBuf,
    receiver: Receiver<GitStateMessage>,
) {
    thread::spawn(move || {
        while let Ok(first) = receiver.recv() {
            let burst = drain_burst(&receiver, first, GIT_STATE_COALESCE);
            // The window this watch speaks for may have closed and been pruned;
            // then its `WindowWatch` is gone from the map and this thread retires.
            let state: State<WatcherState> = app.state();
            let Ok(windows) = state.windows.lock() else {
                return;
            };
            let Some(watch) = windows.get(&label).cloned() else {
                return;
            };
            drop(windows);
            {
                let Ok(mut guard) = watch.git_state.lock() else {
                    return;
                };
                let speaks_for_current_root =
                    guard.as_ref().is_some_and(|active| active.root == root);
                if !speaks_for_current_root {
                    return;
                }
                if burst.rearm {
                    rearm_git_state_locked(&app, &label, &root, &mut guard);
                }
            }
            if burst.rearm || burst.ignore_touched {
                // A `git init` flips the feed's ignore policy from the static
                // fallback to git's own rules; a touched ancestor `.gitignore`
                // changes the rules in place. Both re-filter the events already
                // shown. Outside the `git_state` lock — the refresh takes the
                // watcher/policy locks instead.
                refresh_ignore_policy(&app, &watch, &label);
            }
            if burst.rearm || burst.state_touched {
                let _ = app.emit_to(&label, GIT_STATE_EVENT, ());
            }
        }
    });
}

/// Keep receiving until the channel stays quiet for `window`, folding every
/// message into the burst's distinct effects (git init creates `.git`, `HEAD`,
/// and `config` in one quick flurry — one coalesced re-arm; a `.gitignore`
/// save alongside a checkout still refreshes the rules AND re-emits state).
fn drain_burst(
    receiver: &Receiver<GitStateMessage>,
    first: GitStateMessage,
    window: Duration,
) -> GitStateBurst {
    let mut burst = GitStateBurst::default();
    let mut fold = |message: GitStateMessage| match message {
        GitStateMessage::StateTouched => burst.state_touched = true,
        GitStateMessage::GitDirAppeared => burst.rearm = true,
        GitStateMessage::IgnoreRulesTouched => burst.ignore_touched = true,
    };
    fold(first);
    while let Ok(message) = receiver.recv_timeout(window) {
        fold(message);
    }
    burst
}

/// Watch a set of folders (non-recursively) and report when anything inside one
/// appears or disappears — the project picker's eyes. It hands over the *parents*
/// of the rows it shows, never the rows themselves: a watch holds a handle on the
/// folder it watches, and a handle on a project would be the very thing stopping
/// that project from being deleted. Re-arming replaces the previous set.
#[tauri::command]
pub fn watch_dirs(
    app: AppHandle,
    window: WebviewWindow,
    state: State<WatcherState>,
    dirs: Vec<String>,
) -> Result<(), String> {
    let label = window.label().to_string();
    prune_closed_windows(&app, &state);
    let mut guard = state.dirs.lock().map_err(|e| e.to_string())?;
    // Drop this window's old watcher before opening the new one, so its handles
    // go with it; another window's picker watch is untouched.
    guard.remove(&label);
    if dirs.is_empty() {
        return Ok(());
    }

    let app_handle = app.clone();
    let callback_label = label.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };
        // Only appearance/disappearance moves rows on the page; edits inside a
        // project are the Change Feed's business, not the picker's. Routed to the
        // owning window alone so a sibling's picker doesn't rescan on our news.
        if matches!(event.kind, EventKind::Create(_) | EventKind::Remove(_)) {
            let _ = app_handle.emit_to(&callback_label, "dirs://changed", ());
        }
    })
    .map_err(|e| e.to_string())?;

    for dir in dirs {
        // A folder that is already gone isn't an error — that's precisely the news
        // the caller is listening for, and the next re-arm won't ask for it again.
        let _ = watcher.watch(Path::new(&dir), RecursiveMode::NonRecursive);
    }

    guard.insert(label, watcher);
    Ok(())
}

fn handle_event(app: &AppHandle, label: &str, event: Event) {
    let Some(kind) = ChangeKind::from_event(event.kind) else {
        return;
    };

    let state: State<WatcherState> = app.state();
    // This callback belongs to one window's watch; resolve its bookkeeping and
    // route every surfaced change back to that window alone (`emit_to`), never
    // broadcast to sibling windows watching other projects.
    let Some(watch) = window_watch(&state, label) else {
        return;
    };
    // The active watch root, for the MCP-config and ancestor-relative checks
    // below — the recursive watcher only ever reports paths under it.
    let watch_root = watch
        .watcher
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|active| active.root.clone()));

    for path in event.paths {
        if ignored(&path) || path.is_dir() {
            continue;
        }
        // A created/modified file that has already vanished was an atomic-write
        // scratch file — skip it rather than surface a preview-less phantom card.
        if !surfaces(kind, path.exists()) {
            continue;
        }

        // A changed MCP config (`.mcp.json`) may have gained or lost a server;
        // if so, the affected agent's sessions restart to pick it up. Checked
        // before the ignore filter, since `.mcp.json` is often git-ignored.
        if let Some(root) = &watch_root {
            detect_mcp_change(app, &watch, label, root, &path);
        }

        // A changed .gitignore — edited, appearing for the first time, or
        // deleted — changes what counts as ignored, so rebuild the whole policy
        // (fresh rules in static mode, dropped memoized decisions in git mode)
        // and tell the feed to re-filter what it already shows. The .gitignore
        // itself is not ignored, so it still surfaces as its own change.
        if is_gitignore_file(&path) {
            refresh_ignore_policy(app, &watch, label);
        }

        // Skip whatever the project itself would ignore: git's own ignore rules in
        // a repo, or the tech-inferred ignore directories when there is no git. The
        // baseline `ignored` pre-filter above already dropped the giant dirs, so a
        // repo never shells git for node_modules/target and each surviving path is
        // git-checked at most once (memoized).
        if excluded_by_ignore_policy(&watch, &path) {
            continue;
        }

        // Debounce: editors emit bursts per save.
        {
            let Ok(mut seen) = watch.last_seen.lock() else {
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
        if let Ok(mut baselines) = watch.baselines.lock() {
            baselines.entry(path.clone()).or_insert_with(|| match kind {
                ChangeKind::Created => Some(String::new()),
                ChangeKind::Modified | ChangeKind::Deleted => read_preview_text(&path),
            });
        }

        let new_count = line_count(&path);
        let (added, removed) = {
            let Ok(mut counts) = watch.line_counts.lock() else {
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
        let _ = app.emit_to(label, "feed://change", ev);
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
pub fn feed_diff(
    window: WebviewWindow,
    state: State<WatcherState>,
    path: String,
) -> Result<Option<FeedDiff>, String> {
    let Some(watch) = window_watch(&state, window.label()) else {
        return Ok(None);
    };
    let before = {
        let baselines = watch.baselines.lock().map_err(|e| e.to_string())?;
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

/// The image extensions the Change Feed previews inline, each paired with the
/// MIME type its `data:` URL carries. The one authoritative home for the
/// extension→MIME mapping (its TS mirror is `ImageExtension` in `@/lib/image`);
/// an extension absent here is not treated as a previewable image. SVG is
/// included and served as a data URL like the raster formats — the frontend
/// renders every image through `<img src>`, so untrusted SVG markup is never
/// inlined into the DOM (no scripts run, no external refs load).
const IMAGE_MIME_TYPES: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("avif", "image/avif"),
    ("bmp", "image/bmp"),
    ("ico", "image/x-icon"),
    ("svg", "image/svg+xml"),
];

/// The MIME type for `path`'s extension when it names an image the feed can
/// preview, else `None`. Case-insensitive on the extension.
fn image_mime_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension().and_then(|name| name.to_str())?;
    let lowercased = extension.to_ascii_lowercase();
    IMAGE_MIME_TYPES
        .iter()
        .find(|(name, _)| *name == lowercased)
        .map(|(_, mime)| *mime)
}

/// Largest image the Change Feed will inline as a data URL. A few megabytes
/// covers real project assets; past it the preview is skipped rather than
/// bloating the webview with megabytes of base64 for one card.
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

/// Whether an image of `byte_length` is small enough to inline, capped at
/// [`MAX_IMAGE_BYTES`].
fn image_within_cap(byte_length: u64) -> bool {
    byte_length <= MAX_IMAGE_BYTES
}

/// Standard-alphabet (RFC 4648) base64 for the image data URL — a few lines of
/// std rather than a new dependency (the only base64 the backend needs). Encodes
/// each three input bytes into four characters, padding a final partial group
/// with `=`.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const PAD: char = '=';
    let sextet = |value: u8| char::from(ALPHABET[usize::from(value & 0x3f)]);

    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(sextet(first >> 2));
        encoded.push(sextet((first << 4) | (second >> 4)));
        encoded.push(if chunk.len() > 1 {
            sextet((second << 2) | (third >> 6))
        } else {
            PAD
        });
        encoded.push(if chunk.len() > 2 { sextet(third) } else { PAD });
    }
    encoded
}

/// A Change Feed card's inline image preview: the changed image file's bytes as a
/// ready-to-use `data:` URL, so the frontend renders it with a plain `<img src>`
/// and needs no asset protocol or extra capability (mirrors `feed_diff`'s flow).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedImage {
    /// A `data:<mime>;base64,<bytes>` URL — directly usable as an `<img>` source.
    data_url: String,
}

/// The inline image preview for `path`: its bytes as a base64 `data:` URL tagged
/// with the extension's MIME type. `None` when `path` is not a previewable image,
/// was not surfaced by this window's watch this session, is missing / not a
/// regular file, or is larger than [`MAX_IMAGE_BYTES`] — the card then falls back
/// to its text summary. Reads the CURRENT bytes (what the image is now); a
/// before/after for a modified image is a deliberate non-goal here. Gated on a
/// captured baseline like `feed_diff`, so the command can't base64 an arbitrary
/// file off disk.
#[tauri::command]
pub fn feed_image(
    window: WebviewWindow,
    state: State<WatcherState>,
    path: String,
) -> Result<Option<FeedImage>, String> {
    let Some(watch) = window_watch(&state, window.label()) else {
        return Ok(None);
    };
    {
        let baselines = watch.baselines.lock().map_err(|e| e.to_string())?;
        if !baselines.contains_key(Path::new(&path)) {
            return Ok(None);
        }
    }

    let file = Path::new(&path);
    let Some(mime) = image_mime_type(file) else {
        return Ok(None);
    };
    let Ok(metadata) = std::fs::metadata(file) else {
        return Ok(None);
    };
    if !metadata.is_file() || !image_within_cap(metadata.len()) {
        return Ok(None);
    }
    let Ok(bytes) = std::fs::read(file) else {
        return Ok(None);
    };

    Ok(Some(FeedImage {
        data_url: format!("data:{mime};base64,{}", base64_encode(&bytes)),
    }))
}

/// The subset of `paths` the current ignore policy excludes — how the frontend
/// re-filters the Change Feed it already rendered after `feed://ignore-changed`
/// (events for now-ignored paths are dropped; never-recorded ones can't be
/// resurrected, they simply surface again on their next real change).
#[tauri::command]
pub fn feed_ignored(
    window: WebviewWindow,
    state: State<WatcherState>,
    paths: Vec<String>,
) -> Vec<String> {
    let watch = window_watch(&state, window.label());
    paths
        .into_iter()
        .filter(|candidate| {
            let path = Path::new(candidate);
            ignored(path)
                || watch
                    .as_ref()
                    .is_some_and(|watch| excluded_by_ignore_policy(watch, path))
        })
        .collect()
}

pub fn init(app: &AppHandle) {
    app.manage(WatcherState::default());
}

#[cfg(test)]
mod tests {
    use super::{
        base64_encode, drain_burst, git_state_dirs, ignored_by_static_dirs, image_mime_type,
        image_within_cap, is_git_dir_entry, is_git_state_file, manifest_ignore_dirs,
        read_preview_text, resolve_watch_root, static_ignore_dirs, surfaces, unique_dirs,
        ChangeKind, GitStateMessage, MAX_IMAGE_BYTES, MAX_PREVIEW_BYTES,
    };
    use std::collections::HashSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::channel;
    use std::time::Duration;

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

    #[test]
    fn resolve_watch_root_accepts_an_existing_directory() {
        let dir = scratch("root");
        let path = dir.to_string_lossy();
        assert!(resolve_watch_root(&path).is_ok_and(|root| root.is_dir()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_watch_root_rejects_a_missing_path() {
        let dir = scratch("root-missing");
        let path = dir.join("nope").to_string_lossy().into_owned();
        assert!(resolve_watch_root(&path).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_watch_root_rejects_a_file() {
        let dir = scratch("root-file");
        let file = dir.join("a.txt");
        fs::write(&file, b"x").expect("write file");
        let path = file.to_string_lossy().into_owned();
        assert!(resolve_watch_root(&path).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_node_manifest_maps_to_node_modules_and_friends() {
        let dirs = manifest_ignore_dirs("package.json");
        assert!(dirs.contains(&"node_modules"));
        assert!(dirs.contains(&"dist"));
        // A lockfile alone implies the same node ignore set.
        assert!(manifest_ignore_dirs("pnpm-lock.yaml").contains(&"node_modules"));
    }

    #[test]
    fn a_cargo_manifest_maps_to_target() {
        assert!(manifest_ignore_dirs("Cargo.toml").contains(&"target"));
    }

    #[test]
    fn a_dotnet_project_file_maps_to_bin_and_obj_by_extension() {
        let dirs = manifest_ignore_dirs("MyApp.csproj");
        assert!(dirs.contains(&"bin"));
        assert!(dirs.contains(&"obj"));
    }

    #[test]
    fn an_unrecognized_file_implies_no_ignore_dirs() {
        assert!(manifest_ignore_dirs("README.md").is_empty());
    }

    #[test]
    fn static_ignore_dirs_unions_manifest_dirs_with_the_baseline() {
        let dir = scratch("static-node");
        fs::write(dir.join("package.json"), b"{}").expect("write manifest");
        let dirs = static_ignore_dirs(&dir);
        assert!(dirs.contains("node_modules"));
        // The baseline is always present, so static mode never weakens the
        // always-on pre-filter.
        assert!(dirs.contains(".git"));
        assert!(dirs.contains("target"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn static_ignore_dirs_for_a_bare_folder_is_just_the_baseline() {
        let dir = scratch("static-bare");
        let dirs = static_ignore_dirs(&dir);
        assert!(dirs.contains(".git"));
        assert!(dirs.contains("node_modules"));
        // No manifest present, so nothing beyond the baseline is added.
        assert!(!dirs.contains(".gradle"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn head_and_config_are_the_git_state_files() {
        assert!(is_git_state_file(Path::new(r"C:\repo\.git\HEAD")));
        assert!(is_git_state_file(Path::new(r"C:\repo\.git\config")));
    }

    #[test]
    fn other_git_dir_entries_are_not_git_state_files() {
        assert!(!is_git_state_file(Path::new(r"C:\repo\.git\index")));
        assert!(!is_git_state_file(Path::new(r"C:\repo\.git\ORIG_HEAD")));
        assert!(!is_git_state_file(Path::new(r"C:\repo\.git\config.lock")));
        assert!(!is_git_state_file(Path::new(r"C:\repo\.git\packed-refs")));
    }

    #[test]
    fn only_a_dot_git_entry_announces_the_repo_appearing() {
        assert!(is_git_dir_entry(Path::new(r"C:\repo\.git")));
        assert!(!is_git_dir_entry(Path::new(r"C:\repo\.gitignore")));
        assert!(!is_git_dir_entry(Path::new(r"C:\repo\src")));
    }

    #[test]
    fn unique_dirs_trims_dedupes_and_skips_empty_lines() {
        let dirs =
            unique_dirs([" C:/repo/.git ", "", "C:/repo/.git", "C:/common/.git"].into_iter());
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("C:/repo/.git"),
                PathBuf::from("C:/common/.git")
            ]
        );
    }

    #[test]
    fn a_workspace_with_a_git_dir_watches_exactly_that_dir() {
        let dir = scratch("git-state-repo");
        let git_dir = dir.join(".git");
        fs::create_dir_all(&git_dir).expect("create .git dir");
        assert_eq!(git_state_dirs(&dir), vec![git_dir]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_workspace_without_git_yields_no_state_dirs() {
        let dir = scratch("git-state-bare");
        assert!(git_state_dirs(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
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

    /// A busy burst asks for each distinct effect it saw — a `git init` re-arm
    /// doesn't swallow a state emit, nor a `.gitignore` refresh.
    #[test]
    fn a_burst_coalesces_every_distinct_effect() {
        let (sender, receiver) = channel::<GitStateMessage>();
        sender
            .send(GitStateMessage::GitDirAppeared)
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
    fn image_mime_type_maps_known_extensions_case_insensitively() {
        assert_eq!(
            image_mime_type(Path::new("a/b/logo.png")),
            Some("image/png")
        );
        assert_eq!(image_mime_type(Path::new("photo.JPG")), Some("image/jpeg"));
        assert_eq!(image_mime_type(Path::new("photo.jpeg")), Some("image/jpeg"));
        assert_eq!(
            image_mime_type(Path::new("icon.svg")),
            Some("image/svg+xml")
        );
        assert_eq!(image_mime_type(Path::new("art.avif")), Some("image/avif"));
        assert_eq!(
            image_mime_type(Path::new("favicon.ico")),
            Some("image/x-icon")
        );
    }

    #[test]
    fn image_mime_type_is_none_for_non_images() {
        assert_eq!(image_mime_type(Path::new("main.rs")), None);
        assert_eq!(image_mime_type(Path::new("README")), None);
        // Only the final extension decides; a `.png` in the stem doesn't count.
        assert_eq!(image_mime_type(Path::new("archive.png.zip")), None);
    }

    #[test]
    fn image_within_cap_rejects_only_over_the_limit() {
        assert!(image_within_cap(0));
        assert!(image_within_cap(MAX_IMAGE_BYTES));
        assert!(!image_within_cap(MAX_IMAGE_BYTES + 1));
    }

    /// The RFC 4648 test vectors — proof the hand-rolled encoder pads partial
    /// groups correctly (the case a data URL depends on).
    #[test]
    fn base64_encode_matches_the_rfc_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_path_under_an_ignored_directory_is_excluded_in_static_mode() {
        let mut dirs = HashSet::new();
        dirs.insert("node_modules".to_string());
        let path = Path::new("project/node_modules/react/index.js");
        assert!(ignored_by_static_dirs(path, &dirs));
    }

    #[test]
    fn a_normal_source_path_is_not_excluded_in_static_mode() {
        let mut dirs = HashSet::new();
        dirs.insert("node_modules".to_string());
        let path = Path::new("project/src/app.ts");
        assert!(!ignored_by_static_dirs(path, &dirs));
    }
}
