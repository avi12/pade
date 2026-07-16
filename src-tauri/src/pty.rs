//! PTY host: runs agent CLIs unmodified in real pseudo-terminals.
//!
//! ToS-clean by construction — the ADE only reads a PTY's output stream and
//! writes the user's keystrokes back in. It never scripts around the agent.
//!
//! Multi-session: each agent runs in its own PTY keyed by a session id, so the
//! user can switch between and combine agents. Events carry the id so the right
//! terminal receives them.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// All live PTY sessions, keyed by session id.
#[derive(Default)]
pub struct PtyState(pub Mutex<HashMap<String, Pty>>);

pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    /// The agent process itself. Held so that ending a session can end the
    /// process — see `Drop`.
    child: Box<dyn Child + Send + Sync>,
    /// Rolling, ANSI-stripped tail of this session's output — the context the
    /// AI session-namer reads. Its own lock so the read thread never contends
    /// with the sessions map.
    transcript: Arc<Mutex<String>>,
    /// Everything the terminal would need to paint this session from scratch.
    /// A PTY has no scrollback of its own: whatever the frontend never received,
    /// or received and then threw away, is gone. So a terminal that attaches to a
    /// session already in flight — a remounted component, a reloaded window, a
    /// reopened tab — has nothing to draw and sits blank while the agent, quite
    /// happily, waits for input. Keeping the raw stream is what lets it catch up.
    history: Arc<Mutex<History>>,
}

impl Drop for Pty {
    /// Ending a session has to end its process. Dropping the PTY only hangs the
    /// child up, and a shell (or an agent mid-task) can outlive that — on Windows
    /// a surviving child keeps its working directory locked, so the workspace it
    /// runs in can then be neither deleted nor moved. Kill it and reap it here,
    /// so the folder is free the moment the session is gone.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A session's replayable output, and how many chunks have been emitted for it.
/// The counter is what makes the handover exact: the frontend can be listening to
/// the live stream while it asks for the history, and `seq` tells it which of the
/// chunks it caught are already inside that history (see `pty_history`).
///
/// `seq` invariants the frontend's splice depends on (Hyrum): 1-based, +1 per
/// *emitted* chunk (empty decodes never reach here), `0` = empty/unknown
/// session, and `data` is the byte-trimmed but seq-complete concatenation of
/// chunks `1..=seq`.
#[derive(Default)]
pub struct History {
    data: String,
    seq: u64,
    /// Is the program currently painting the ALTERNATE screen (a fullscreen TUI, a
    /// pager)? Then this history is a stream of framebuffer edits, not a document,
    /// and replaying a trimmed one paints a torn frame. The frontend needs to know
    /// so it can ask the program to repaint itself instead.
    alternate: bool,
}

/// A terminal control sequence: the Control Sequence Introducer (`ESC [`)
/// followed by the body, composed so the parts are named rather than one
/// opaque escape string.
macro_rules! control_sequence {
    ($body:literal) => {
        concat!("\x1b[", $body)
    };
}

/// DEC private mode 1049 — the alternate screen. A program sets the mode (`h`)
/// to take the screen over and resets it (`l`) to give it back. Wire constants
/// shared with `Terminal.svelte`, which writes the enter sequence when
/// replaying an alternate-screen history — the two sides must change together.
const ENTER_ALTERNATE_SCREEN: &str = control_sequence!("?1049h");
const LEAVE_ALTERNATE_SCREEN: &str = control_sequence!("?1049l");

/// How much recent output to keep per session for naming (bytes, tail-trimmed).
const TRANSCRIPT_CAP: usize = 16 * 1024;

/// How much raw output to keep per session for replay. Bigger than the transcript
/// because this is the unstripped stream — escape codes and repaints included.
const HISTORY_CAP: usize = 512 * 1024;

/// Strip terminal control sequences so the buffered transcript is plain text:
/// drop ESC-introduced CSI/OSC sequences, carriage returns, and other C0
/// control bytes, keeping printable characters, newlines, and tabs.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\u{1b}' => match chars.peek() {
                Some('[') => {
                    chars.next();
                    // CSI: parameters until a final byte in @…~.
                    while let Some(&param) = chars.peek() {
                        chars.next();
                        if ('@'..='~').contains(&param) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    // OSC: until BEL or the ST (ESC \) terminator.
                    while let Some(&param) = chars.peek() {
                        chars.next();
                        if param == '\u{7}' {
                            break;
                        }
                        if param == '\u{1b}' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            },
            '\r' => {}
            '\n' | '\t' => out.push(ch),
            other if u32::from(other) < 0x20 => {}
            other => out.push(other),
        }
    }
    out
}

/// Append cleaned output to a session's transcript, trimming the front to
/// `TRANSCRIPT_CAP` on a char boundary so it never grows unbounded.
fn append_transcript(transcript: &Arc<Mutex<String>>, cleaned: &str) {
    if cleaned.is_empty() {
        return;
    }
    let Ok(mut buffer) = transcript.lock() else {
        return;
    };
    buffer.push_str(cleaned);
    if buffer.len() > TRANSCRIPT_CAP {
        let mut cut = buffer.len() - TRANSCRIPT_CAP;
        while cut < buffer.len() && !buffer.is_char_boundary(cut) {
            cut += 1;
        }
        buffer.drain(..cut);
    }
}

/// The screen the program switched to, judging by the last switch sequence in
/// `window`: `Some(true)` for the alternate screen, `Some(false)` for the
/// normal one, `None` when the window contains no switch at all.
fn last_screen_switch(window: &str) -> Option<bool> {
    let entered = window.rfind(ENTER_ALTERNATE_SCREEN);
    let left = window.rfind(LEAVE_ALTERNATE_SCREEN);
    match (entered, left) {
        (Some(enter), Some(leave)) => Some(enter > leave),
        (Some(_), None) => Some(true),
        (None, Some(_)) => Some(false),
        (None, None) => None,
    }
}

/// Append raw output to a session's replay history and take the sequence number of
/// this chunk. Trimming the front to `HISTORY_CAP` cuts at a line break (and never
/// mid-character), so a replay starts on a whole line rather than halfway through
/// an escape sequence.
fn append_history(history: &Arc<Mutex<History>>, raw: &str) -> u64 {
    let Ok(mut buffer) = history.lock() else {
        return 0;
    };
    buffer.data.push_str(raw);
    buffer.seq += 1;

    // Track which screen the program is on: the last switch wins; no switch keeps
    // the current screen. Judged over a window reaching back past the chunk
    // boundary, so a switch sequence split across two reads still counts once its
    // tail arrives.
    let lookback = raw.len() + ENTER_ALTERNATE_SCREEN.len() - 1;
    let mut window_start = buffer.data.len().saturating_sub(lookback);
    while !buffer.data.is_char_boundary(window_start) {
        window_start += 1;
    }
    if let Some(alternate) = last_screen_switch(&buffer.data[window_start..]) {
        buffer.alternate = alternate;
    }

    if buffer.data.len() > HISTORY_CAP {
        // Walk the cap overflow to a char boundary BEFORE slicing — landing inside
        // a multibyte character would panic — then cut just past the first line
        // break so a replay starts on a whole line. A tail with no line break at
        // all is dropped entirely rather than replayed torn.
        let mut overflow = buffer.data.len() - HISTORY_CAP;
        while !buffer.data.is_char_boundary(overflow) {
            overflow += 1;
        }
        let cut = buffer.data[overflow..]
            .find('\n')
            .map_or(buffer.data.len(), |line_break| overflow + line_break + 1);
        buffer.data.drain(..cut);
    }

    buffer.seq
}

/// Decode the completed UTF-8 prefix of `pending` and consume it, leaving an
/// incomplete trailing character in place — a PTY read can split one across two
/// chunks, and decoding the halves separately would turn both into U+FFFD.
/// Genuinely invalid bytes become one U+FFFD each and are consumed.
fn drain_decoded(pending: &mut Vec<u8>) -> String {
    let mut out = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(text) => {
                out.push_str(text);
                pending.clear();
                return out;
            }
            Err(error) => {
                let valid_end = error.valid_up_to();
                out.push_str(&String::from_utf8_lossy(&pending[..valid_end]));
                match error.error_len() {
                    // The chunk ends mid-character: keep the partial bytes for
                    // the next read to complete.
                    None => {
                        pending.drain(..valid_end);
                        return out;
                    }
                    Some(invalid_len) => {
                        out.push('\u{FFFD}');
                        pending.drain(..valid_end + invalid_len);
                    }
                }
            }
        }
    }
}

#[derive(Serialize, Clone)]
struct Chunk {
    id: String,
    data: String,
    /// This chunk's position in the session's stream — see `History`.
    seq: u64,
}

/// A session's replayable output and the sequence number of the last chunk in it,
/// plus whether the program is currently painting the alternate screen.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HistorySnapshot {
    data: String,
    seq: u64,
    alternate: bool,
}

#[derive(Serialize, Clone)]
struct Exit {
    id: String,
}

/// Resolve the program to run: the explicit command, else `ADE_AGENT_CMD`, else
/// the platform shell (so a session is always launchable), and apply whatever
/// environment that agent needs (the registry knows; this module doesn't).
///
/// The command is exec'd by absolute path when we can find one. Left as a bare
/// name it would be resolved by the PTY against *our* inherited PATH — the same
/// stale copy that hides a freshly-installed agent from detection — so a CLI ADE
/// had just listed as available could fail to start. Unresolvable commands (a task
/// runner, a shell) pass through untouched, for the PTY to resolve as before.
fn build_command(command: Option<String>) -> CommandBuilder {
    let program = command
        .or_else(|| std::env::var("ADE_AGENT_CMD").ok())
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "powershell.exe".into()
            } else {
                std::env::var("SHELL").unwrap_or_else(|_| "bash".into())
            }
        });
    let exe = crate::agents::program(&program).map_or_else(
        || program.clone(),
        |path| path.to_string_lossy().into_owned(),
    );

    let mut cmd = CommandBuilder::new(&exe);
    // Keyed by the command ADE knows (`codex`), not the file it resolved to
    // (`…\codex-x86_64-pc-windows-msvc.exe`).
    for (key, value) in crate::agents::spawn_env(&program) {
        cmd.env(key, value);
    }
    cmd
}

// A PTY spawn is inherently wide (id, command, args, cwd, dimensions) and two of
// these are Tauri-injected (app, state), not caller-supplied — grouping them
// would only obscure the IPC shape the frontend sends.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn pty_spawn(
    app: AppHandle,
    state: State<PtyState>,
    id: String,
    command: Option<String>,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    // Extra arguments for `command` (e.g. the project path for a terminal
    // editor). Empty/absent for a plain agent or shell session.
    args: Option<Vec<String>>,
) -> Result<(), String> {
    let mut sessions = state.0.lock().map_err(|e| e.to_string())?;
    if sessions.contains_key(&id) {
        return Ok(()); // already running
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    let mut cmd = build_command(command);
    for arg in args.unwrap_or_default() {
        cmd.arg(arg);
    }
    // An explicit cwd (e.g. a per-branch worktree) overrides the process dir.
    let dir = cwd
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok());
    if let Some(dir) = dir {
        cmd.cwd(dir);
    }

    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    // Pump this session's output to the frontend, tagged with its id, while keeping
    // the raw stream for replay and mirroring a cleaned tail into the transcript for
    // the AI namer. The chunk is recorded BEFORE it is emitted, so any chunk the
    // frontend can hear is already inside the history it is about to ask for.
    let app_reader = app.clone();
    let session_id = id.clone();
    let transcript = Arc::new(Mutex::new(String::new()));
    let transcript_reader = transcript.clone();
    let history = Arc::new(Mutex::new(History::default()));
    let history_reader = history.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut pending = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(n) if n > 0 => {
                    pending.extend_from_slice(&buf[..n]);
                    let data = drain_decoded(&mut pending);
                    if data.is_empty() {
                        continue;
                    }
                    append_transcript(&transcript_reader, &strip_ansi(&data));
                    let seq = append_history(&history_reader, &data);
                    let _ = app_reader.emit(
                        "pty://data",
                        Chunk {
                            id: session_id.clone(),
                            data,
                            seq,
                        },
                    );
                }
                // EOF (agent exited) or a read error — end the pump.
                _ => break,
            }
        }
        let _ = app_reader.emit(
            "pty://exit",
            Exit {
                id: session_id.clone(),
            },
        );
    });

    sessions.insert(
        id,
        Pty {
            master: pair.master,
            writer,
            child,
            transcript,
            history,
        },
    );
    Ok(())
}

/// Everything a terminal needs to paint a session it is attaching to mid-flight,
/// plus the sequence number of the last chunk already inside it — so a frontend that
/// was listening while it asked can tell which chunks it caught are duplicates and
/// which are genuinely new. Empty for an unknown session.
#[tauri::command]
pub fn pty_history(state: State<PtyState>, id: String) -> HistorySnapshot {
    let Ok(sessions) = state.0.lock() else {
        return HistorySnapshot::default();
    };
    let Some(pty) = sessions.get(&id) else {
        return HistorySnapshot::default();
    };
    pty.history
        .lock()
        .map(|buffer| HistorySnapshot {
            data: buffer.data.clone(),
            seq: buffer.seq,
            alternate: buffer.alternate,
        })
        .unwrap_or_default()
}

/// Read a session's current transcript, or an empty string if it is unknown or
/// the lock is poisoned. The context the AI session-namer summarises.
pub fn transcript_of(state: &PtyState, id: &str) -> String {
    let Ok(sessions) = state.0.lock() else {
        return String::new();
    };
    let Some(pty) = sessions.get(id) else {
        return String::new();
    };
    pty.transcript
        .lock()
        .map(|buffer| buffer.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub fn pty_write(state: State<PtyState>, id: String, data: String) -> Result<(), String> {
    let mut sessions = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(pty) = sessions.get_mut(&id) {
        pty.writer
            .write_all(data.as_bytes())
            .map_err(|e| e.to_string())?;
        pty.writer.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn pty_resize(state: State<PtyState>, id: String, cols: u16, rows: u16) -> Result<(), String> {
    let sessions = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(pty) = sessions.get(&id) {
        pty.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Close a session: dropping it kills and reaps the agent process (see `Pty`'s
/// `Drop`), so by the time this returns the session's cwd is unlocked and its
/// workspace can be deleted or moved.
#[tauri::command]
pub fn pty_kill(state: State<PtyState>, id: String) -> Result<(), String> {
    let mut sessions = state.0.lock().map_err(|e| e.to_string())?;
    sessions.remove(&id);
    Ok(())
}

/// Terminate every live session — called as the app exits so no agent lingers
/// after the window is gone and every workspace cwd-lock is freed.
pub fn kill_all(state: &PtyState) {
    if let Ok(mut sessions) = state.0.lock() {
        sessions.clear();
    }
}

pub fn init(app: &AppHandle) {
    app.manage(PtyState::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_with(chunks: &[&str]) -> (Arc<Mutex<History>>, u64) {
        let history = Arc::new(Mutex::new(History::default()));
        let mut seq = 0;
        for chunk in chunks {
            seq = append_history(&history, chunk);
        }
        (history, seq)
    }

    fn snapshot(history: &Arc<Mutex<History>>) -> (String, bool) {
        let buffer = history.lock().expect("history lock");
        (buffer.data.clone(), buffer.alternate)
    }

    #[test]
    fn entering_the_alternate_screen_sets_the_flag() {
        let (history, _) = history_with(&["hello \x1b[?1049h painting"]);
        assert!(snapshot(&history).1);
    }

    #[test]
    fn leaving_the_alternate_screen_clears_the_flag() {
        let (history, _) = history_with(&["\x1b[?1049h tui", "bye \x1b[?1049l prompt"]);
        assert!(!snapshot(&history).1);
    }

    #[test]
    fn the_last_switch_in_a_chunk_wins() {
        let (history, _) = history_with(&["\x1b[?1049h tui \x1b[?1049l back"]);
        assert!(!snapshot(&history).1);

        let (history, _) = history_with(&["\x1b[?1049l \x1b[?1049h tui"]);
        assert!(snapshot(&history).1);
    }

    #[test]
    fn a_chunk_without_a_switch_keeps_the_current_screen() {
        let (history, _) = history_with(&["\x1b[?1049h tui", "more painting"]);
        assert!(snapshot(&history).1);
    }

    #[test]
    fn a_switch_split_across_two_reads_still_counts() {
        let (enter_head, enter_tail) = ENTER_ALTERNATE_SCREEN.split_at(4);
        let (history, _) = history_with(&[enter_head, enter_tail]);
        assert!(snapshot(&history).1, "split enter sequence must be seen");

        let (leave_head, leave_tail) = LEAVE_ALTERNATE_SCREEN.split_at(5);
        let (history, _) = history_with(&["\x1b[?1049h tui", leave_head, leave_tail]);
        assert!(!snapshot(&history).1, "split leave sequence must be seen");
    }

    #[test]
    fn history_trims_to_the_cap_at_a_line_break() {
        let filler = format!("{}\n", "x".repeat(1023));
        let over_cap = HISTORY_CAP / filler.len() + 2;
        let (history, seq) = history_with(&vec![filler.as_str(); over_cap]);
        let (data, _) = snapshot(&history);
        assert!(data.len() <= HISTORY_CAP);
        assert!(data.starts_with('x'), "replay starts on a whole line");
        assert_eq!(seq, over_cap as u64);
    }

    #[test]
    fn a_tail_with_no_line_break_is_dropped_entirely() {
        let blob = "y".repeat(HISTORY_CAP + 1);
        let (history, _) = history_with(&[blob.as_str()]);
        assert_eq!(snapshot(&history).0, "");
    }

    #[test]
    fn trimming_never_slices_inside_a_multibyte_character() {
        // Overflow the cap so the cut position lands inside the run of
        // multibyte characters — slicing there would panic before the fix.
        let multibyte = "é".repeat(HISTORY_CAP / 2);
        let (history, _) = history_with(&[multibyte.as_str(), multibyte.as_str()]);
        let (data, _) = snapshot(&history);
        assert!(data.len() <= HISTORY_CAP);
    }

    #[test]
    fn drain_decoded_reassembles_a_character_split_across_reads() {
        let bytes = "caf\u{e9} au lait".as_bytes();
        let (head, tail) = bytes.split_at(4); // splits the two-byte é
        let mut pending = Vec::new();

        pending.extend_from_slice(head);
        let first = drain_decoded(&mut pending);
        assert_eq!(first, "caf");
        assert!(!pending.is_empty(), "partial character is held back");

        pending.extend_from_slice(tail);
        let second = drain_decoded(&mut pending);
        assert_eq!(second, "\u{e9} au lait");
        assert!(pending.is_empty());
    }

    #[test]
    fn drain_decoded_replaces_genuinely_invalid_bytes() {
        let mut pending = vec![b'o', b'k', 0xFF, b'!'];
        assert_eq!(drain_decoded(&mut pending), "ok\u{FFFD}!");
        assert!(pending.is_empty());
    }

    #[test]
    fn transcript_trims_on_a_char_boundary() {
        let transcript = Arc::new(Mutex::new(String::new()));
        let multibyte = "日".repeat(TRANSCRIPT_CAP / 3 + 10);
        append_transcript(&transcript, &multibyte);
        let tail = transcript.lock().expect("transcript lock").clone();
        assert!(tail.len() <= TRANSCRIPT_CAP);
        assert!(tail.chars().all(|ch| ch == '日'));
    }
}
