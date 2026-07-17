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
fn surface_for(theme: Theme) -> Color {
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

/// Which project each window currently has open, keyed by window label. Lets the
/// picker focus an already-open project's window instead of opening it twice.
#[derive(Default)]
pub struct WindowProjects(pub Mutex<HashMap<String, String>>);

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
    if let Ok(mut projects) = state.0.lock() {
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
        let Ok(projects) = state.0.lock() else {
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
        if let Ok(mut projects) = state.0.lock() {
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

/// Focus the previous/next open PADE window in stable creation order, wrapping
/// around at the ends. Returns true when another window was focused (false when
/// this is the only window). The calling window is injected as `window`, so the
/// frontend passes only a direction; live windows are re-enumerated each press,
/// so a closed window simply drops out of the cycle.
#[tauri::command]
pub fn window_focus_relative(
    app: AppHandle,
    window: WebviewWindow,
    direction: CycleDirection,
) -> bool {
    let mut labels: Vec<String> = app.webview_windows().into_keys().collect();
    labels.sort_by_key(|label| (order_key(label), label.clone()));

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

/// Every open PADE window that has a project, in stable creation order, flagging
/// the caller as the current window. Windows with no registered project (an empty
/// or picker window) are omitted. Drives the switcher's "Open windows" section, so
/// its order matches the `Ctrl+Shift+Alt+[`/`]` cycle order.
#[tauri::command]
pub fn window_list(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<WindowProjects>,
) -> Vec<WindowInfo> {
    let me = window.label();
    let projects = state
        .0
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let mut labels: Vec<String> = app.webview_windows().into_keys().collect();
    labels.sort_by_key(|label| (order_key(label), label.clone()));
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
    builder
        .min_inner_size(720.0, 480.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}
