//! Hang watchdog for a window's renderer.
//!
//! `WebView2` reports a hung renderer (`RENDER_PROCESS_UNRESPONSIVE`) only after
//! the page ignores user input, so a page that hangs while nobody touches it -
//! a Blink loop during an OS light/dark flip, seen spinning for minutes - stays
//! frozen with nothing reporting it. And `Reload()` on a hung renderer has to
//! wait for that same renderer. So the host pings the page itself: a no-op
//! script, which any live renderer answers within milliseconds. A visible page
//! that leaves `UNANSWERED_PINGS_BEFORE_KILL` pings in a row unanswered is hung,
//! and its renderer process is handed to the caller to end. The renderer's exit
//! then takes the `ProcessFailed` path, which reloads the page in a fresh
//! process; the app's sessions live in the backend and are re-adopted.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2Controller, ICoreWebView2Environment13, ICoreWebView2FrameInfo2,
    ICoreWebView2ProcessExtendedInfo, ICoreWebView2ProcessExtendedInfoCollection, ICoreWebView2_2,
    ICoreWebView2_20, COREWEBVIEW2_PROCESS_KIND, COREWEBVIEW2_PROCESS_KIND_RENDERER,
};
use webview2_com::{ExecuteScriptCompletedHandler, GetProcessExtendedInfosCompletedHandler};
use windows_core::{w, Interface, BOOL};

/// How many pings in a row a visible page may leave unanswered before its
/// renderer counts as hung. With the caller pinging every 10 s that is about
/// 40 s of a page answering nothing at all - no legitimate work blocks the
/// page that long.
const UNANSWERED_PINGS_BEFORE_KILL: u32 = 4;

/// The no-op a live renderer answers at once.
const PING_SCRIPT: windows_core::PCWSTR = w!("0");

/// One window's count of pings its page has not answered yet.
#[derive(Debug, Default, Clone)]
pub struct HangWatch {
    unanswered: Arc<AtomicU32>,
}

/// What one ping found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PingOutcome {
    /// The ping went out; the page has answered the earlier ones or has time left.
    Sent,
    /// The window is hidden: a hidden page may be frozen on purpose, so it is
    /// not judged, and its count starts over.
    Skipped,
    /// The page stayed silent too long: its renderer is being looked up and
    /// handed to `on_hung_renderer`.
    Hung,
}

fn is_visible(controller: &ICoreWebView2Controller) -> windows_core::Result<bool> {
    let mut is_visible = BOOL::default();
    // SAFETY: `controller` is a live COM interface on its UI thread and the
    // out-pointer targets a local of the exact ABI type.
    unsafe { controller.IsVisible(&raw mut is_visible) }?;
    Ok(is_visible.as_bool())
}

/// Ping `controller`'s page once. Call it on the window's UI thread, at a
/// steady interval. On a hang, `on_hung_renderer` receives the process id of
/// the renderer running the page's main frame, once the runtime lists it.
pub fn ping(
    controller: &ICoreWebView2Controller,
    watch: &HangWatch,
    on_hung_renderer: Box<dyn FnOnce(u32) + Send>,
) -> windows_core::Result<PingOutcome> {
    if !is_visible(controller)? {
        watch.unanswered.store(0, Ordering::Relaxed);
        return Ok(PingOutcome::Skipped);
    }

    // SAFETY: as in `is_visible`; the getter only reads a vtable slot.
    let webview = unsafe { controller.CoreWebView2() }?;
    let is_hung = watch.unanswered.load(Ordering::Relaxed) >= UNANSWERED_PINGS_BEFORE_KILL;
    if is_hung {
        watch.unanswered.store(0, Ordering::Relaxed);
        find_main_frame_renderer(&webview, on_hung_renderer)?;
        return Ok(PingOutcome::Hung);
    }

    watch.unanswered.fetch_add(1, Ordering::Relaxed);
    let unanswered = Arc::clone(&watch.unanswered);
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |_result, _json| {
        // Any completion, an error included, means the renderer ran a task.
        unanswered.store(0, Ordering::Relaxed);
        Ok(())
    }));
    // SAFETY: `webview` is live on its UI thread; the script is a static wide
    // string and `handler` a live COM callback.
    unsafe { webview.ExecuteScript(PING_SCRIPT, &handler) }?;
    Ok(PingOutcome::Sent)
}

/// Ask the runtime which renderer runs `webview`'s main frame, and hand its
/// process id to `on_found`.
fn find_main_frame_renderer(
    webview: &ICoreWebView2,
    on_found: Box<dyn FnOnce(u32) + Send>,
) -> windows_core::Result<()> {
    let mut main_frame_id = 0u32;
    let webview_20: ICoreWebView2_20 = webview.cast()?;
    // SAFETY: live interface, out-pointer to a local of the ABI type.
    unsafe { webview_20.FrameId(&raw mut main_frame_id) }?;
    let webview_2: ICoreWebView2_2 = webview.cast()?;
    // SAFETY: live interface; the getter returns an owned COM reference.
    let environment: ICoreWebView2Environment13 = unsafe { webview_2.Environment() }?.cast()?;
    let handler =
        GetProcessExtendedInfosCompletedHandler::create(Box::new(move |result, processes| {
            result?;
            let Some(processes) = processes else {
                return Ok(());
            };
            match renderer_running(&processes, main_frame_id)? {
                Some(process_id) => on_found(process_id),
                None => eprintln!("webview hang watchdog: no renderer lists the hung main frame"),
            }
            Ok(())
        }));
    // SAFETY: live interface and a live COM completion callback.
    unsafe { environment.GetProcessExtendedInfos(&handler) }
}

/// The renderer process whose frames include `frame_id`.
fn renderer_running(
    processes: &ICoreWebView2ProcessExtendedInfoCollection,
    frame_id: u32,
) -> windows_core::Result<Option<u32>> {
    let mut count = 0u32;
    // SAFETY: live collection, out-pointer to a local of the ABI type.
    unsafe { processes.Count(&raw mut count) }?;
    for i in 0..count {
        // SAFETY: `i` is below the collection's own count.
        let process = unsafe { processes.GetValueAtIndex(i) }?;
        if let Some(process_id) = renderer_id_if_running(&process, frame_id)? {
            return Ok(Some(process_id));
        }
    }
    Ok(None)
}

fn renderer_id_if_running(
    process: &ICoreWebView2ProcessExtendedInfo,
    frame_id: u32,
) -> windows_core::Result<Option<u32>> {
    // SAFETY: live interface; the getter returns an owned COM reference.
    let info = unsafe { process.ProcessInfo() }?;
    let mut kind = COREWEBVIEW2_PROCESS_KIND::default();
    // SAFETY: live interface, out-pointer to a local of the ABI type.
    unsafe { info.Kind(&raw mut kind) }?;
    let is_renderer = kind == COREWEBVIEW2_PROCESS_KIND_RENDERER;
    if !is_renderer {
        return Ok(None);
    }

    let is_running_frame = runs_frame(process, frame_id)?;
    if !is_running_frame {
        return Ok(None);
    }

    let mut process_id = 0i32;
    // SAFETY: live interface, out-pointer to a local of the ABI type.
    unsafe { info.ProcessId(&raw mut process_id) }?;
    Ok(u32::try_from(process_id).ok())
}

fn runs_frame(
    process: &ICoreWebView2ProcessExtendedInfo,
    frame_id: u32,
) -> windows_core::Result<bool> {
    // SAFETY: live interface; the getters return owned COM references.
    let frames = unsafe { process.AssociatedFrameInfos() }?;
    // SAFETY: as above.
    let iterator = unsafe { frames.GetIterator() }?;
    let mut has_current = BOOL::default();
    // SAFETY: live iterator, out-pointer to a local of the ABI type.
    unsafe { iterator.HasCurrent(&raw mut has_current) }?;
    while has_current.as_bool() {
        // SAFETY: `HasCurrent` just reported an element.
        let frame: ICoreWebView2FrameInfo2 = unsafe { iterator.GetCurrent() }?.cast()?;
        let mut id = 0u32;
        // SAFETY: live interface, out-pointer to a local of the ABI type.
        unsafe { frame.FrameId(&raw mut id) }?;
        let is_wanted_frame = id == frame_id;
        if is_wanted_frame {
            return Ok(true);
        }
        // SAFETY: live iterator, out-pointer to a local of the ABI type.
        unsafe { iterator.MoveNext(&raw mut has_current) }?;
    }
    Ok(false)
}
