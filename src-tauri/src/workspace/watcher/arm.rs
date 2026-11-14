//! Arming and releasing the Change Feed watch for the projects a window holds.

use super::events::handle_event;
use super::git_state::arm_git_state;
use super::git_state::GitStatePlan;
use super::ignore::compute_ignore_policy;
use super::ignore::IgnoreRules;
use super::languages::rearm_language_watch_after_feed;
use super::mcp::mcp_baseline;
use super::registry::prune_closed_windows;
use super::registry::watch_set_change;
use super::registry::window_watches;
use super::registry::ProjectWatch;
use super::registry::WatcherState;
use crate::util::now_milliseconds;
use notify::Event;
use notify::RecursiveMode;
use notify::Watcher;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri::State;
use tauri::WebviewWindow;

/// Normalize a project root the frontend hands `watch_projects` into a Change
/// Feed watch root: collapse separators and `.`/`..` to one spelling - but
/// without the Windows `\\?\` verbatim prefix `std::fs::canonicalize` would add,
/// which would leak into every emitted change path - and confirm it is an
/// existing directory, so a drifted or bad path fails loudly here rather than
/// silently watching the wrong tree. Mirrors workspace's `canonical_path`.
pub(super) fn resolve_watch_root(root: &str) -> Result<PathBuf, String> {
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
pub(super) fn build_project_watch(
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
pub(crate) async fn watch_projects(
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
