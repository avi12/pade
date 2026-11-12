//! Language-census watches: tells the frontend when a project's languages may have changed.

use super::change::ChangeKind;
use super::ignore::compute_ignore_policy;
use super::ignore::is_always_ignored;
use super::ignore::is_gitignore_file;
use super::ignore::IgnoreRules;
use super::registry::prune_closed_windows;
use super::registry::window_watches;
use super::registry::LanguageWatch;
use super::registry::WatcherState;
use notify::Event;
use notify::RecursiveMode;
use notify::Watcher;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::State;
use tauri::WebviewWindow;

/// Announces that a watched project's files changed, so its languages may
/// have. The payload is the project path, as the window spelled it.
pub(super) const LANGUAGES_CHANGED_EVENT: &str = "languages://changed";

/// What one folder of a window's language watch reports for.
#[derive(Clone)]
pub(super) enum LanguageRoute {
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
pub(super) enum LanguageNews {
    /// The tracked set moved (a stage, commit or checkout): the index stamp
    /// already invalidates the count.
    TrackedSet,
    /// A file the census may count was written, created or removed: the index
    /// never sees it, so the project's working-tree generation must move.
    WorkingTree,
}

/// The `.git` files whose change means the tracked set (or what is checked
/// out) moved: the index on a stage or commit, `HEAD` on a checkout.
pub(super) const GIT_TRACKED_SET_FILES: &[&str] = &["index", "HEAD"];

pub(super) fn is_tracked_set_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| GIT_TRACKED_SET_FILES.contains(&name))
}

/// The git folders holding a project's index files - its own `.git` and each
/// submodule's - spelled under `root`, as a watch on `root` reports them. A
/// linked worktree (whose `.git` is a file) has none.
pub(super) fn git_directories_of(root: &Path) -> Vec<PathBuf> {
    crate::vcs::tracked_set_indexes(root)
        .iter()
        .filter_map(|index| Some(index.parent()?.to_path_buf()))
        .collect()
}

/// Whether a change to `path`, seen under the folder `watched` that `route`
/// watches, is news for that project's languages. Build output, dependency
/// folders and git's own churn are not; neither is anything the project's
/// ignore rules skip - the census would never count it either.
pub(super) fn language_news(
    watched: &Path,
    route: &LanguageRoute,
    path: &Path,
) -> Option<LanguageNews> {
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
pub(super) fn planned_language_routes(
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
pub(super) fn report_language_news(
    app: &AppHandle,
    label: &str,
    project: &str,
    news: LanguageNews,
) {
    if news == LanguageNews::WorkingTree {
        crate::workspace::ide::note_working_tree_change(Path::new(project));
    }
    let _ = app.emit_to(label, LANGUAGES_CHANGED_EVENT, project);
}

/// The notify callback of a window's language watch: route every path to each
/// watched folder above it (a nested project's change is news for its parent
/// too) and report what counts. The routes are cloned out first, so an ignore
/// check that shells git never holds up a re-arm.
pub(super) fn handle_language_event(
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
pub(super) fn rearm_language_watch(
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
pub(super) fn rearm_language_watch_after_feed(app: &AppHandle, state: &WatcherState, label: &str) {
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
