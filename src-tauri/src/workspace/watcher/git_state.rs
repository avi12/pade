//! Live git state watching: HEAD and config changes, coalesced into one event.

use super::ignore::is_gitignore_file;
use super::ignore::refresh_ignore_policy;
use super::ignore::GIT_DIRECTORY_NAME;
use super::registry::ProjectRootPayload;
use super::registry::ProjectWatch;
use crate::vcs::git_in;
use notify::Event;
use notify::EventKind;
use notify::RecommendedWatcher;
use notify::RecursiveMode;
use notify::Watcher;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Weak;
use std::thread;
use std::time::Duration;
use tauri::AppHandle;
use tauri::Emitter;

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
pub(super) const GIT_STATE_EVENT: &str = "git://state";

/// How long a burst of git-state touches is coalesced before one event is
/// emitted. A checkout rewrites `HEAD` several times (lockfile, rename, reflog
/// bookkeeping); waiting for the burst to go quiet emits once, after git has
/// settled, so listeners re-fetch the final state.
pub(super) const GIT_STATE_COALESCE: Duration = Duration::from_millis(250);

/// The files inside a git dir whose content IS the live state PADE displays:
/// `HEAD` names the checked-out branch, `config` holds the remotes.
pub(super) const GIT_STATE_FILE_NAMES: &[&str] = &["HEAD", "config"];

/// What the git-state watcher's callback tells its coalescer thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GitStateMessage {
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
pub(super) struct GitStateBurst {
    pub(super) rearm: bool,
    pub(super) state_touched: bool,
    pub(super) ignore_touched: bool,
}

/// Whether `path` names one of the two git-state files. The watch is
/// non-recursive on the git dir itself, so a matching name is a top-level
/// entry, never something under `objects/` or `refs/`.
pub(super) fn is_git_state_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| GIT_STATE_FILE_NAMES.contains(&name))
}

/// Whether `path` names a `.git` entry (the thing `git init` creates).
pub(super) fn is_git_directory_entry(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(GIT_DIRECTORY_NAME)
}

pub(super) fn git_directory_lifecycle_changed(kind: EventKind, paths: &[PathBuf]) -> bool {
    matches!(kind, EventKind::Create(_) | EventKind::Remove(_))
        && paths.iter().any(|path| is_git_directory_entry(path))
}

/// The directories holding the workspace's `HEAD` and `config`, each to be
/// watched non-recursively. A plain repo keeps both directly in `<root>/.git` -
/// one dir. A linked worktree or submodule keeps a `.git` *file* pointing
/// elsewhere: `HEAD` sits in the resolved git dir and `config` in the common
/// dir, so both are watched (deduplicated when they coincide). Empty when the
/// workspace has no git yet.
pub(super) fn git_state_directories(root: &Path) -> Vec<PathBuf> {
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
pub(super) fn resolved_git_directories(root: &Path) -> Vec<PathBuf> {
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
pub(super) fn unique_directories<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<PathBuf> {
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
pub(super) fn arm_git_state(app: &AppHandle, watch: &Arc<ProjectWatch>, plan: &GitStatePlan) {
    let Ok(mut guard) = watch.git_state.lock() else {
        return;
    };
    rearm_git_state_locked(app, watch, plan, &mut guard);
}

/// What arming a git-state watch needs to ask git, gathered up front so the
/// arming itself never shells out - `watch_projects` asks before it takes any
/// lock a synchronous command reads.
pub(super) struct GitStatePlan {
    /// The dirs holding `HEAD` and `config`; empty when there is no repo yet.
    directories: Vec<PathBuf>,
    /// The ancestors whose `.gitignore` the workspace inherits (repo only).
    ancestor_directories: Vec<PathBuf>,
}

impl GitStatePlan {
    pub(super) fn for_root(root: &Path) -> Self {
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
pub(super) fn rearm_git_state_locked(
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
pub(super) fn watch_for_git_init(
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
pub(super) fn watch_git_state(
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
pub(super) fn gitignore_ancestor_directories(root: &Path) -> Vec<PathBuf> {
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
pub(super) fn repository_toplevel(root: &Path) -> Option<PathBuf> {
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
pub(super) fn spawn_git_state_coalescer(
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
pub(super) fn drain_burst(
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
