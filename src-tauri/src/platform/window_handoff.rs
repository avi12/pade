//! Handing sessions from one PADE window to another: a live session moved into
//! a new window (shift-click in the left rail), every session of a window
//! merged into another before it closes, or a conversation resumed in a new
//! window.
//!
//! A hand-off is two things that must travel together: the live PTYs change
//! owner (`pty::transfer_sessions`, the processes keep running untouched), and
//! the receiving window learns what it now holds - an opaque payload the
//! frontend wrote and validates on arrival (zod), parked here per receiving
//! window until it takes it. A window that already runs is told by the
//! `window://handoff` event; a window being created takes its parked payloads
//! as it boots, so neither can miss one.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::terminal::pty::{transfer_sessions, PtyState};

/// The event a receiving window takes its parked hand-offs on.
const HANDOFF_EVENT: &str = "window://handoff";
/// A hand-off payload names a few sessions and a conversation; anything larger
/// is a malformed request.
const MAXIMUM_HANDOFF_BYTES: usize = 64 * 1024;
/// How many hand-offs may wait for one window before it takes them.
const MAXIMUM_PARKED_HANDOFFS: usize = 16;

/// Hand-off payloads waiting for their receiving window, keyed by its label.
#[derive(Default, Debug)]
pub struct WindowHandoffs(Mutex<HashMap<String, Vec<String>>>);

/// Move `sessions` from window `from` to window `to` and park `payload` for
/// `to`. The PTYs move first: when they cannot (one is gone, or `to` is full),
/// nothing is parked and the sending window keeps everything.
pub fn hand_off(
    app: &AppHandle,
    from: &str,
    to: &str,
    sessions: &[String],
    payload: String,
) -> Result<(), String> {
    if payload.len() > MAXIMUM_HANDOFF_BYTES {
        return Err("hand-off exceeds its size limit".into());
    }
    let pty_state = app.state::<PtyState>();
    transfer_sessions(pty_state.inner(), from, to, sessions)?;
    let handoffs = app.state::<WindowHandoffs>();
    let mut parked = handoffs.0.lock().map_err(|error| error.to_string())?;
    let waiting = parked.entry(to.to_string()).or_default();
    if waiting.len() >= MAXIMUM_PARKED_HANDOFFS {
        drop(parked);
        transfer_sessions(pty_state.inner(), to, from, sessions)?;
        return Err("the receiving window has too many hand-offs waiting".into());
    }
    waiting.push(payload);
    Ok(())
}

/// Give back sessions a hand-off moved to a window that never came to be, so
/// the sending window still owns them.
pub fn undo_hand_off(app: &AppHandle, from: &str, to: &str, sessions: &[String]) {
    let pty_state = app.state::<PtyState>();
    if let Err(error) = transfer_sessions(pty_state.inner(), to, from, sessions) {
        eprintln!("[pade] could not return sessions after a failed hand-off: {error}");
    }
    forget(app, to);
}

/// Hand the calling window's `sessions` to the open window `to`, focus it, and
/// tell it to take them. False when `to` is not an open window.
#[tauri::command]
pub fn window_hand_off(
    app: AppHandle,
    window: WebviewWindow,
    to: String,
    sessions: Vec<String>,
    payload: String,
) -> Result<bool, String> {
    let is_other_open_window = to != window.label() && app.get_webview_window(&to).is_some();
    if !is_other_open_window {
        return Ok(false);
    }
    hand_off(&app, window.label(), &to, &sessions, payload)?;
    let _ = app.emit_to(to.as_str(), HANDOFF_EVENT, ());
    if let Some(target) = app.get_webview_window(&to) {
        let _ = target.unminimize();
        let _ = target.set_focus();
    }
    Ok(true)
}

/// The hand-offs parked for the calling window, removed as they are taken.
#[tauri::command]
pub fn window_take_handoffs(window: WebviewWindow, state: State<WindowHandoffs>) -> Vec<String> {
    state
        .0
        .lock()
        .ok()
        .and_then(|mut parked| parked.remove(window.label()))
        .unwrap_or_default()
}

/// Drop whatever still waits for a window that closed.
pub fn forget(app: &AppHandle, label: &str) {
    if let Ok(mut parked) = app.state::<WindowHandoffs>().0.lock() {
        parked.remove(label);
    }
}

pub fn init(app: &AppHandle) {
    app.manage(WindowHandoffs::default());
}
