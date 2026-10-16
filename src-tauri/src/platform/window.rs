//! Multi-window - spawn additional PADE app windows (Ctrl+Shift+N / app menu).
//!
//! Each new window loads the same frontend `index.html` with a query string that
//! tells the app what to boot into (`?w=empty` picker, `?w=temp` throwaway
//! workspace, or `?w=open&path=<enc>` a specific project). The frontend routing
//! that reads `location.search` lives in the shell task - this module only spawns
//! a window that loads the app with the right query. New windows clone the main
//! window's sizing/decorations so they feel identical.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::window::Color;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalSize, Theme, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, Window, Wry,
};

use crate::terminal::theming::Scheme;
use crate::util::{comparable_path, same_path};

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
/// brings the open one up.
const DASHBOARD_LABEL: &str = "dashboard";

/// M3 surface colors, mirroring `--surface` in `src/theme.css` for the light and
/// dark schemes. Painted as the webview background at window creation so a window
/// opens already in-theme: `WebView2` otherwise shows an unthemed white surface
/// until the HTML/CSS first paints, flashing white on a dark desktop. This is the
/// native side of the token - Rust can't read the CSS custom property - so keep
/// the two in sync.
const SURFACE_LIGHT: Color = Color(248, 250, 251, 255); // hsl(210deg 30% 98%)
const SURFACE_DARK: Color = Color(14, 20, 27, 255); // hsl(214deg 30% 8%)

/// The frontend's `appearance.scheme` for a resolved OS theme. The one mapping
/// the startup read, the flip event and the surface paint all go through, so a
/// window can never start on one scheme and flip to another.
fn scheme_for(theme: Theme) -> Scheme {
    match theme {
        Theme::Dark => Scheme::Dark,
        // `Theme` is `#[non_exhaustive]`; treat light and anything new as light.
        _ => Scheme::Light,
    }
}

/// The surface color matching a resolved OS theme.
fn surface_for(theme: Theme) -> Color {
    match scheme_for(theme) {
        Scheme::Light => SURFACE_LIGHT,
        Scheme::Dark => SURFACE_DARK,
    }
}

/// Paint `window`'s webview with the themed surface so it shows in-theme before
/// the frontend renders. Best-effort - a failed theme probe leaves the default.
pub fn paint_surface(window: &WebviewWindow) {
    if let Ok(theme) = window.theme() {
        let _ = window.set_background_color(Some(surface_for(theme)));
    }
}

/// The frontend event carrying an OS theme flip's authoritative scheme.
pub const THEME_CHANGED_EVENT: &str = "theme://changed";

/// This window's OS theme right now, for the frontend to seed its scheme with at
/// startup.
///
/// `WebView2`'s `prefers-color-scheme` can still answer *light* while the window
/// is already dark - it resolves after the first script runs - and the flip event
/// above only fires on a genuine *change*, so nothing ever corrects that first
/// answer. A session spawned in that window then themes its agent for the wrong
/// scheme for the life of the process (Claude reads `$COLORFGBG` at spawn; see
/// `theming`). Reading the native theme closes the gap. Pure registry lookup, so
/// it stays sync.
#[tauri::command]
pub fn window_theme(window: Window) -> Scheme {
    window.theme().map_or(Scheme::Light, scheme_for)
}

/// React to a runtime OS theme flip. `WebView2` does not reliably deliver the
/// `prefers-color-scheme` change to a window that stays *focused/visible* - the
/// frontend's media query can go stale with no visibility/focus transition to
/// reconcile off - so the native `ThemeChanged` window event is the trustworthy
/// signal. Repaint the surface in-theme and hand the resolved scheme to that
/// window's frontend, which follows it verbatim (see prefs.svelte). Emitted only
/// to the window that changed, so each window re-themes from its own OS theme.
pub fn on_theme_changed(window: &Window, theme: Theme) {
    if let Some(webview) = window.get_webview_window(window.label()) {
        let _ = webview.set_background_color(Some(surface_for(theme)));
    }

    let _ = window.emit_to(window.label(), THEME_CHANGED_EVENT, scheme_for(theme));
}

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
/// until this moment.
pub fn on_destroyed(window: &Window) {
    let state = window.state::<WindowProjects>();
    let previous = state
        .projects
        .lock()
        .ok()
        .and_then(|mut projects| projects.remove(window.label()));
    if let Some(previous) = previous {
        mark_left_in_background(previous);
        broadcast_windows_changed(window.app_handle());
    }
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

/// Which neighbouring window to cycle to. Mirrors the frontend
/// `focusRelative("previous" | "next")` - one authoritative home for the two
/// names. `pub` because it appears in a `#[tauri::command]` signature.
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum CycleDirection {
    Previous,
    Next,
}

/// Stable creation-order sort key: the startup main window first (0), then each
/// spawned `w-{n}` by its sequence number. An unrecognised label sorts last so a
/// stray window never breaks the cycle. Lexicographic sorting would misplace
/// `w-10` before `w-2`; the numeric parse keeps them in creation order.
fn order_key(label: &str) -> u32 {
    if label == MAIN_WINDOW_LABEL {
        return 0;
    }
    label
        .strip_prefix(SPAWNED_LABEL_PREFIX)
        .and_then(|sequence| sequence.parse::<u32>().ok())
        .unwrap_or(u32::MAX)
}

/// The one sort key both the switcher list and the `Ctrl+Alt+[`/`]` cycle read, so
/// they never diverge. Windows the user has explicitly ordered (drag-reorder) come
/// first, in that order; any live window not yet in the explicit order follows, in
/// stable creation order. The label breaks remaining ties deterministically. A
/// label recorded in the explicit order but no longer live simply never gets a key
/// (only live windows are enumerated), so it drops out.
fn window_sort_key(label: &str, order: &[String]) -> (usize, u32, String) {
    let explicit = order
        .iter()
        .position(|ordered| ordered == label)
        .unwrap_or(usize::MAX);
    (explicit, order_key(label), label.to_string())
}

/// Every live window's label, in switcher order.
fn sorted_live_labels(app: &AppHandle, state: &WindowProjects) -> Vec<String> {
    let order = state.order_snapshot();
    let mut labels: Vec<String> = app.webview_windows().into_keys().collect();
    labels.sort_by_cached_key(|label| window_sort_key(label, &order));
    labels
}

/// Record the user's explicit "Open windows" order (the drag-reordered labels) so
/// both `window_list` and `window_focus_relative` sort by it - one source of truth
/// for the switcher's order and the `Ctrl+Alt+[`/`]` cycle. Session-scoped: labels
/// are ephemeral per app run, so there's nothing meaningful to persist to disk.
#[tauri::command]
pub fn window_reorder(app: AppHandle, state: tauri::State<WindowProjects>, labels: Vec<String>) {
    if let Ok(mut order) = state.order.lock() {
        *order = labels;
    }
    broadcast_windows_changed(&app);
}

/// The index `direction` lands on from `current` among `count` windows, wrapping
/// around at both ends.
fn cycled_index(current: usize, count: usize, direction: CycleDirection) -> usize {
    match direction {
        CycleDirection::Next => (current + 1) % count,
        CycleDirection::Previous => (current + count - 1) % count,
    }
}

/// Focus the previous/next open PADE window, wrapping around at the ends, in the
/// user's explicit switcher order (drag-reordered via `window_reorder`) and
/// creation order for any window not yet reordered - the same order the switcher's
/// "Open windows" list shows. Returns true when another window was focused (false
/// when this is the only window). The calling window is injected as `window`, so
/// the frontend passes only a direction; live windows are re-enumerated each press,
/// so a closed window simply drops out of the cycle.
#[tauri::command]
pub fn window_focus_relative(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<WindowProjects>,
    direction: CycleDirection,
) -> bool {
    let labels = sorted_live_labels(&app, &state);
    let Some(current) = labels.iter().position(|label| label == window.label()) else {
        return false;
    };
    let is_only_window = labels.len() < 2;
    if is_only_window {
        return false;
    }

    let target_label = &labels[cycled_index(current, labels.len(), direction)];
    app.get_webview_window(target_label)
        .is_some_and(|target| bring_to_front(&target))
}

/// One open PADE window and the project it has: its unique `label` (the id the
/// frontend focuses by), the project path, and whether it is the calling window.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    label: String,
    path: String,
    is_current: bool,
}

/// Every open PADE window that has a project, in the user's explicit switcher order
/// (drag-reordered via `window_reorder`, falling back to creation order for any not
/// yet reordered), flagging the caller as the current window. Windows with no
/// registered project (an empty or picker window) are omitted. Drives the
/// switcher's "Open windows" section, so its order matches the `Ctrl+Alt+[`/`]`
/// cycle order - both read the one explicit order.
#[tauri::command]
pub fn window_list(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<WindowProjects>,
) -> Vec<WindowInfo> {
    let projects = state
        .projects
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    sorted_live_labels(&app, &state)
        .into_iter()
        .filter_map(|label| {
            let path = projects.get(&label)?.clone();
            let is_current = label == window.label();
            Some(WindowInfo {
                label,
                path,
                is_current,
            })
        })
        .collect()
}

/// Focus a specific open PADE window by label - the switcher's "Open windows"
/// rows. Returns true when that window existed and took focus.
#[tauri::command]
pub fn window_focus_label(app: AppHandle, label: String) -> bool {
    app.get_webview_window(&label)
        .is_some_and(|target| bring_to_front(&target))
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
}

impl<'path> LaunchMode<'path> {
    /// Resolve the requested `mode` + optional `path` into a launch intent. A
    /// present `path` always means "open that project", regardless of `mode`.
    fn resolve(mode: &str, path: Option<&'path str>) -> Self {
        if let Some(path) = path {
            return Self::Open(path);
        }
        match mode {
            "temp" => Self::Temp,
            "dashboard" => Self::Dashboard,
            _ => Self::Empty,
        }
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
#[tauri::command]
pub async fn window_create(
    app: AppHandle,
    mode: String,
    path: Option<String>,
) -> Result<(), String> {
    let launch = LaunchMode::resolve(&mode, path.as_deref());
    let label = launch.label();
    if let Some(open) = app.get_webview_window(&label) {
        bring_to_front(&open);
        return Ok(());
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
    app_window_builder(&app, &label, url, main_look.as_ref())
        .build()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{cycled_index, order_key, window_sort_key, CycleDirection, LaunchMode};

    /// Sort the live window labels the way `window_list`/`window_focus_relative` do,
    /// given the user's explicit order - the behaviour both commands share.
    fn sorted(live: &[&str], order: &[&str]) -> Vec<String> {
        let order: Vec<String> = order.iter().map(|label| (*label).to_string()).collect();
        let mut labels: Vec<String> = live.iter().map(|label| (*label).to_string()).collect();
        labels.sort_by_cached_key(|label| window_sort_key(label, &order));
        labels
    }

    #[test]
    fn explicit_order_wins_over_creation_order() {
        // Creation order alone would be main, w-1, w-2; the explicit order overrides it.
        assert_eq!(
            sorted(&["main", "w-1", "w-2"], &["w-2", "main", "w-1"]),
            ["w-2", "main", "w-1"]
        );
    }

    #[test]
    fn unknown_windows_fall_back_to_creation_order() {
        // Only w-2 is explicitly ordered; the rest follow in creation order - and
        // w-2 (numeric 2) precedes w-10, which lexicographic sorting would misplace.
        assert_eq!(
            sorted(&["main", "w-2", "w-10"], &["w-2"]),
            ["w-2", "main", "w-10"]
        );
    }

    #[test]
    fn removed_label_is_ignored() {
        // w-9 sits in the explicit order but is no longer live, so it never appears;
        // the remaining live windows keep the explicit order.
        assert_eq!(
            sorted(&["main", "w-1"], &["w-1", "w-9", "main"]),
            ["w-1", "main"]
        );
    }

    #[test]
    fn creation_order_key_parses_sequence_numerically() {
        assert_eq!(order_key("main"), 0);
        assert!(order_key("w-2") < order_key("w-10"));
    }

    #[test]
    fn cycling_wraps_around_at_both_ends() {
        assert_eq!(cycled_index(2, 3, CycleDirection::Next), 0);
        assert_eq!(cycled_index(0, 3, CycleDirection::Previous), 2);
        assert_eq!(cycled_index(1, 3, CycleDirection::Next), 2);
    }

    #[test]
    fn a_path_always_opens_that_project_whatever_the_mode() {
        assert_eq!(LaunchMode::resolve("temp", None).query(), "w=temp");
        assert_eq!(LaunchMode::resolve("empty", None).query(), "w=empty");
        assert_eq!(LaunchMode::resolve("anything", None).query(), "w=empty");
        assert_eq!(
            LaunchMode::resolve("temp", Some(r"C:\my project")).query(),
            "w=open&path=C%3A%5Cmy%20project"
        );
    }

    #[test]
    fn the_dashboard_always_takes_its_one_label() {
        let dashboard = LaunchMode::resolve("dashboard", None);
        assert_eq!(dashboard.query(), "w=dashboard");
        assert_eq!(dashboard.label(), dashboard.label());
        assert!(LaunchMode::resolve("empty", None).label().starts_with("w-"));
    }
}
