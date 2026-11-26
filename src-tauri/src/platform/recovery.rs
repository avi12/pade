//! Auto-recovery from a crashed `WebView2` process.
//!
//! Under heavy load a window's `WebView2` browser process can die; the native
//! (tao) window survives but renders a permanent black void. The backend is
//! untouched - PTY sessions are registered per window label and the frontend
//! re-adopts them on a same-window reload - so the webview is disposable.
//! `guard` subscribes each window to `WebView2`'s `ProcessFailed` event (the
//! unsafe COM side lives in the `webview-recovery` workspace crate, since this
//! crate forbids unsafe code): a dead renderer is reloaded in place there,
//! while a dead browser process - where every COM object is defunct - is
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
pub(crate) fn is_recreate_pending() -> bool {
    PENDING_RECREATES.load(Ordering::Relaxed) > 0
}

/// The address each armed window reopens at, keyed by window label: one entry
/// per live webview. The entry is the whole contract - a window is watched
/// exactly while it has one, so arming again only moves the address, and the
/// crash that takes the entry is the only one that rebuilds the window.
#[cfg(windows)]
struct ArmedAddresses(std::collections::BTreeMap<String, tauri::Url>);

#[cfg(windows)]
impl ArmedAddresses {
    /// Point `label` at `url`. True when the label had no entry, so its
    /// webview still needs its one `ProcessFailed` subscription.
    fn arm(&mut self, label: &str, url: tauri::Url) -> bool {
        self.0.insert(label.to_string(), url).is_none()
    }

    /// Hand over `label`'s address and stop tracking it. `None` when a
    /// rebuild already took it, or the window closed.
    fn take(&mut self, label: &str) -> Option<tauri::Url> {
        self.0.remove(label)
    }
}

#[cfg(windows)]
static ARMED_ADDRESSES: std::sync::Mutex<ArmedAddresses> =
    std::sync::Mutex::new(ArmedAddresses(std::collections::BTreeMap::new()));

#[cfg(windows)]
fn take_armed_address(label: &str) -> Option<tauri::Url> {
    ARMED_ADDRESSES.lock().ok()?.take(label)
}

/// Forget a window that is gone, so a later window under the same label
/// subscribes its own webview.
#[cfg(not(windows))]
pub(crate) fn disarm(_label: &str) {}

/// Forget a window that is gone, so a later window under the same label
/// subscribes its own webview.
#[cfg(windows)]
pub(crate) fn disarm(label: &str) {
    drop(take_armed_address(label));
}

/// Arm crash auto-recovery for the calling window. The FRONTEND invokes this
/// on every boot, and again whenever its address moves, with its live
/// `location.href`: the frontend is the one party that always knows the
/// window's real URL - a backend capture at window creation raced navigation
/// and once rebuilt a crashed window onto `about:blank` - and a rebuilt
/// window's fresh boot arms its replacement webview by the same route.
/// A webview is subscribed once: arming it again (a reload, a project switch)
/// only updates the address it would reopen at. Stacked subscriptions once
/// rebuilt a crashed window as many times as it had been armed.
/// Best-effort: a window that fails to arm just keeps the black-void-until-
/// manual-restart behavior.
#[cfg(not(windows))]
#[tauri::command]
pub fn recovery_arm(_window: WebviewWindow, _url: String) {}

/// Arm crash auto-recovery for the calling window (see the non-Windows stub's
/// doc for the contract; this is the working half).
#[cfg(windows)]
#[tauri::command]
pub(crate) fn recovery_arm(window: WebviewWindow, url: String) {
    use tauri::Manager;

    let label = window.label().to_string();
    let Ok(url) = tauri::Url::parse(&url) else {
        eprintln!("webview crash recovery: unparseable url from {label}");
        return;
    };
    let Ok(mut armed_addresses) = ARMED_ADDRESSES.lock() else {
        eprintln!("webview crash recovery not armed for {label}: its registry is poisoned");
        return;
    };
    let is_first_arm = armed_addresses.arm(&label, url);
    drop(armed_addresses);
    if !is_first_arm {
        return;
    }

    let app = window.app_handle().clone();
    let subscribed = window.with_webview({
        let label = label.clone();
        move |platform_webview| {
            let watched = webview_recovery::watch_process_failed(
                &platform_webview.controller(),
                Box::new({
                    let label = label.clone();
                    move || recreate(&app, &label)
                }),
            );
            if let Err(error) = watched {
                eprintln!("webview crash recovery not armed: {error}");
                disarm(&label);
            }
        }
    });
    if let Err(error) = subscribed {
        eprintln!("webview crash recovery not armed for {label}: {error}");
        disarm(&label);
    }
}

/// The still-live native window's placement, carried over to its replacement
/// so recovery is visually seamless.
#[cfg(windows)]
struct Placement {
    look: crate::platform::window::WindowLook,
    position: Option<tauri::PhysicalPosition<i32>>,
    is_maximized: bool,
}

/// Replace `label`'s window after its browser process died. Runs inside the
/// COM callback on the event-loop thread: capture placement, destroy, then
/// hand the rebuild to a worker thread (building a webview window on the
/// event-loop thread deadlocks - see `window_create`). Taking the armed
/// address is what keeps this to one rebuild per crash.
#[cfg(windows)]
fn recreate(app: &tauri::AppHandle, label: &str) {
    use tauri::Manager;

    let Some(url) = take_armed_address(label) else {
        return;
    };
    let Some(crashed) = app.get_webview_window(label) else {
        return;
    };
    let placement = Placement {
        look: crate::platform::window::WindowLook::from_window(&crashed),
        position: crashed.outer_position().ok(),
        is_maximized: crashed.is_maximized().unwrap_or(false),
    };
    PENDING_RECREATES.fetch_add(1, Ordering::Relaxed);
    // destroy(), not close(): close asks the frontend to confirm via its
    // close-requested handler - an answer a dead webview can never give.
    if crashed.destroy().is_err() {
        PENDING_RECREATES.fetch_sub(1, Ordering::Relaxed);
        return;
    }
    let app = app.clone();
    let label = label.to_string();
    std::thread::spawn(move || {
        rebuild(&app, &label, &url, &placement);
        PENDING_RECREATES.fetch_sub(1, Ordering::Relaxed);
    });
}

/// Build the replacement window once the event loop has released the label.
#[cfg(windows)]
fn rebuild(app: &tauri::AppHandle, label: &str, url: &tauri::Url, placement: &Placement) {
    use std::time::Duration;

    use tauri::{Manager, WebviewUrl};

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

    let mut builder = crate::platform::window::app_window_builder(
        app,
        label,
        WebviewUrl::External(url.clone()),
        Some(&placement.look),
    );
    if let Some(position) = placement.position {
        builder = builder.position(f64::from(position.x), f64::from(position.y));
    }
    // No explicit re-arm once built: the replacement's frontend boots from the
    // real app URL and calls `recovery_arm` itself, like any window.
    let window = match builder.build() {
        Ok(window) => window,
        Err(error) => {
            eprintln!("webview crash recovery: rebuilding {label} failed: {error}");
            return;
        }
    };
    if placement.is_maximized {
        let _ = window.maximize();
    }
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use crate::platform::recovery::ArmedAddresses;

    fn address(query: &str) -> tauri::Url {
        tauri::Url::parse(&format!("http://localhost:1430/?{query}")).expect("a valid test url")
    }

    fn empty() -> ArmedAddresses {
        ArmedAddresses(std::collections::BTreeMap::new())
    }

    #[test]
    fn only_the_first_arm_of_a_window_asks_for_a_subscription() {
        let mut armed = empty();
        assert!(armed.arm("w-1", address("w=empty")));
        assert!(!armed.arm("w-1", address("w=empty")));
        assert!(armed.arm("w-2", address("w=empty")));
    }

    #[test]
    fn arming_again_moves_the_address_a_rebuild_reopens() {
        let mut armed = empty();
        armed.arm("w-1", address("w=empty"));
        armed.arm("w-1", address("w=temp"));
        assert_eq!(armed.take("w-1"), Some(address("w=temp")));
    }

    #[test]
    fn a_crash_is_taken_once_however_many_handlers_report_it() {
        let mut armed = empty();
        armed.arm("w-1", address("w=empty"));
        assert!(armed.take("w-1").is_some());
        assert!(armed.take("w-1").is_none());
    }

    #[test]
    fn a_rebuilt_window_subscribes_its_new_webview() {
        let mut armed = empty();
        armed.arm("w-1", address("w=empty"));
        armed.take("w-1");
        assert!(armed.arm("w-1", address("w=empty")));
    }
}
