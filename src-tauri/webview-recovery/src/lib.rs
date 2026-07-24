//! Safe wrapper over `WebView2`'s `ProcessFailed` event.
//!
//! `WebView2` splits into a browser process and per-page renderer processes;
//! either can die under load. This crate owns the unsafe COM subscription and
//! applies Microsoft's documented recovery for the recoverable kinds itself:
//! a dead *renderer* is healed in place with `Reload()`, a *hung* renderer
//! only after it stays hung across consecutive reports (the event re-fires
//! roughly every 15s while unresponsiveness lasts, and Microsoft's guidance is
//! to pick a threshold rather than reload on the first report — a long
//! synchronous script or a busy system often recovers on its own). A dead
//! *browser process* leaves every COM object defunct — nothing can be done on
//! this side, so it is escalated to the caller, who must rebuild the window.
//! Every failure logs its kind/reason pair; crash dumps land in the user data
//! folder's failure-report directory for deeper diagnosis.

use std::sync::atomic::{AtomicU32, Ordering};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2Controller, ICoreWebView2ProcessFailedEventArgs,
    ICoreWebView2ProcessFailedEventArgs2, COREWEBVIEW2_PROCESS_FAILED_KIND,
    COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
    COREWEBVIEW2_PROCESS_FAILED_REASON,
};
use webview2_com::ProcessFailedEventHandler;
use windows_core::Interface;

/// How many consecutive unresponsive reports (one roughly every 15s) a main
/// frame renderer gets before it is reloaded — about half a minute of true
/// hang, enough for a busy system or a heavy synchronous script to recover
/// without losing the page's state to a needless reload.
const UNRESPONSIVE_REPORTS_BEFORE_RELOAD: u32 = 2;

/// The failure's reason category, for the diagnostic log line. A missing
/// `Args2` interface (an old runtime) just logs the kind alone.
fn failure_reason(args: &ICoreWebView2ProcessFailedEventArgs) -> Option<i32> {
    let extended: ICoreWebView2ProcessFailedEventArgs2 = args.cast().ok()?;
    let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON::default();
    // SAFETY: `extended` is live for the duration of the callback and the
    // out-pointer targets a local of the exact ABI type.
    unsafe { extended.Reason(&raw mut reason) }.ok()?;
    Some(reason.0)
}

/// Subscribe `controller`'s webview to `ProcessFailed`. Renderer failures are
/// reloaded in place here (a hang only after it persists);
/// `on_browser_process_gone` fires only when the browser process itself died
/// and the whole window must be recreated. The subscription lives as long as
/// the webview, so the registration token is deliberately not kept.
pub fn watch_process_failed(
    controller: &ICoreWebView2Controller,
    on_browser_process_gone: Box<dyn Fn() + Send>,
) -> windows_core::Result<()> {
    // SAFETY: `controller` is a live COM interface handed out by wry on the
    // `WebView2` UI thread; the getter only reads a vtable slot.
    let webview = unsafe { controller.CoreWebView2() }?;
    let consecutive_unresponsive = AtomicU32::new(0);
    let handler = ProcessFailedEventHandler::create(Box::new(
        move |sender: Option<ICoreWebView2>,
              args: Option<ICoreWebView2ProcessFailedEventArgs>| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
            // SAFETY: `args` is live for the duration of the callback and the
            // out-pointer targets a local of the exact ABI type.
            unsafe { args.ProcessFailedKind(&raw mut kind) }?;
            let reason = failure_reason(&args);
            eprintln!(
                "webview process failure: kind={} reason={:?} (dumps in the user data folder's failure reports)",
                kind.0, reason
            );

            if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE {
                let reports = consecutive_unresponsive.fetch_add(1, Ordering::Relaxed) + 1;
                if reports < UNRESPONSIVE_REPORTS_BEFORE_RELOAD {
                    return Ok(());
                }
            }

            let renderer_gone = kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE;
            if renderer_gone {
                consecutive_unresponsive.store(0, Ordering::Relaxed);
                if let Some(webview) = sender {
                    // SAFETY: the sender webview is still valid — only its
                    // renderer died or hung; `Reload()` is the documented recovery.
                    unsafe { webview.Reload() }?;
                }
                return Ok(());
            }

            consecutive_unresponsive.store(0, Ordering::Relaxed);
            if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                on_browser_process_gone();
            }
            // GPU, utility, and frame-renderer failures recover on their own
            // (or affect only iframes PADE does not use) — logged above, no
            // action, per Microsoft's process-related-events guidance.
            Ok(())
        },
    ));
    let mut token = 0i64;
    // SAFETY: standard COM event registration; `handler` is a live interface
    // and the token out-pointer targets a local.
    unsafe { webview.add_ProcessFailed(&handler, &raw mut token) }
}
