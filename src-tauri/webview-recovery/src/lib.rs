//! Safe wrapper over `WebView2`'s `ProcessFailed` event.
//!
//! `WebView2` splits into a browser process and per-page renderer processes;
//! either can die under load. This crate owns the unsafe COM subscription and
//! applies Microsoft's documented recovery for the recoverable kinds itself:
//! a dead or hung *renderer* is healed in place with `Reload()`. A dead
//! *browser process* leaves every COM object defunct — nothing can be done on
//! this side, so it is escalated to the caller, who must rebuild the window.

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2Controller, ICoreWebView2ProcessFailedEventArgs,
    COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
};
use webview2_com::ProcessFailedEventHandler;

/// Subscribe `controller`'s webview to `ProcessFailed`. Renderer failures are
/// reloaded in place here; `on_browser_process_gone` fires only when the
/// browser process itself died and the whole window must be recreated. The
/// subscription lives as long as the webview, so the registration token is
/// deliberately not kept.
pub fn watch_process_failed(
    controller: &ICoreWebView2Controller,
    on_browser_process_gone: Box<dyn Fn() + Send>,
) -> windows_core::Result<()> {
    // SAFETY: `controller` is a live COM interface handed out by wry on the
    // `WebView2` UI thread; the getter only reads a vtable slot.
    let webview = unsafe { controller.CoreWebView2() }?;
    let handler = ProcessFailedEventHandler::create(Box::new(
        move |sender: Option<ICoreWebView2>,
              args: Option<ICoreWebView2ProcessFailedEventArgs>| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
            // SAFETY: `args` is live for the duration of the callback and the
            // out-pointer targets a local of the exact ABI type.
            unsafe { args.ProcessFailedKind(&raw mut kind) }?;
            let renderer_gone = kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE;
            if renderer_gone {
                if let Some(webview) = sender {
                    // SAFETY: the sender webview is still valid — only its
                    // renderer died; `Reload()` is the documented recovery.
                    unsafe { webview.Reload() }?;
                }
                return Ok(());
            }
            if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                on_browser_process_gone();
            }
            // Frame-renderer and utility-process failures recover on their
            // own (or affect only iframes PADE does not use) — no action.
            Ok(())
        },
    ));
    let mut token = 0i64;
    // SAFETY: standard COM event registration; `handler` is a live interface
    // and the token out-pointer targets a local.
    unsafe { webview.add_ProcessFailed(&handler, &raw mut token) }
}
