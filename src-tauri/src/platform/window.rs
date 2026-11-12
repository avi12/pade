//! Multi-window - spawn additional PADE app windows (Ctrl+Shift+N / app menu).
//!
//! Each new window loads the same frontend `index.html` with a query string that
//! tells the app what to boot into (`?w=empty` picker, `?w=temp` throwaway
//! workspace, or `?w=open&path=<enc>` a specific project). The frontend routing
//! that reads `location.search` lives in the shell task - this module only spawns
//! a window that loads the app with the right query. New windows clone the main
//! window's sizing/decorations so they feel identical.

pub(crate) mod order;
#[cfg(test)]
mod tests;
pub(crate) mod theme;

use self::order::order_key;
use self::theme::surface_for;
use crate::util::{comparable_path, same_path};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalSize, Theme, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, Window, Wry,
};

/// The window the app starts with (see `tauri.conf.json`).
pub(crate) const MAIN_WINDOW_LABEL: &str = "main";

/// Every app window's title.
const WINDOW_TITLE: &str = "PADE";

/// The smallest an app window may be resized to, in logical pixels.
const MINIMUM_WINDOW_WIDTH: f64 = 720.0;

const MINIMUM_WINDOW_HEIGHT: f64 = 480.0;

/// The label prefix of every spawned window (`w-1`, `w-2`, …).
const SPAWNED_LABEL_PREFIX: &str = "w-";

/// The agents dashboard's label. There is only ever one: asking for it again
/// brings the open one up. `capabilities/default.json` lists it by this name
/// beside `main` and `w-*`, so it gets the same permissions.
const DASHBOARD_LABEL: &str = "dashboard";

/// The `mode` a window spawned to receive a hand-off asks for.
const HANDOFF_MODE: &str = "handoff";

/// The frontend event carrying an OS theme flip's authoritative scheme.
pub const THEME_CHANGED_EVENT: &str = "theme://changed";

/// What a new app window copies from an existing one so it opens looking
/// identical: its theme (for the surface paint), size and decorations. Each part
/// is optional because each probe of a live window can fail on its own.
pub(crate) struct WindowLook {
    theme: Option<Theme>,
    size: Option<PhysicalSize<u32>>,
    is_decorated: Option<bool>,
}

impl WindowLook {
    pub(crate) fn of(window: &WebviewWindow) -> Self {
        Self {
            theme: window.theme().ok(),
            size: window.inner_size().ok(),
            is_decorated: window.is_decorated().ok(),
        }
    }
}

/// A builder for a PADE app window with `label` at `url`, titled and bounded
/// like every other, and dressed in `look` when there is one to copy. The one
/// home for app-window chrome, shared by a spawned window and a crash rebuild.
pub(crate) fn app_window_builder<'app>(
    app: &'app AppHandle,
    label: &str,
    url: WebviewUrl,
    look: Option<&WindowLook>,
) -> WebviewWindowBuilder<'app, Wry, AppHandle> {
    let mut builder = WebviewWindowBuilder::new(app, label, url)
        .title(WINDOW_TITLE)
        .min_inner_size(MINIMUM_WINDOW_WIDTH, MINIMUM_WINDOW_HEIGHT);
    let Some(look) = look else {
        return builder;
    };
    // Open in-theme, avoiding a white flash on a dark desktop.
    if let Some(theme) = look.theme {
        builder = builder.background_color(surface_for(theme));
    }
    if let Some(size) = look.size {
        builder = builder.inner_size(f64::from(size.width), f64::from(size.height));
    }
    if let Some(is_decorated) = look.is_decorated {
        builder = builder.decorations(is_decorated);
    }
    builder
}

/// Per-window state the switcher reads: which project each window has open (keyed
/// by window label), and the user's explicit drag-reorder of the "Open windows"
/// list. The order is session-scoped - window labels are ephemeral per app run, so
/// persisting it across restarts by label would be meaningless.
#[derive(Default)]
pub struct WindowProjects {
    /// Which project each window currently has open, keyed by window label. Lets the
    /// picker focus an already-open project's window instead of opening it twice.
    projects: Mutex<HashMap<String, String>>,
    /// The user's explicit label order for the switcher list and the `Ctrl+Alt+[`/`]`
    /// cycle - the single source both read. A live window absent from it falls back
    /// to creation order (`order_key`) after the explicitly ordered ones.
    order: Mutex<Vec<String>>,
}

impl WindowProjects {
    /// The project registered to one live application window. Native commands
    /// use this instead of process-global cwd when selecting project data.
    pub(crate) fn project_for(&self, label: &str) -> Result<String, String> {
        self.projects
            .lock()
            .map_err(|error| error.to_string())?
            .get(label)
            .filter(|path| !path.is_empty())
            .cloned()
            .ok_or_else(|| "this window has no registered project".to_string())
    }

    /// A copy of the explicit switcher order, so sorting never holds the lock.
    fn order_snapshot(&self) -> Vec<String> {
        self.order
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}

/// The projects open in a live window right now - an entry whose window has
/// since closed is skipped.
pub(crate) fn open_projects(app: &AppHandle) -> Vec<String> {
    let state = app.state::<WindowProjects>();
    let Ok(projects) = state.projects.lock() else {
        return Vec::new();
    };
    projects
        .iter()
        .filter(|(label, path)| !path.is_empty() && app.get_webview_window(label).is_some())
        .map(|(_, path)| path.clone())
        .collect()
}

/// Stamp a project a window just left, off the calling thread - the stamp is a
/// settings-file write and these callers run on the main thread.
fn mark_left_in_background(path: String) {
    if path.is_empty() {
        return;
    }
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = crate::workspace::mark_left(&path) {
            eprintln!("[pade] could not record leaving {path}: {error}");
        }
    });
}

/// A window closed: it no longer holds its project, and that project was in use
/// until this moment. Every session it still owned ends with it - the frontend's
/// graceful leave normally got there first, and this is the backstop for a
/// close that skipped it (a second X click mid-leave, a webview that died) so
/// no agent keeps running where no window can reach it. A window being rebuilt
/// after a webview crash keeps its sessions: its replacement re-attaches them.
pub fn on_destroyed(window: &Window) {
    let state = window.state::<WindowProjects>();
    let previous = state
        .projects
        .lock()
        .ok()
        .and_then(|mut projects| projects.remove(window.label()));
    if let Some(previous) = previous {
        mark_left_in_background(previous);
    }
    if !crate::platform::recovery::is_recreate_pending() {
        crate::terminal::pty::kill_owned(window.app_handle(), window.label());
        crate::platform::window_handoff::forget(window.app_handle(), window.label());
    }
    broadcast_windows_changed(window.app_handle());
}

/// Broadcast that the open-windows set or its order changed, so every window's
/// switcher (`AppMenu`) can refresh its "Open windows" list live - the one home
/// for the event name the frontend `windows.onChanged` listens on.
const WINDOWS_CHANGED_EVENT: &str = "windows://changed";

/// App-wide notify that the open-windows list changed (a window registered a
/// project, or the user drag-reordered it), so a switcher open in another window
/// reflects it at once instead of only on its next open.
fn broadcast_windows_changed(app: &AppHandle) {
    let _ = app.emit(WINDOWS_CHANGED_EVENT, ());
}

/// Restore and focus `window`, reporting whether it took focus.
fn bring_to_front(window: &WebviewWindow) -> bool {
    let _ = window.unminimize();
    window.set_focus().is_ok()
}

/// Record the project the calling window now has open.
#[tauri::command]
pub fn window_register_project(
    window: WebviewWindow,
    state: tauri::State<WindowProjects>,
    path: String,
) {
    let previous = state.projects.lock().ok().and_then(|mut projects| {
        // Stored verbatim so the switcher's "Open windows" list can show the real
        // path/name; comparison normalizes on read (see `window_focus_project`).
        projects.insert(window.label().to_string(), path.clone())
    });
    if let Some(previous) = previous.filter(|previous| !same_path(previous, &path)) {
        mark_left_in_background(previous);
    }
    broadcast_windows_changed(window.app_handle());
}

/// Focus another window already showing `path`. Returns true when one was found
/// and focused, so the caller can skip opening the project again. Prunes any
/// stale entry whose window has since closed.
#[tauri::command]
pub fn window_focus_project(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<WindowProjects>,
    path: String,
) -> bool {
    let target = comparable_path(&path);
    let current_label = window.label();
    let candidates: Vec<String> = {
        let Ok(projects) = state.projects.lock() else {
            return false;
        };
        projects
            .iter()
            .filter(|(label, project)| {
                label.as_str() != current_label && comparable_path(project) == target
            })
            .map(|(label, _)| label.clone())
            .collect()
    };

    for label in candidates {
        if let Some(target_window) = app.get_webview_window(&label) {
            bring_to_front(&target_window);
            return true;
        }
        // The window is gone - drop its stale entry.
        if let Ok(mut projects) = state.projects.lock() {
            projects.remove(&label);
        }
    }
    false
}

/// The next spawned window's sequence number (`w-1`, `w-2`, …). Labels must be
/// unique per app run; a simple atomic is enough and needs no dependency.
static NEXT_WINDOW_SEQUENCE: AtomicU32 = AtomicU32::new(1);

/// The launch intents a spawned window can carry. One authoritative home for the
/// mode literals and the `w=` query strings the frontend router reads.
enum LaunchMode<'path> {
    Empty,
    Temp,
    Dashboard,
    Open(&'path str),
    /// A project window that boots from the sessions handed to it
    /// (`window_handoff`) instead of entering the project fresh.
    Handoff(&'path str),
}

impl<'path> LaunchMode<'path> {
    /// Resolve the requested `mode` + optional `path` into a launch intent. A
    /// present `path` means "open that project" - taking a hand-off there when
    /// the mode asks for one.
    fn resolve(mode: &str, path: Option<&'path str>) -> Self {
        if let Some(path) = path {
            if mode == HANDOFF_MODE {
                return Self::Handoff(path);
            }
            return Self::Open(path);
        }
        match mode {
            "temp" => Self::Temp,
            "dashboard" => Self::Dashboard,
            _ => Self::Empty,
        }
    }

    /// Whether the window zooms the way a browser does: `WebView2`'s own zoom on
    /// Ctrl with the wheel, plus, minus or zero. Only the dashboard - it is read,
    /// not typed in, while a project window's terminal owns those keys and its
    /// font size is the app's zoom preference.
    fn has_browser_zoom(&self) -> bool {
        matches!(self, Self::Dashboard)
    }

    /// The label a window of this mode takes: the one fixed dashboard label, or
    /// the next spawned `w-{n}`.
    fn label(&self) -> String {
        if matches!(self, Self::Dashboard) {
            return DASHBOARD_LABEL.to_string();
        }
        let sequence = NEXT_WINDOW_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        format!("{SPAWNED_LABEL_PREFIX}{sequence}")
    }

    /// The query string the frontend router boots from.
    fn query(&self) -> String {
        match self {
            Self::Empty => "w=empty".to_string(),
            Self::Temp => "w=temp".to_string(),
            Self::Dashboard => "w=dashboard".to_string(),
            Self::Open(path) => {
                format!("w=open&path={}", crate::util::percent_encode(path, b""))
            }
            Self::Handoff(path) => {
                format!("w=handoff&path={}", crate::util::percent_encode(path, b""))
            }
        }
    }
}

/// Spawn a new PADE window loading the app with a `w=` query describing what to
/// boot into. `mode` is `"empty"` | `"temp"` | `"dashboard"`; a present `path`
/// opens that project. The dashboard is one window: asking again focuses it.
///
/// **`async` is load-bearing on Windows.** Tauri runs a synchronous command *on the
/// event loop thread*, and building a webview window blocks that thread waiting for
/// `WebView2` to hand the webview back - which it can only do from the event loop. The
/// two wait on each other: the OS window appears (title bar, themed surface) with no
/// webview in it, and the whole app stops pumping messages, so the blank window cannot
/// even be closed. Tauri's own docs say it outright: "on Windows, this function
/// deadlocks when used in a synchronous command". An async command runs off that
/// thread, leaving the event loop free to finish the job.
///
/// A `handoff` window also takes the calling window's `sessions` and the
/// `payload` describing them (`window_handoff`), parked before the window is
/// built so its boot always finds them - and handed back if the build fails.
#[tauri::command]
pub async fn window_create(
    app: AppHandle,
    window: WebviewWindow,
    mode: String,
    path: Option<String>,
    sessions: Option<Vec<String>>,
    payload: Option<String>,
) -> Result<(), String> {
    let launch = LaunchMode::resolve(&mode, path.as_deref());
    let label = launch.label();
    if let Some(open) = app.get_webview_window(&label) {
        bring_to_front(&open);
        return Ok(());
    }
    let handed_sessions = sessions.unwrap_or_default();
    let is_handoff = matches!(launch, LaunchMode::Handoff(_));
    if is_handoff {
        crate::platform::window_handoff::hand_off(
            &app,
            window.label(),
            &label,
            &handed_sessions,
            payload.unwrap_or_default(),
        )
        .await?;
    }
    let query = launch.query();
    let url = WebviewUrl::App(format!("index.html?{query}").into());
    let main_look = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .map(|main| WindowLook::of(&main));

    // Build visible: the builder's background color already paints the themed
    // surface at creation, so there's no white flash to hide. (Building hidden
    // and calling `show()` here left the window invisible - a freshly built
    // webview window doesn't reliably show from within the command.) Crash
    // recovery arms once the window's frontend boots and calls `recovery_arm`
    // with its live URL (see recovery.rs).
    let built = app_window_builder(&app, &label, url, main_look.as_ref())
        .zoom_hotkeys_enabled(launch.has_browser_zoom())
        .build();
    if let Err(error) = built {
        if is_handoff {
            crate::platform::window_handoff::undo_hand_off(
                &app,
                window.label(),
                &label,
                &handed_sessions,
            )
            .await;
        }
        return Err(error.to_string());
    }
    Ok(())
}

/// The page a recreated window loads: the app with no `w=` query, whose boot
/// re-attaches the sessions its label's snapshot names (`reattachAfterReload`).
const PLAIN_ROUTE: &str = "index.html";

/// Rebuild the windows whose sessions outlived the app. Agents run in the PTY
/// host, so a restarted app (a dev rebuild, a crash) finds them still running,
/// owned by window labels it no longer has; `main` comes back by itself, every
/// other window is built again under its old label, so its persisted snapshot
/// re-attaches it - no agent is left running where no window can reach it.
pub(crate) fn recreate_session_windows<'owner>(
    app: &AppHandle,
    owners: impl Iterator<Item = &'owner str>,
) {
    let live: Vec<String> = app.webview_windows().into_keys().collect();
    let missing = labels_to_recreate(owners, &live);
    if missing.is_empty() {
        return;
    }
    reserve_sequences_past(&missing);
    let main_look = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .map(|main| WindowLook::of(&main));
    for label in missing {
        let url = WebviewUrl::App(PLAIN_ROUTE.into());
        if let Err(error) = app_window_builder(app, &label, url, main_look.as_ref()).build() {
            eprintln!("[pade] could not recreate window {label} for its running sessions: {error}");
        }
    }
}

/// The owner labels that need a window built: every one not live already,
/// except `main` (the app always creates it), once each, in creation order.
fn labels_to_recreate<'owner>(
    owners: impl Iterator<Item = &'owner str>,
    live: &[String],
) -> Vec<String> {
    let mut labels: Vec<String> = owners
        .filter(|owner| !owner.is_empty() && *owner != MAIN_WINDOW_LABEL)
        .filter(|owner| !live.iter().any(|label| label == owner))
        .map(ToString::to_string)
        .collect();
    labels.sort_by_cached_key(|label| (order_key(label), label.clone()));
    labels.dedup();
    labels
}

/// Make sure the next spawned window's `w-{n}` is past every recreated one, so
/// a new window never collides with a label a running session still owns.
fn reserve_sequences_past(labels: &[String]) {
    let highest = labels
        .iter()
        .filter_map(|label| {
            label
                .strip_prefix(SPAWNED_LABEL_PREFIX)?
                .parse::<u32>()
                .ok()
        })
        .max();
    if let Some(highest) = highest {
        NEXT_WINDOW_SEQUENCE.fetch_max(highest.saturating_add(1), Ordering::Relaxed);
    }
}
