//! Multi-window — spawn additional PADE app windows (Ctrl+Shift+N / app menu).
//!
//! Each new window loads the same frontend `index.html` with a query string that
//! tells the app what to boot into (`?w=empty` picker, `?w=temp` throwaway
//! workspace, or `?w=open&path=<enc>` a specific project). The frontend routing
//! that reads `location.search` lives in the shell task — this module only spawns
//! a window that loads the app with the right query. New windows clone the main
//! window's sizing/decorations so they feel identical.

use std::sync::atomic::{AtomicU32, Ordering};

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

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
            let encoded = percent_encode(path.as_deref().unwrap_or_default());
            format!("w=open&path={encoded}")
        }
    };

    let seq = WINDOW_SEQ.fetch_add(1, Ordering::Relaxed);
    let label = format!("w-{seq}");

    // In dev, point runtime windows at the dev server explicitly. A relative
    // `WebviewUrl::App` on a spawned window doesn't reliably load the dev server
    // (it renders a blank window), so resolve the absolute dev URL and carry the
    // launch query on it. In a bundled build there is no dev URL, so fall back to
    // the app asset path, which works there.
    let webview_url = match app.config().build.dev_url.clone() {
        Some(mut dev) => {
            dev.set_path("/index.html");
            dev.set_query(Some(&query));
            WebviewUrl::External(dev)
        }
        None => WebviewUrl::App(format!("index.html?{query}").into()),
    };

    let mut builder = WebviewWindowBuilder::new(&app, &label, webview_url).title("PADE");

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

    // NOTE: runtime-created windows must NOT be transparent — a transparent
    // WebView2 window spawned at runtime renders as a blank white screen on
    // Windows (tauri-apps/tauri#10011, #13963). The main window's transparency
    // is set up at startup and is unaffected; spawned windows stay opaque.
    builder
        .min_inner_size(720.0, 480.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Percent-encode a string for use as a URL query value, dependency-free. Keeps
/// the RFC 3986 unreserved set (`A–Z a–z 0–9 - _ . ~`) verbatim and encodes every
/// other byte as `%XX` — so Windows path separators/spaces survive the round-trip.
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &byte in input.as_bytes() {
        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if unreserved {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(hex_digit(byte >> 4));
            out.push(hex_digit(byte & 0x0f));
        }
    }
    out
}

/// The uppercase hex character for a nibble (0..=15).
fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => char::from(b'0' + nibble),
        _ => char::from(b'A' + (nibble - 10)),
    }
}
