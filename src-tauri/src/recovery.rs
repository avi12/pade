//! Auto-recovery from a crashed `WebView2` process.
//!
//! Under heavy load a window's `WebView2` browser process can die; the native
//! (tao) window survives but renders a permanent black void. The backend is
//! untouched — PTY sessions are registered per window label and the frontend
//! re-adopts them on a same-window reload — so the webview is disposable.
//! `guard` subscribes each window to `WebView2`'s `ProcessFailed` event (the
//! unsafe COM side lives in the `webview-recovery` workspace crate, since this
//! crate forbids unsafe code): a dead renderer is reloaded in place there,
//! while a dead browser process — where every COM object is defunct — is
//! escalated here to destroy the window and rebuild it with the same label and
//! URL, which is what lets session re-adoption kick in.

use std::sync::atomic::{AtomicUsize, Ordering};

use tauri::WebviewWindow;

/// Windows whose crashed webview is currently being rebuilt. Read by the app's
/// `ExitRequested` handler: destroying the last window mid-recovery would
/// otherwise exit the whole app before the replacement exists.
static PENDING_RECREATES: AtomicUsize = AtomicUsize::new(0);

/// Whether any window is currently between destroy and rebuild, so an
/// `ExitRequested` fired by that destroy must be vetoed.
pub fn recreate_pending() -> bool {
    PENDING_RECREATES.load(Ordering::Relaxed) > 0
}

/// Arm crash auto-recovery for the calling window. The FRONTEND invokes this
/// on every boot with its live `location.href`: the frontend is the one party
/// that always knows the window's real URL — a backend capture at window
/// creation raced navigation and once rebuilt a crashed window onto
/// `about:blank` — and a rebuilt window's fresh boot re-arms its replacement
/// webview by the same route. Re-invocations on the same webview (an HMR
/// full reload) stack duplicate handlers, which is harmless: a second
/// recreate finds the label already gone, a second `Reload()` is idempotent.
/// Best-effort: a window that fails to arm just keeps the black-void-until-
/// manual-restart behavior.
#[cfg(not(windows))]
#[tauri::command]
pub fn recovery_arm(_window: WebviewWindow, _url: String) {}

/// Arm crash auto-recovery for the calling window (see the non-Windows stub's
/// doc for the contract; this is the working half).
#[cfg(windows)]
#[tauri::command]
pub fn recovery_arm(window: WebviewWindow, url: String) {
    use tauri::Manager;

    let Ok(url) = tauri::Url::parse(&url) else {
        eprintln!(
            "webview crash recovery: unparseable url from {}",
            window.label()
        );
        return;
    };

    let app = window.app_handle().clone();
    let label = window.label().to_string();
    let armed = window.with_webview({
        let label = label.clone();
        move |platform_webview| {
            let watched = webview_recovery::watch_process_failed(
                &platform_webview.controller(),
                Box::new(move || recreate(&app, &label, &url)),
            );
            if let Err(error) = watched {
                eprintln!("webview crash recovery not armed: {error}");
            }
        }
    });
    if let Err(error) = armed {
        eprintln!("webview crash recovery not armed for {label}: {error}");
    }
}

/// The still-live native window's placement, carried over to its replacement
/// so recovery is visually seamless.
#[cfg(windows)]
struct Placement {
    position: Option<tauri::PhysicalPosition<i32>>,
    size: Option<tauri::PhysicalSize<u32>>,
    maximized: bool,
    decorated: Option<bool>,
    theme: Option<tauri::Theme>,
}

/// Replace `label`'s window after its browser process died. Runs inside the
/// COM callback on the event-loop thread: capture placement, destroy, then
/// hand the rebuild to a worker thread (building a webview window on the
/// event-loop thread deadlocks — see `window_create`).
#[cfg(windows)]
fn recreate(app: &tauri::AppHandle, label: &str, url: &tauri::Url) {
    use tauri::Manager;

    let Some(dead) = app.get_webview_window(label) else {
        return;
    };
    let placement = Placement {
        position: dead.outer_position().ok(),
        size: dead.inner_size().ok(),
        maximized: dead.is_maximized().unwrap_or(false),
        decorated: dead.is_decorated().ok(),
        theme: dead.theme().ok(),
    };
    PENDING_RECREATES.fetch_add(1, Ordering::Relaxed);
    // destroy(), not close(): close asks the frontend to confirm via its
    // close-requested handler — an answer a dead webview can never give.
    if dead.destroy().is_err() {
        PENDING_RECREATES.fetch_sub(1, Ordering::Relaxed);
        return;
    }
    let app = app.clone();
    let label = label.to_string();
    let url = url.clone();
    std::thread::spawn(move || {
        rebuild(&app, &label, &url, &placement);
        PENDING_RECREATES.fetch_sub(1, Ordering::Relaxed);
    });
}

/// Build the replacement window once the event loop has released the label.
#[cfg(windows)]
fn rebuild(app: &tauri::AppHandle, label: &str, url: &tauri::Url, placement: &Placement) {
    use std::time::Duration;

    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    // The destroy is processed by the event loop only after the COM callback
    // returns, so the label frees up moments later; there is no event for
    // "label released", hence a short bounded poll.
    const LABEL_RELEASE_POLL: Duration = Duration::from_millis(20);
    const LABEL_RELEASE_ATTEMPTS: u32 = 250;
    for _ in 0..LABEL_RELEASE_ATTEMPTS {
        if app.get_webview_window(label).is_none() {
            break;
        }
        std::thread::sleep(LABEL_RELEASE_POLL);
    }
    if app.get_webview_window(label).is_some() {
        eprintln!("webview crash recovery: window {label} never released its label");
        return;
    }

    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::External(url.clone()))
        .title("PADE")
        .min_inner_size(720.0, 480.0);
    if let Some(theme) = placement.theme {
        builder = builder.background_color(crate::window::surface_for(theme));
    }
    if let Some(size) = placement.size {
        builder = builder.inner_size(f64::from(size.width), f64::from(size.height));
    }
    if let Some(position) = placement.position {
        builder = builder.position(f64::from(position.x), f64::from(position.y));
    }
    if let Some(decorated) = placement.decorated {
        builder = builder.decorations(decorated);
    }
    match builder.build() {
        Ok(window) => {
            if placement.maximized {
                let _ = window.maximize();
            }
            // No explicit re-arm: the replacement's frontend boots from the
            // real app URL and calls `recovery_arm` itself, like any window.
        }
        Err(error) => eprintln!("webview crash recovery: rebuilding {label} failed: {error}"),
    }
}
