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

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

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
        projects.insert(window.label().to_string(), normalize(&path));
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
            .filter(|(label, project)| **label != me && **project == target)
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
#[tauri::command]
pub fn window_create(app: AppHandle, mode: String, path: Option<String>) -> Result<(), String> {
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

    builder
        .min_inner_size(720.0, 480.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}
