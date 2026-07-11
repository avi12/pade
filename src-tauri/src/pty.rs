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

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// All live PTY sessions, keyed by session id.
#[derive(Default)]
pub struct PtyState(pub Mutex<HashMap<String, Pty>>);

pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    /// Rolling, ANSI-stripped tail of this session's output — the context the
    /// AI session-namer reads. Its own lock so the read thread never contends
    /// with the sessions map.
    transcript: Arc<Mutex<String>>,
}

/// How much recent output to keep per session for naming (bytes, tail-trimmed).
const TRANSCRIPT_CAP: usize = 16 * 1024;

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

#[derive(Serialize, Clone)]
struct Chunk {
    id: String,
    data: String,
}

#[derive(Serialize, Clone)]
struct Exit {
    id: String,
}

/// Resolve the program to run: the explicit command, else `ADE_AGENT_CMD`, else
/// the platform shell (so a session is always launchable).
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
    CommandBuilder::new(program)
}

#[tauri::command]
pub fn pty_spawn(
    app: AppHandle,
    state: State<PtyState>,
    id: String,
    command: Option<String>,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
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
    // An explicit cwd (e.g. a per-branch worktree) overrides the process dir.
    let dir = cwd
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok());
    if let Some(dir) = dir {
        cmd.cwd(dir);
    }

    let _child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    // Pump this session's output to the frontend, tagged with its id, while
    // mirroring a cleaned tail into the transcript for the AI namer.
    let app_reader = app.clone();
    let session_id = id.clone();
    let transcript = Arc::new(Mutex::new(String::new()));
    let transcript_reader = transcript.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(n) if n > 0 => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    append_transcript(&transcript_reader, &strip_ansi(&data));
                    let _ = app_reader.emit(
                        "pty://data",
                        Chunk {
                            id: session_id.clone(),
                            data,
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
            transcript,
        },
    );
    Ok(())
}

/// The rolling, ANSI-stripped transcript for a session — the context the AI
/// session-namer summarises. Empty string if the session is unknown.
#[tauri::command]
pub fn session_transcript(state: State<PtyState>, id: String) -> String {
    transcript_of(&state, &id)
}

/// Read a session's current transcript, or an empty string if it is unknown or
/// the lock is poisoned. Shared by the command and the session-namer.
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

/// Close a session and release its PTY (dropping the writer ends the child).
#[tauri::command]
pub fn pty_kill(state: State<PtyState>, id: String) -> Result<(), String> {
    let mut sessions = state.0.lock().map_err(|e| e.to_string())?;
    sessions.remove(&id);
    Ok(())
}

pub fn init(app: &AppHandle) {
    app.manage(PtyState::default());
}
