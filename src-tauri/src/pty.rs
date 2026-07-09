//! PTY host: runs the agent CLI unmodified in a real pseudo-terminal.
//!
//! ToS-clean by construction — the ADE only reads the PTY's output stream and
//! writes the user's keystrokes back in. It never scripts around the agent.

use std::io::{Read, Write};
use std::sync::Mutex;

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tauri::{AppHandle, Emitter, Manager, State};

/// Live PTY handles. `None` until `pty_spawn` is called.
#[derive(Default)]
pub struct PtyState(pub Mutex<Option<Pty>>);

pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
}

/// The command to run in the terminal. Defaults to the platform shell so the
/// window is useful even before `claude` is on PATH; override with ADE_AGENT_CMD
/// (e.g. `claude`) to launch the agent directly.
fn agent_command() -> CommandBuilder {
    if let Ok(cmd) = std::env::var("ADE_AGENT_CMD") {
        return CommandBuilder::new(cmd);
    }
    #[cfg(windows)]
    {
        CommandBuilder::new("powershell.exe")
    }
    #[cfg(not(windows))]
    {
        CommandBuilder::new(std::env::var("SHELL").unwrap_or_else(|_| "bash".into()))
    }
}

#[tauri::command]
pub fn pty_spawn(
    app: AppHandle,
    state: State<PtyState>,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Ok(()); // already running
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;

    let mut cmd = agent_command();
    // Start the agent in the directory the ADE was opened on.
    if let Ok(cwd) = std::env::current_dir() {
        cmd.cwd(cwd);
    }

    let _child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    // Pump PTY output to the frontend terminal.
    let app_reader = app.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF — agent exited
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_reader.emit("pty://data", chunk);
                }
                Err(_) => break,
            }
        }
        let _ = app_reader.emit("pty://exit", ());
    });

    *guard = Some(Pty { master: pair.master, writer });
    Ok(())
}

#[tauri::command]
pub fn pty_write(state: State<PtyState>, data: String) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(pty) = guard.as_mut() {
        pty.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        pty.writer.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn pty_resize(state: State<PtyState>, cols: u16, rows: u16) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(pty) = guard.as_ref() {
        pty.master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Ensure a `PtyState` exists on the app.
pub fn init(app: &AppHandle) {
    app.manage(PtyState::default());
}
