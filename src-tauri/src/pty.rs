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
use std::sync::Mutex;

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// All live PTY sessions, keyed by session id.
#[derive(Default)]
pub struct PtyState(pub Mutex<HashMap<String, Pty>>);

pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
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
    if let Ok(cwd) = std::env::current_dir() {
        cmd.cwd(cwd);
    }

    let _child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    // Pump this session's output to the frontend, tagged with its id.
    let app_reader = app.clone();
    let session_id = id.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(n) if n > 0 => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
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
        },
    );
    Ok(())
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
