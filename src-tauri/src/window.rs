//! Multi-window — spawn additional PADE app windows (Ctrl+Shift+N / app menu).
//!
//! Each new window loads the same frontend `index.html` with a query string that
//! tells the app what to boot into (`?w=empty` picker, `?w=temp` throwaway
//! workspace, or `?w=open&path=<enc>` a specific project). The frontend routing
//! that reads `location.search` lives in the shell task — this module only spawns
//! a window that loads the app with the right query. New windows clone the main
//! window's sizing/decorations so they feel identical.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::window::Color;
use tauri::{AppHandle, Manager, Theme, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// M3 surface colors, mirroring `--surface` in `src/theme.css` for the light and
/// dark schemes. Painted as the webview background at window creation so a window
/// opens already in-theme: `WebView2` otherwise shows an unthemed white surface
/// until the HTML/CSS first paints, flashing white on a dark desktop. This is the
/// native side of the token — Rust can't read the CSS custom property — so keep
/// the two in sync.
const SURFACE_LIGHT: Color = Color(248, 250, 251, 255); // hsl(210deg 30% 98%)
const SURFACE_DARK: Color = Color(14, 20, 27, 255); // hsl(214deg 30% 8%)

/// The surface color matching a resolved OS theme.
pub(crate) fn surface_for(theme: Theme) -> Color {
    match theme {
        Theme::Dark => SURFACE_DARK,
        // `Theme` is `#[non_exhaustive]`; treat light and anything new as light.
        _ => SURFACE_LIGHT,
    }
}

/// Paint `window`'s webview with the themed surface so it shows in-theme before
/// the frontend renders. Best-effort — a failed theme probe leaves the default.
pub fn paint_surface(window: &WebviewWindow) {
    if let Ok(theme) = window.theme() {
        let _ = window.set_background_color(Some(surface_for(theme)));
    }
}

/// Per-window state the switcher reads: which project each window has open (keyed
/// by window label), and the user's explicit drag-reorder of the "Open windows"
/// list. The order is session-scoped — window labels are ephemeral per app run, so
/// persisting it across restarts by label would be meaningless.
#[derive(Default)]
pub struct WindowProjects {
    /// Which project each window currently has open, keyed by window label. Lets the
    /// picker focus an already-open project's window instead of opening it twice.
    projects: Mutex<HashMap<String, String>>,
    /// The user's explicit label order for the switcher list and the `Ctrl+Alt+[`/`]`
    /// cycle — the single source both read. A live window absent from it falls back
    /// to creation order (`order_key`) after the explicitly ordered ones.
    order: Mutex<Vec<String>>,
}

impl WindowProjects {
    /// The project registered to one live application window. Native commands
    /// use this instead of process-global cwd when selecting project data.
    pub(crate) fn project_for(&self, label: &str) -> Result<String, String> {
        self.projects
            .lock()
            .map_err(|e| e.to_string())?
            .get(label)
            .filter(|path| !path.is_empty())
            .cloned()
            .ok_or_else(|| "this window has no registered project".to_string())
    }
}

/// Canonicalize a path for cross-window comparison — `/`-separated, no trailing
/// slash, lowercased on case-insensitive Windows.
fn normalize(path: &str) -> String {
    let trimmed = path.replace('\\', "/");
    let trimmed = trimmed.trim_end_matches('/');
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    }
}

/// Record the project the calling window now has open.
#[tauri::command]
pub fn window_register_project(
    window: WebviewWindow,
    state: tauri::State<WindowProjects>,
    path: String,
) {
    if let Ok(mut projects) = state.projects.lock() {
        // Stored verbatim so the switcher's "Open windows" list can show the real
        // path/name; comparison normalizes on read (see `window_focus_project`).
        projects.insert(window.label().to_string(), path);
    }
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
    let target = normalize(&path);
    let me = window.label().to_string();
    let candidates: Vec<String> = {
        let Ok(projects) = state.projects.lock() else {
            return false;
        };
        projects
            .iter()
            .filter(|(label, project)| **label != me && normalize(project) == target)
            .map(|(label, _)| label.clone())
            .collect()
    };

    for label in candidates {
        if let Some(target_window) = app.get_webview_window(&label) {
            let _ = target_window.unminimize();
            let _ = target_window.set_focus();
            return true;
        }
        // The window is gone — drop its stale entry.
        if let Ok(mut projects) = state.projects.lock() {
            projects.remove(&label);
        }
    }
    false
}

/// Which neighbouring window to cycle to. Mirrors the frontend
/// `focusRelative("previous" | "next")` — one authoritative home for the two
/// names. `pub` because it appears in a `#[tauri::command]` signature.
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum CycleDirection {
    Previous,
    Next,
}

/// Stable creation-order sort key: the startup `main` window first (0), then each
/// spawned `w-{n}` by its sequence number. An unrecognised label sorts last so a
/// stray window never breaks the cycle. Lexicographic sorting would misplace
/// `w-10` before `w-2`; the numeric parse keeps them in creation order.
fn order_key(label: &str) -> u32 {
    if label == "main" {
        return 0;
    }
    label
        .strip_prefix("w-")
        .and_then(|seq| seq.parse::<u32>().ok())
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

/// Record the user's explicit "Open windows" order (the drag-reordered labels) so
/// both `window_list` and `window_focus_relative` sort by it — one source of truth
/// for the switcher's order and the `Ctrl+Alt+[`/`]` cycle. Session-scoped: labels
/// are ephemeral per app run, so there's nothing meaningful to persist to disk.
#[tauri::command]
pub fn window_reorder(state: tauri::State<WindowProjects>, labels: Vec<String>) {
    if let Ok(mut order) = state.order.lock() {
        *order = labels;
    }
}

/// Focus the previous/next open PADE window, wrapping around at the ends, in the
/// user's explicit switcher order (drag-reordered via `window_reorder`) and
/// creation order for any window not yet reordered — the same order the switcher's
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
    let order = state
        .order
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let mut labels: Vec<String> = app.webview_windows().into_keys().collect();
    labels.sort_by_cached_key(|label| window_sort_key(label, &order));

    let me = window.label();
    let Some(current) = labels.iter().position(|label| label == me) else {
        return false;
    };
    let count = labels.len();
    if count < 2 {
        return false;
    }

    let target_index = match direction {
        CycleDirection::Next => (current + 1) % count,
        CycleDirection::Previous => (current + count - 1) % count,
    };

    let Some(target) = app.get_webview_window(&labels[target_index]) else {
        return false;
    };
    let _ = target.unminimize();
    target.set_focus().is_ok()
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
/// cycle order — both read the one explicit order.
#[tauri::command]
pub fn window_list(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<WindowProjects>,
) -> Vec<WindowInfo> {
    let me = window.label();
    let projects = state
        .projects
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let order = state
        .order
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let mut labels: Vec<String> = app.webview_windows().into_keys().collect();
    labels.sort_by_cached_key(|label| window_sort_key(label, &order));
    labels
        .into_iter()
        .filter_map(|label| {
            let path = projects.get(&label)?.clone();
            let is_current = label.as_str() == me;
            Some(WindowInfo {
                label,
                path,
                is_current,
            })
        })
        .collect()
}

/// Focus a specific open PADE window by label — the switcher's "Open windows"
/// rows. Returns true when that window existed and took focus.
#[tauri::command]
pub fn window_focus_label(app: AppHandle, label: String) -> bool {
    if let Some(target) = app.get_webview_window(&label) {
        let _ = target.unminimize();
        return target.set_focus().is_ok();
    }
    false
}

/// Monotonic counter for unique window labels (`w-1`, `w-2`, …). Labels must be
/// unique per app run; a simple atomic is enough and needs no dependency.
static WINDOW_SEQ: AtomicU32 = AtomicU32::new(1);

/// The launch intents a spawned window can carry, in the exact `w=` query strings
/// the frontend router reads. One authoritative home for the mode literals.
enum LaunchMode {
    Empty,
    Temp,
    Open,
}

impl LaunchMode {
    /// Resolve the requested `mode` + optional `path` into a launch intent. A
    /// present `path` always means "open that project", regardless of `mode`.
    fn resolve(mode: &str, path: Option<&str>) -> Self {
        if path.is_some() {
            return LaunchMode::Open;
        }
        match mode {
            "temp" => LaunchMode::Temp,
            _ => LaunchMode::Empty,
        }
    }
}

/// Spawn a new PADE window loading the app with a `w=` query describing what to
/// boot into. `mode` is `"empty"` | `"temp"`; a present `path` opens that project.
///
/// **`async` is load-bearing on Windows.** Tauri runs a synchronous command *on the
/// event loop thread*, and building a webview window blocks that thread waiting for
/// `WebView2` to hand the webview back — which it can only do from the event loop. The
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
    let query = match LaunchMode::resolve(&mode, path.as_deref()) {
        LaunchMode::Empty => "w=empty".to_string(),
        LaunchMode::Temp => "w=temp".to_string(),
        // `path` is Some here by construction of `resolve`.
        LaunchMode::Open => {
            let encoded = crate::util::percent_encode(path.as_deref().unwrap_or_default(), b"");
            format!("w=open&path={encoded}")
        }
    };

    let seq = WINDOW_SEQ.fetch_add(1, Ordering::Relaxed);
    let label = format!("w-{seq}");
    let url = format!("index.html?{query}");

    let mut builder =
        WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into())).title("PADE");

    // Clone the main window's sizing/decorations so a spawned window matches it.
    if let Some(main) = app.get_webview_window("main") {
        // Open in-theme like the main window, avoiding a white flash on dark.
        if let Ok(theme) = main.theme() {
            builder = builder.background_color(surface_for(theme));
        }
        if let Ok(size) = main.inner_size() {
            #[allow(clippy::cast_precision_loss)]
            {
                builder = builder.inner_size(f64::from(size.width), f64::from(size.height));
            }
        }
        if let Ok(decorated) = main.is_decorated() {
            builder = builder.decorations(decorated);
        }
    }

    // Build visible: the builder's `background_color` above already paints the
    // themed surface at creation, so there's no white flash to hide. (Building
    // hidden and calling `show()` here left the window invisible — a freshly
    // built webview window doesn't reliably show from within the command.)
    let window = builder
        .min_inner_size(720.0, 480.0)
        .build()
        .map_err(|e| e.to_string())?;
    // Crash recovery arms once the window's frontend boots and calls
    // `recovery_arm` with its live URL (see recovery.rs).
    let _ = window;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{order_key, window_sort_key};

    /// Sort the live window labels the way `window_list`/`window_focus_relative` do,
    /// given the user's explicit order — the behaviour both commands share.
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
        // Only w-2 is explicitly ordered; the rest follow in creation order — and
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
}
