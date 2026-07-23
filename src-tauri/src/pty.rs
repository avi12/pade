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
pub struct PtyState(pub Mutex<HashMap<String, Arc<Pty>>>);

pub struct Pty {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    /// The agent process itself. Held so that ending a session can end the
    /// process — see `Drop`.
    child: Mutex<Box<dyn Child + Send + Sync>>,
    /// The command the spawn asked for (the agent's canonical name, e.g.
    /// `claude`) — `None` when the spawn fell through to the default shell.
    /// Kept so `pty_list` can tell a reloaded frontend what each session runs.
    command: Option<String>,
    /// The directory the session was spawned into — its workspace mapping.
    cwd: Option<String>,
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
    /// Ending a session has to end its whole process *tree*. Dropping the PTY only
    /// hangs the child up, and a shell (or an agent mid-task) can outlive that.
    /// Worse, on Windows `Child::kill` terminates the immediate child alone, so
    /// the agent's own descendants — the shell it runs under, a `node`, a tool it
    /// spawned — survive and keep the working directory locked, and the workspace
    /// can then be neither deleted nor moved ("used by another process", os error
    /// 32). Kill the tree first (while it's still walkable from the child), then
    /// reap the immediate child, so the folder is free the moment the session ends.
    fn drop(&mut self) {
        let child = match self.child.get_mut() {
            Ok(child) => child,
            Err(poisoned) => poisoned.into_inner(),
        };
        #[cfg(windows)]
        if let Some(pid) = child.process_id() {
            kill_process_tree(pid);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Terminate a process and every descendant it spawned. `Child::kill` reaches
/// only the immediate child, so an agent's grandchildren live on and hold the
/// workspace cwd; `taskkill /T` walks the tree from `pid` and ends them all, `/F`
/// forces it. Best-effort — a process already gone just makes this a no-op.
/// Shelled out through `util::command` so it inherits `CREATE_NO_WINDOW`.
#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    let _ = crate::util::command("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .output();
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
    /// Backing storage for the replay tail. `start` advances through this string
    /// as old output expires; compacting only occasionally avoids moving a near-
    /// 512 KiB tail on every PTY chunk once a session has been running for a while.
    data: String,
    start: usize,
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

/// How often each session's reaper checks whether its agent process has exited
/// on its own (see the reaper thread in `pty_spawn`).
const CHILD_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

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
    let mut window_start = buffer.data.len().saturating_sub(lookback).max(buffer.start);
    while !buffer.data.is_char_boundary(window_start) {
        window_start += 1;
    }
    if let Some(alternate) = last_screen_switch(&buffer.data[window_start..]) {
        buffer.alternate = alternate;
    }

    if buffer.data.len() - buffer.start > HISTORY_CAP {
        // Walk the cap overflow to a char boundary BEFORE slicing — landing inside
        // a multibyte character would panic — then cut just past the first line
        // break so a replay starts on a whole line. A tail with no line break at
        // all is dropped entirely rather than replayed torn.
        let mut overflow = buffer.start + (buffer.data.len() - buffer.start - HISTORY_CAP);
        while !buffer.data.is_char_boundary(overflow) {
            overflow += 1;
        }
        let cut = buffer.data[overflow..]
            .find('\n')
            .map_or(buffer.data.len(), |line_break| overflow + line_break + 1);
        buffer.start = cut;

        // `String::drain` moves the retained tail. Do that only after a full
        // history window has expired, not once per output chunk.
        if buffer.start >= HISTORY_CAP {
            let start = buffer.start;
            buffer.data.drain(..start);
            buffer.start = 0;
        }
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
fn build_command(
    command: Option<String>,
    scheme: Option<crate::theming::Scheme>,
    conversation_id: Option<&str>,
) -> CommandBuilder {
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

    // The agent's leading arguments, in the order they must appear; caller-supplied
    // args (e.g. a terminal editor's project path) are appended by `pty_spawn`
    // after this. Assembled up front so the same list feeds either a direct spawn
    // or the Windows light-console wrapper below.
    let mut leading_args: Vec<String> = Vec::new();
    // Launch the agent in its skip-every-permission ("yolo") mode so ADE drives it
    // autonomously — no per-tool/edit approval stops. Empty for a shell or an
    // unknown command.
    for arg in crate::agents::session_args(&program) {
        leading_args.push(arg.to_string());
    }
    // Per-scheme theme arguments for arg-themed CLIs (codex's global
    // `-c tui.theme=…`), so the agent's TUI starts on ADE's current appearance —
    // see theming.rs. Interactive spawn path only; the oneshot `codex exec` renders
    // no TUI. A global option, so its order among the flags doesn't matter. Empty
    // for a shell, an unknown command, or an agent themed by file/env instead.
    if let Some(scheme) = scheme {
        for arg in crate::theming::spawn_args(&program, scheme) {
            leading_args.push(arg.to_string());
        }
    }
    // Bind the session to ADE's stable conversation id (`--session-id <uuid>`), so a
    // later spawn with the same id resumes THIS conversation rather than the most
    // recent — how ADE restarts one specific session (e.g. after `.mcp.json`
    // changed). Only for an agent whose CLI supports it; a shell/editor ignores it.
    if let (Some(flag), Some(conversation_id)) =
        (crate::agents::session_id_flag(&program), conversation_id)
    {
        leading_args.push(flag.to_string());
        leading_args.push(conversation_id.to_string());
    }

    // Windows + light scheme only: some agents (Codex) paint their input composer
    // box from the *detected* terminal background, which under ConPTY is the
    // pseudoconsole's hard-coded BLACK buffer no env/arg/config/OSC channel can
    // change (see agents.rs). The one lever is a child-side console write, so ADE
    // prefixes `cmd /c color F0 & …` to flip the buffer light before the agent
    // probes it. Applied at spawn: a later scheme flip won't recolor an
    // already-running box (acceptable — every spawn-time theme signal works this
    // way). Every other case (dark scheme, non-Windows, agents that don't need it)
    // spawns the agent unchanged.
    let force_light_console = cfg!(target_os = "windows")
        && scheme == Some(crate::theming::Scheme::Light)
        && crate::agents::needs_light_console_fix(&program);
    let mut cmd = if force_light_console {
        light_console_command(&exe, &leading_args)
    } else {
        let mut cmd = CommandBuilder::new(&exe);
        for arg in &leading_args {
            cmd.arg(arg);
        }
        cmd
    };

    // Environment goes on whichever process ADE spawns; a `cmd.exe` wrapper passes
    // it through to the agent it launches. Keyed by the command ADE knows (`codex`),
    // not the file it resolved to (`…\codex-x86_64-pc-windows-msvc.exe`).
    for (key, value) in crate::agents::spawn_env(&program) {
        cmd.env(key, value);
    }
    // Per-scheme theme environment for env-themed CLIs (aider, cursor-agent), so the
    // agent starts matching ADE's current appearance — see theming.rs.
    if let Some(scheme) = scheme {
        for (key, value) in crate::theming::spawn_env(&program, scheme) {
            cmd.env(key, value);
        }
        // …and the tui-config file pair for file-themed CLIs (opencode):
        // theming.rs materializes the config on disk and returns the env pair
        // pointing at it. Filesystem I/O happens here, before any session lock.
        for (key, value) in crate::theming::spawn_tui_config_env(&program, scheme) {
            cmd.env(key, value);
        }
    }
    // ADE's terminal renders OSC 8 hyperlinks (xterm + linkHandler), but a CLI can't
    // tell from inside a bare ConPTY: Ink/terminal-link probe the environment
    // (TERM_PROGRAM, VTE version) and silently fall back to plain text when nothing
    // matches. This is the escape hatch those probes honor.
    cmd.env("FORCE_HYPERLINK", "1");
    cmd
}

/// Wrap `exe` + `args` as `cmd /c color F0 & <exe> <args…>` so the `ConPTY` console
/// buffer reads as a light background *before* the launched agent probes it — ADE's
/// Windows fix for Codex's terminal-background-derived composer box (see
/// `build_command` and `agents::needs_light_console_fix`). `color F0` sets the
/// buffer to black-on-white, which `GetConsoleScreenBufferInfoEx` (the probe's
/// Windows fallback) then reports as light.
///
/// Each token is a *separate* argv entry on purpose: portable-pty quotes each with
/// standard Windows (`CommandLineToArgvW`) rules, which for tokens without embedded
/// quotes is exactly what `cmd.exe` parses — a spaced exe path becomes one `"…"`
/// token, and the bare `&` stays cmd's command separator. Building one pre-quoted
/// shell string instead would be double-escaped by portable-pty and mis-parsed by
/// cmd.
fn light_console_command(exe: &str, args: &[String]) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("cmd.exe");
    for token in ["/c", "color", "F0", "&", exe] {
        cmd.arg(token);
    }
    for arg in args {
        cmd.arg(arg);
    }
    cmd
}

// A PTY spawn is inherently wide (id, command, args, cwd, dimensions) and two of
// these are Tauri-injected (app, state), not caller-supplied — grouping them
// would only obscure the IPC shape the frontend sends.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn pty_spawn(
    app: AppHandle,
    state: State<'_, PtyState>,
    id: String,
    command: Option<String>,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    // Extra arguments for `command` (e.g. the project path for a terminal
    // editor). Empty/absent for a plain agent or shell session.
    args: Option<Vec<String>>,
    // ADE's resolved appearance at spawn time, for env-themed CLIs (theming.rs).
    scheme: Option<crate::theming::Scheme>,
    // A stable conversation id to pin/resume this session to, for an agent whose
    // CLI supports it (`claude --session-id`). Absent for a shell/editor.
    conversation_id: Option<String>,
) -> Result<(), String> {
    {
        let sessions = state.0.lock().map_err(|e| e.to_string())?;
        if sessions.contains_key(&id) {
            return Ok(()); // already running
        }
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

    // An explicit cwd (e.g. a per-branch worktree) overrides the process dir.
    let dir = cwd
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok());
    let spawn_cwd = dir.as_ref().map(|path| path.to_string_lossy().into_owned());

    // Remembered on the session so `pty_list` can report what it runs and where
    // — the roster a reloaded frontend re-attaches its panes against.
    let spawn_command = command.clone();
    let mut cmd = build_command(command, scheme, conversation_id.as_deref());
    for arg in args.unwrap_or_default() {
        cmd.arg(arg);
    }
    if let Some(dir) = dir {
        cmd.cwd(dir);
    }

    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);

    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let transcript = Arc::new(Mutex::new(String::new()));
    let history = Arc::new(Mutex::new(History::default()));
    let pty = Arc::new(Pty {
        master: Mutex::new(pair.master),
        writer: Mutex::new(writer),
        child: Mutex::new(child),
        command: spawn_command,
        cwd: spawn_cwd,
        transcript: Arc::clone(&transcript),
        history: Arc::clone(&history),
    });

    // Opening a PTY and spawning an agent can block on the operating system. Only
    // take the registry lock to publish the completed session, so existing panes
    // remain responsive while a new CLI starts. A concurrent duplicate is dropped
    // here, which terminates the unregistered child through `Pty::drop`.
    let mut sessions = state.0.lock().map_err(|e| e.to_string())?;
    if sessions.contains_key(&id) {
        drop(sessions);
        drop(pty);
        return Ok(());
    }
    sessions.insert(id.clone(), pty);
    drop(sessions);

    spawn_session_output_pump(app.clone(), id.clone(), reader, transcript, history);

    // Reap the agent when it exits on its own. The reader thread's EOF alone
    // can't be relied on for that: on Windows the ConPTY host (conhost) keeps
    // the master pipe open after the child dies, so the read blocks forever, no
    // exit ever reaches the frontend, and the dead session's conhost lingers.
    // Poll the child instead; once it has exited, drop the session — that closes
    // the ConPTY, which EOFs the reader, whose existing end-of-pump emit is the
    // one authoritative source of `pty://exit` (DRY). A hand-closed session
    // (`pty_kill`) is already out of the map, which ends this thread too.
    spawn_session_exit_monitor(app, id);
    Ok(())
}

/// Pump one session's output after it has been published in the registry. A chunk
/// enters replay history before its event is emitted, so attach/replay handover
/// remains exact even when the frontend starts listening mid-stream.
fn spawn_session_output_pump(
    app: AppHandle,
    id: String,
    mut reader: Box<dyn Read + Send>,
    transcript: Arc<Mutex<String>>,
    history: Arc<Mutex<History>>,
) {
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        let mut pending = Vec::new();
        loop {
            match reader.read(&mut buffer) {
                Ok(bytes_read) if bytes_read > 0 => {
                    pending.extend_from_slice(&buffer[..bytes_read]);
                    let data = drain_decoded(&mut pending);
                    if data.is_empty() {
                        continue;
                    }
                    append_transcript(&transcript, &strip_ansi(&data));
                    let seq = append_history(&history, &data);
                    let _ = app.emit(
                        "pty://data",
                        Chunk {
                            id: id.clone(),
                            data,
                            seq,
                        },
                    );
                }
                // EOF (agent exited) or a read error — end the pump.
                _ => break,
            }
        }
        let _ = app.emit("pty://exit", Exit { id });
    });
}

/// Poll one child process and remove precisely that session when it exits. The
/// registry lock only resolves the session `Arc`; process polling and destruction
/// happen outside it, so a dead process can never stall input for another pane.
fn spawn_session_exit_monitor(app: AppHandle, id: String) {
    std::thread::spawn(move || loop {
        std::thread::sleep(CHILD_POLL_INTERVAL);
        let Some(state) = app.try_state::<PtyState>() else {
            break;
        };
        let pty = {
            let Ok(sessions) = state.0.lock() else {
                break;
            };
            sessions.get(&id).cloned()
        };
        let Some(pty) = pty else {
            break;
        };
        let still_running = pty
            .child
            .lock()
            .is_ok_and(|mut child| matches!(child.try_wait(), Ok(None)));
        if !still_running {
            // A caller can kill and immediately recreate an id. Only remove the
            // exact PTY this reaper owns; a stale reaper must not reap its successor.
            let removed = {
                let Ok(mut sessions) = state.0.lock() else {
                    break;
                };
                if sessions
                    .get(&id)
                    .is_some_and(|active| Arc::ptr_eq(active, &pty))
                {
                    sessions.remove(&id)
                } else {
                    None
                }
            };
            drop(removed);
            break;
        }
    });
}

/// One live PTY session as `pty_list` reports it: what it runs (`command` — the
/// spawn request's agent name, `None` for the default-shell fallback) and where
/// (`cwd`). Deliberately no idleness signal: the frontend's session-status
/// store (the output-quiet detector) is the one authority on idle, and a second
/// wire-level gauge alongside it could only drift from the first.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    id: String,
    command: Option<String>,
    cwd: Option<String>,
}

/// Every session the backend still hosts — the roster a reloaded frontend
/// intersects its persisted pane mapping with to re-attach instead of booting
/// blind (the accidental-reload incident: live PTYs kept running invisibly with
/// no way back). A listed session is alive by construction: the reaper drops a
/// dead one from the map. Sorted by id so the order is deterministic.
#[tauri::command]
pub fn pty_list(state: State<PtyState>) -> Vec<SessionInfo> {
    let Ok(sessions) = state.0.lock() else {
        return Vec::new();
    };
    let mut infos: Vec<SessionInfo> = sessions
        .iter()
        .map(|(id, pty)| SessionInfo {
            id: id.clone(),
            command: pty.command.clone(),
            cwd: pty.cwd.clone(),
        })
        .collect();
    infos.sort_by(|a, b| a.id.cmp(&b.id));
    infos
}

/// Everything a terminal needs to paint a session it is attaching to mid-flight,
/// plus the sequence number of the last chunk already inside it — so a frontend that
/// was listening while it asked can tell which chunks it caught are duplicates and
/// which are genuinely new. Empty for an unknown session.
#[tauri::command]
pub fn pty_history(state: State<PtyState>, id: String) -> HistorySnapshot {
    let pty = {
        let Ok(sessions) = state.0.lock() else {
            return HistorySnapshot::default();
        };
        sessions.get(&id).cloned()
    };
    let Some(pty) = pty else {
        return HistorySnapshot::default();
    };
    pty.history
        .lock()
        .map(|buffer| HistorySnapshot {
            data: buffer.data[buffer.start..].to_string(),
            seq: buffer.seq,
            alternate: buffer.alternate,
        })
        .unwrap_or_default()
}

/// Read a session's current transcript, or an empty string if it is unknown or
/// the lock is poisoned. The context the AI session-namer summarises.
pub fn transcript_of(state: &PtyState, id: &str) -> String {
    let pty = {
        let Ok(sessions) = state.0.lock() else {
            return String::new();
        };
        sessions.get(id).cloned()
    };
    let Some(pty) = pty else {
        return String::new();
    };
    pty.transcript
        .lock()
        .map(|buffer| buffer.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn pty_write(state: State<'_, PtyState>, id: String, data: String) -> Result<(), String> {
    let pty = state.0.lock().map_err(|e| e.to_string())?.get(&id).cloned();
    if let Some(pty) = pty {
        let mut writer = pty.writer.lock().map_err(|e| e.to_string())?;
        writer
            .write_all(data.as_bytes())
            .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn pty_resize(
    state: State<'_, PtyState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let pty = state.0.lock().map_err(|e| e.to_string())?.get(&id).cloned();
    if let Some(pty) = pty {
        pty.master
            .lock()
            .map_err(|e| e.to_string())?
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
pub async fn pty_kill(state: State<'_, PtyState>, id: String) -> Result<(), String> {
    let mut sessions = state.0.lock().map_err(|e| e.to_string())?;
    let pty = sessions.remove(&id);
    drop(sessions);
    drop(pty);
    Ok(())
}

/// Terminate every live session — called as the app exits so no agent lingers
/// after the window is gone and every workspace cwd-lock is freed.
pub fn kill_all(state: &PtyState) {
    if let Ok(mut sessions) = state.0.lock() {
        let all = std::mem::take(&mut *sessions);
        drop(sessions);
        drop(all);
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
        (buffer.data[buffer.start..].to_string(), buffer.alternate)
    }

    /// The Windows light-console wrapper must lead with `cmd /c color F0 &`, keep
    /// the agent's absolute path as ONE argv token even with spaces, and preserve
    /// every following arg verbatim — this is the quoting-risk surface the fix
    /// rides on.
    #[test]
    fn light_console_command_round_trips_the_path_and_args() {
        let exe = r"C:\Program Files\codex dir\codex.exe";
        let args = vec![
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
            "-c".to_string(),
            "tui.theme=catppuccin-latte".to_string(),
        ];
        let built = light_console_command(exe, &args);
        let assembled: Vec<String> = built
            .get_argv()
            .iter()
            .map(|token| token.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            assembled,
            vec![
                "cmd.exe".to_string(),
                "/c".to_string(),
                "color".to_string(),
                "F0".to_string(),
                "&".to_string(),
                exe.to_string(),
                "--dangerously-bypass-approvals-and-sandbox".to_string(),
                "-c".to_string(),
                "tui.theme=catppuccin-latte".to_string(),
            ]
        );
        // The spaced path is a single token (portable-pty quotes it at spawn), not
        // split — the whole point of separate argv entries.
        assert!(assembled.contains(&exe.to_string()));
    }

    /// Only Codex opts into the console workaround; agents themed by file/env and
    /// unknown commands do not.
    #[test]
    fn only_codex_needs_the_light_console_fix() {
        assert!(crate::agents::needs_light_console_fix("codex"));
        assert!(!crate::agents::needs_light_console_fix("claude"));
        assert!(!crate::agents::needs_light_console_fix("aider"));
        assert!(!crate::agents::needs_light_console_fix("powershell.exe"));
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
