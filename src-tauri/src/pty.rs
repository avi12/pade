//! PTY host: runs agent CLIs unmodified in real pseudo-terminals.
//!
//! ToS-clean by construction - the ADE only reads a PTY's output stream and
//! writes the user's keystrokes back in. It never scripts around the agent.
//!
//! Multi-session: each agent runs in its own PTY keyed by a session id, so the
//! user can switch between and combine agents. Events carry the id so the right
//! terminal receives them.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtyPair, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::agents::DEFAULT_SHELL;
use crate::theming::Scheme;
use crate::util::{owner_access, OwnerAccess, MAXIMUM_PATH_BYTES, MAXIMUM_SESSION_ID_BYTES};

const MAXIMUM_SESSIONS_PER_WINDOW: usize = 32;
const MAXIMUM_PTY_DIMENSION: u16 = 1_000;
const MAXIMUM_PTY_INPUT_BYTES: usize = 1024 * 1024;
/// Bounds on what a spawn request may carry, so a malformed IPC call can never
/// hand the OS an unbounded command line (the id and path bounds live in `util`).
const MAXIMUM_SPAWN_ARGUMENTS: usize = 64;
const MAXIMUM_CONVERSATION_ID_BYTES: usize = 256;

/// The error every owner check answers a window that reaches for another
/// window's session with.
const FOREIGN_SESSION_ERROR: &str = "PTY belongs to another window";

/// Overrides the program a session runs when the spawn names none.
const AGENT_COMMAND_OVERRIDE_VARIABLE: &str = "ADE_AGENT_CMD";
/// The login shell outside Windows, when the spawn names no command.
const UNIX_SHELL_VARIABLE: &str = "SHELL";

/// All live PTY sessions, keyed by session id.
#[derive(Default)]
pub struct PtyState(pub Mutex<HashMap<String, Arc<Pty>>>);

pub struct Pty {
    /// The window allowed to re-adopt and control this session.
    owner: String,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    /// The agent process itself. Held so that ending a session can end the
    /// process - see `Drop`.
    child: Mutex<Box<dyn Child + Send + Sync>>,
    /// The command the spawn asked for (the agent's canonical name, e.g.
    /// `claude`) - `None` when the spawn fell through to the default shell.
    /// Kept so `pty_list` can tell a reloaded frontend what each session runs.
    command: Option<String>,
    /// The directory the session was spawned into - its workspace mapping.
    cwd: Option<String>,
    /// Rolling, ANSI-stripped tail of this session's output - the context the
    /// AI session-namer reads. Its own lock so the read thread never contends
    /// with the sessions map.
    transcript: Arc<Mutex<String>>,
    /// Everything the terminal would need to paint this session from scratch.
    /// A PTY has no scrollback of its own: whatever the frontend never received,
    /// or received and then threw away, is gone. So a terminal that attaches to a
    /// session already in flight - a remounted component, a reloaded window, a
    /// reopened tab - has nothing to draw and sits blank while the agent, quite
    /// happily, waits for input. Keeping the raw stream is what lets it catch up.
    history: Arc<Mutex<History>>,
}

impl Drop for Pty {
    /// Ending a session has to end its whole process *tree*. Dropping the PTY only
    /// hangs the child up, and a shell (or an agent mid-task) can outlive that.
    /// Worse, on Windows `Child::kill` terminates the immediate child alone, so
    /// the agent's own descendants - the shell it runs under, a `node`, a tool it
    /// spawned - survive and keep the working directory locked, and the workspace
    /// can then be neither deleted nor moved ("used by another process", os error
    /// 32). Kill the tree first (while it's still walkable from the child), then
    /// reap the immediate child, so the folder is free the moment the session ends.
    fn drop(&mut self) {
        let child = self
            .child
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        #[cfg(windows)]
        if let Some(process_id) = child.process_id() {
            kill_process_tree(process_id);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Terminate a process and every descendant it spawned. `Child::kill` reaches
/// only the immediate child, so an agent's grandchildren live on and hold the
/// workspace cwd; `taskkill /T` walks the tree from `process_id` and ends them all, `/F`
/// forces it. Best-effort - a process already gone just makes this a no-op.
/// Shelled out through `util::command` so it inherits `CREATE_NO_WINDOW`.
#[cfg(windows)]
fn kill_process_tree(process_id: u32) {
    let _ = crate::util::command("taskkill")
        .args(["/F", "/T", "/PID", &process_id.to_string()])
        .output();
}

/// One running process from a system snapshot: its id, its parent's id, and the
/// command line it was launched with - the minimum needed to walk the tree and
/// recognise the task an agent started.
#[derive(Debug, Clone)]
struct ProcessInfo {
    process_id: u32,
    parent_process_id: u32,
    command_line: String,
}

/// Collapse every run of whitespace to one space, so a command matches regardless
/// of the shell's own reformatting of the line it launched.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A shell separator or quote - anything that is not part of a command token.
fn is_command_boundary(character: char) -> bool {
    matches!(
        character,
        ' ' | '"' | '\'' | '&' | '|' | ';' | '(' | ')' | '<' | '>'
    )
}

/// Whether `command_line` runs `command` as a whole token run - bounded on both
/// sides by a shell separator. A bounded substring match, so it fires when the
/// shell wraps the command (`bash -c "pnpm dev"`, a `cd … &&` prefix, a redirect)
/// yet never mistakes `pnpm dev` for a run of `pnpm dev:prod`.
fn command_matches(command_line: &str, command: &str) -> bool {
    let haystack = collapse_whitespace(command_line);
    let needle = collapse_whitespace(command);
    if needle.is_empty() {
        return false;
    }

    haystack
        .match_indices(needle.as_str())
        .any(|(start, matched)| {
            let end = start + matched.len();
            let is_bounded_before = haystack[..start]
                .chars()
                .next_back()
                .is_none_or(is_command_boundary);
            let is_bounded_after = haystack[end..]
                .chars()
                .next()
                .is_none_or(is_command_boundary);
            is_bounded_before && is_bounded_after
        })
}

/// The processes under `root` whose command line runs `command` - the subtree an
/// agent spawned for a known task. Pure over a process snapshot so the tree-walk
/// and match are unit-testable without enumerating the live system. `root` (the
/// agent itself) is never included, so a Stop can never kill the agent.
fn task_processes(processes: &[ProcessInfo], root: u32, command: &str) -> Vec<u32> {
    // Index children by parent once, so the walk is linear in the snapshot
    // rather than a full rescan per visited process.
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for process in processes {
        let is_own_parent = process.process_id == process.parent_process_id;
        if !is_own_parent {
            children
                .entry(process.parent_process_id)
                .or_default()
                .push(process.process_id);
        }
    }

    let mut descendants: HashSet<u32> = HashSet::new();
    let mut frontier = vec![root];
    while let Some(parent) = frontier.pop() {
        for &child in children.get(&parent).into_iter().flatten() {
            if descendants.insert(child) {
                frontier.push(child);
            }
        }
    }

    processes
        .iter()
        .filter(|process| descendants.contains(&process.process_id))
        .filter(|process| command_matches(&process.command_line, command))
        .map(|process| process.process_id)
        .collect()
}

/// One row of the CIM process snapshot. Fields are optional because a process can
/// vanish mid-query and CIM then reports nulls.
#[cfg(windows)]
#[derive(serde::Deserialize)]
struct CimProcess {
    #[serde(rename = "ProcessId")]
    process_id: Option<u32>,
    #[serde(rename = "ParentProcessId")]
    parent_process_id: Option<u32>,
    #[serde(rename = "CommandLine")]
    command_line: Option<String>,
}

/// Snapshot every running process (id, parent, command line) via a single CIM
/// query. Shelled through `util::command` (no console window; not `wmic`, which is
/// deprecated on current Windows) rather than PEB-walking each process by hand.
#[cfg(windows)]
fn snapshot_processes() -> Result<Vec<ProcessInfo>, String> {
    let output = crate::util::command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,CommandLine | ConvertTo-Json -Compress",
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let rows: Vec<CimProcess> =
        serde_json::from_str(text.trim()).map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            row.process_id.map(|process_id| ProcessInfo {
                process_id,
                parent_process_id: row.parent_process_id.unwrap_or_default(),
                command_line: row.command_line.unwrap_or_default(),
            })
        })
        .collect())
}

/// Find and kill the subtree(s) an agent spawned for `command`, anywhere under its
/// process `root`. Returns whether anything matched. Windows only - the snapshot
/// and the tree-kill both go through Windows tooling.
#[cfg(windows)]
fn stop_task_tree(root: u32, command: &str) -> Result<bool, String> {
    let processes = snapshot_processes()?;
    let targets = task_processes(&processes, root, command);
    for process_id in &targets {
        kill_process_tree(*process_id);
    }

    Ok(!targets.is_empty())
}

#[cfg(not(windows))]
fn stop_task_tree(_root: u32, _command: &str) -> Result<bool, String> {
    Ok(false)
}

/// Whether any process under `root` still runs `command`. Snapshot-and-match with
/// no kill - the read half of [`stop_task_tree`], used to poll an attached task's
/// liveness. Windows only.
#[cfg(windows)]
fn task_running(root: u32, command: &str) -> Result<bool, String> {
    let processes = snapshot_processes()?;
    Ok(!task_processes(&processes, root, command).is_empty())
}

#[cfg(not(windows))]
fn task_running(_root: u32, _command: &str) -> Result<bool, String> {
    Ok(false)
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

/// DEC private mode 1049 - the alternate screen. A program sets the mode (`h`)
/// to take the screen over and resets it (`l`) to give it back. Wire constants
/// shared with `Terminal.svelte`, which writes the enter sequence when
/// replaying an alternate-screen history - the two sides must change together.
const ENTER_ALTERNATE_SCREEN: &str = control_sequence!("?1049h");
const LEAVE_ALTERNATE_SCREEN: &str = control_sequence!("?1049l");

/// How much recent output to keep per session for naming (bytes, tail-trimmed).
const TRANSCRIPT_CAPACITY: usize = 16 * 1024;

/// How often each session's reaper checks whether its agent process has exited
/// on its own (see the reaper thread in `pty_spawn`).
const CHILD_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

/// How much raw output to keep per session for replay. Bigger than the transcript
/// because this is the unstripped stream - escape codes and repaints included.
const HISTORY_CAPACITY: usize = 512 * 1024;

/// The event carrying a session's raw output chunk to its owning window. This name
/// is the frontend contract (`src/lib/bridge.ts`) - the two sides must agree.
const PTY_DATA_EVENT: &str = "pty://data";

/// The event announcing a session's process exited (EOF or read error).
const PTY_EXIT_EVENT: &str = "pty://exit";

/// Strip terminal control sequences so the buffered transcript is plain text:
/// drop ESC-introduced CSI/OSC sequences, carriage returns, and other C0
/// control bytes, keeping printable characters, newlines, and tabs.
fn strip_ansi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut characters = input.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            ESCAPE => match characters.next() {
                Some('[') => skip_control_sequence(&mut characters),
                Some(']') => skip_operating_system_command(&mut characters),
                Some(_) | None => {}
            },
            '\r' => {}
            '\n' | '\t' => output.push(character),
            control if control < FIRST_PRINTABLE => {}
            printable => output.push(printable),
        }
    }
    output
}

const ESCAPE: char = '\u{1b}';
const BELL: char = '\u{7}';
/// Everything below the space is a C0 control byte.
const FIRST_PRINTABLE: char = ' ';

/// Skip a CSI body: parameters up to and including a final byte in `@`..=`~`.
fn skip_control_sequence(characters: &mut impl Iterator<Item = char>) {
    for character in characters.by_ref() {
        if ('@'..='~').contains(&character) {
            break;
        }
    }
}

/// Skip an OSC body: up to and including BEL or the ST (`ESC \`) terminator.
fn skip_operating_system_command(characters: &mut std::iter::Peekable<impl Iterator<Item = char>>) {
    while let Some(character) = characters.next() {
        if character == BELL {
            break;
        }
        if character == ESCAPE {
            characters.next_if_eq(&'\\');
            break;
        }
    }
}

/// Append cleaned output to a session's transcript, trimming the front to
/// `TRANSCRIPT_CAPACITY` on a char boundary so it never grows unbounded.
fn append_transcript(transcript: &Arc<Mutex<String>>, cleaned: &str) {
    if cleaned.is_empty() {
        return;
    }
    let Ok(mut buffer) = transcript.lock() else {
        return;
    };
    buffer.push_str(cleaned);
    if buffer.len() > TRANSCRIPT_CAPACITY {
        let mut cut = buffer.len() - TRANSCRIPT_CAPACITY;
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
/// this chunk. Trimming the front to `HISTORY_CAPACITY` cuts at a line break (and never
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

    if buffer.data.len() - buffer.start > HISTORY_CAPACITY {
        // Walk the cap overflow to a char boundary BEFORE slicing - landing inside
        // a multibyte character would panic - then cut just past the first line
        // break so a replay starts on a whole line. A tail with no line break at
        // all is dropped entirely rather than replayed torn.
        let mut overflow = buffer.start + (buffer.data.len() - buffer.start - HISTORY_CAPACITY);
        while !buffer.data.is_char_boundary(overflow) {
            overflow += 1;
        }
        let cut = buffer.data[overflow..]
            .find('\n')
            .map_or(buffer.data.len(), |line_break| overflow + line_break + 1);
        buffer.start = cut;

        // `String::drain` moves the retained tail. Do that only after a full
        // history window has expired, not once per output chunk.
        if buffer.start >= HISTORY_CAPACITY {
            let start = buffer.start;
            buffer.data.drain(..start);
            buffer.start = 0;
        }
    }

    buffer.seq
}

/// Decode the completed UTF-8 prefix of `pending` and consume it, leaving an
/// incomplete trailing character in place - a PTY read can split one across two
/// chunks, and decoding the halves separately would turn both into U+FFFD.
/// Genuinely invalid bytes become one U+FFFD each and are consumed.
fn drain_decoded(pending: &mut Vec<u8>) -> String {
    let mut output = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(text) => {
                output.push_str(text);
                pending.clear();
                return output;
            }
            Err(error) => {
                let valid_end = error.valid_up_to();
                output.push_str(&String::from_utf8_lossy(&pending[..valid_end]));
                match error.error_len() {
                    // The chunk ends mid-character: keep the partial bytes for
                    // the next read to complete.
                    None => {
                        pending.drain(..valid_end);
                        return output;
                    }
                    Some(invalid_len) => {
                        output.push('\u{FFFD}');
                        pending.drain(..valid_end + invalid_len);
                    }
                }
            }
        }
    }
}

/// Borrowed, so the pump emits each chunk without copying the session id or
/// the decoded text it already holds.
#[derive(Serialize, Clone)]
struct Chunk<'a> {
    id: &'a str,
    data: &'a str,
    /// This chunk's position in the session's stream - see `History`.
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
/// name it would be resolved by the PTY against *our* inherited PATH - the same
/// stale copy that hides a freshly-installed agent from detection - so a CLI ADE
/// had just listed as available could fail to start. Unresolvable commands (a task
/// runner, a shell) pass through untouched, for the PTY to resolve as before.
fn build_command(
    command: Option<String>,
    scheme: Option<Scheme>,
    conversation_id: Option<&str>,
    model_choice: Option<&crate::running_model::ModelChoice>,
) -> CommandBuilder {
    let program = command
        .or_else(|| std::env::var(AGENT_COMMAND_OVERRIDE_VARIABLE).ok())
        .unwrap_or_else(default_shell);
    let executable = crate::agents::program(&program).map_or_else(
        || program.clone(),
        |path| path.to_string_lossy().into_owned(),
    );

    // The agent's leading arguments, in the order they must appear; caller-supplied
    // args (e.g. a terminal editor's project path) are appended by `pty_spawn`
    // after this. Assembled up front so the same list feeds either a direct spawn
    // or the Windows light-console wrapper.
    let mut leading_arguments = agent_launch_arguments(&program, scheme);
    // The agent reports its model and effort out of band (hooks, a plugin) -
    // see running_model.rs. Its arguments join the list (merging into a
    // `--settings` layer the theme arguments may already carry); its environment
    // is applied with the rest.
    let model_report_environment = crate::running_model::attach(
        &program,
        conversation_id,
        model_choice,
        &mut leading_arguments,
    );
    // Bind the session to ADE's stable conversation id (`--session-id <uuid>`), so a
    // later spawn with the same id resumes THIS conversation rather than the most
    // recent - how ADE restarts one specific session (e.g. after `.mcp.json`
    // changed). Only for an agent whose CLI supports it; a shell/editor ignores it.
    if let (Some(flag), Some(conversation_id)) =
        (crate::agents::session_id_flag(&program), conversation_id)
    {
        leading_arguments.extend(flag.arguments(conversation_id));
    }

    let mut process = launcher(&program, &executable, scheme, &leading_arguments);
    apply_agent_environment(&mut process, &program, scheme, model_report_environment);
    apply_terminal_capabilities(&mut process);
    process
}

/// The platform shell a session runs when nothing names a program: the login
/// `$SHELL` outside Windows, else the registry's default shell.
fn default_shell() -> String {
    if cfg!(windows) {
        return DEFAULT_SHELL.into();
    }
    std::env::var(UNIX_SHELL_VARIABLE).unwrap_or_else(|_| DEFAULT_SHELL.into())
}

/// The arguments every interactive launch of `program` leads with: its
/// skip-every-permission ("yolo") mode, so ADE drives it autonomously with no
/// per-tool/edit approval stops, then the per-scheme theme arguments of an
/// arg-themed CLI (codex's global `-c tui.theme=…`) so its TUI starts on ADE's
/// current appearance (see theming.rs). Interactive spawn path only; the oneshot
/// `codex exec` renders no TUI. Empty for a shell or an unknown command.
fn agent_launch_arguments(program: &str, scheme: Option<Scheme>) -> Vec<String> {
    let theme_arguments = scheme.map_or(&[][..], |scheme| {
        crate::theming::spawn_args(program, scheme)
    });
    crate::agents::session_args(program)
        .iter()
        .chain(theme_arguments)
        .map(ToString::to_string)
        .collect()
}

/// The process to spawn for `executable` + `arguments`.
///
/// Windows + light scheme only: some agents (Codex) paint their input composer
/// box from the *detected* terminal background, which under `ConPTY` is the
/// pseudoconsole's hard-coded BLACK buffer no env/arg/config/OSC channel can
/// change (see agents.rs). The one lever is a child-side console write, so ADE
/// prefixes `cmd /c color F0 & …` to flip the buffer light before the agent
/// probes it. Applied at spawn: a later scheme flip won't recolor an
/// already-running box (acceptable - every spawn-time theme signal works this
/// way). Every other case (dark scheme, non-Windows, agents that don't need it)
/// spawns the agent unchanged.
fn launcher(
    program: &str,
    executable: &str,
    scheme: Option<Scheme>,
    arguments: &[String],
) -> CommandBuilder {
    let is_light_console_forced = cfg!(target_os = "windows")
        && scheme == Some(Scheme::Light)
        && crate::agents::needs_light_console_fix(program);
    if is_light_console_forced {
        return light_console_command(executable, arguments);
    }
    let mut process = CommandBuilder::new(executable);
    process.args(arguments);
    process
}

/// The environment `program` needs, applied to whichever process ADE spawns; a
/// `cmd.exe` wrapper passes it through to the agent it launches. Keyed by the
/// command ADE knows (`codex`), not the file it resolved to
/// (`…\codex-x86_64-pc-windows-msvc.exe`).
fn apply_agent_environment(
    process: &mut CommandBuilder,
    program: &str,
    scheme: Option<Scheme>,
    model_report_environment: Vec<(String, String)>,
) {
    for (key, value) in crate::agents::spawn_env(program) {
        process.env(key, value);
    }
    for (key, value) in model_report_environment {
        process.env(key, value);
    }
    let Some(scheme) = scheme else {
        return;
    };
    // Per-scheme theme environment for env-themed CLIs (aider, cursor-agent), so the
    // agent starts matching ADE's current appearance - see theming.rs.
    for (key, value) in crate::theming::spawn_env(program, scheme) {
        process.env(key, value);
    }
    // …and the tui-config file pair for file-themed CLIs (opencode):
    // theming.rs materializes the config on disk and returns the env pair
    // pointing at it. Filesystem I/O happens here, before any session lock.
    for (key, value) in crate::theming::spawn_tui_config_env(program, scheme) {
        process.env(key, value);
    }
    // …and the live theme definition the launch args are about to select
    // (Claude), written before the process starts so its first frame already
    // paints in ADE's scheme. A flip later rewrites this same file, which is
    // what re-themes the session without restarting it.
    if let Err(error) = crate::theming::publish_live_themes(scheme) {
        eprintln!("theming: could not publish the live theme ({error}); {program} starts on its own default");
    }
}

/// ADE's terminal renders the full 16-colour ANSI palette (see terminal-theme.ts),
/// but a CLI can't tell from inside a bare `ConPTY`: Ink/chalk/supports-color probe
/// for a colour-capable terminal (TERM, COLORTERM, an isatty check) and, finding a
/// Windows pseudoconsole that advertises none of them, fall back to MONOCHROME
/// output - Claude Code's banner and every agent's colour go grey. These two are
/// the escape hatch supports-color honours: `FORCE_COLOR` pins the level
/// regardless of the isatty result, `COLORTERM` names truecolor so 24-bit SGR is
/// emitted.
const COLOR_ENVIRONMENT: [(&str, &str); 2] = [("FORCE_COLOR", "3"), ("COLORTERM", "truecolor")];
/// ADE's terminal renders OSC 8 hyperlinks (xterm + linkHandler), but a CLI can't
/// tell from inside a bare `ConPTY`: Ink/terminal-link probe the environment
/// (`TERM_PROGRAM`, VTE version) and silently fall back to plain text when
/// nothing matches. This is the escape hatch those probes honor.
const HYPERLINK_ENVIRONMENT: (&str, &str) = ("FORCE_HYPERLINK", "1");
/// Claude Code stamps every subprocess it spawns with this marker, and a bare
/// `ConPTY` forwards it verbatim - so when ADE is itself launched from inside a
/// Claude Code session, the agent ADE spawns inherits the marker, reads itself as
/// a nested child, and disables transcript saving (no --resume, no history). An
/// agent ADE launches is always its own top-level session, so ADE clears it.
const CHILD_SESSION_MARKER_VARIABLE: &str = "CLAUDE_CODE_CHILD_SESSION";

/// Tell the CLI what ADE's terminal can render, which it cannot probe through a
/// bare `ConPTY`, and clear the nested-session marker it must not inherit.
fn apply_terminal_capabilities(process: &mut CommandBuilder) {
    for (key, value) in COLOR_ENVIRONMENT {
        process.env(key, value);
    }
    process.env_remove(CHILD_SESSION_MARKER_VARIABLE);
    let (key, value) = HYPERLINK_ENVIRONMENT;
    process.env(key, value);
}

/// Wrap `executable` + `arguments` as `cmd /c color F0 & <executable> <arguments…>` so the `ConPTY` console
/// buffer reads as a light background *before* the launched agent probes it - ADE's
/// Windows fix for Codex's terminal-background-derived composer box (see
/// `build_command` and `agents::needs_light_console_fix`). `color F0` sets the
/// buffer to black-on-white, which `GetConsoleScreenBufferInfoEx` (the probe's
/// Windows fallback) then reports as light.
///
/// Each token is a *separate* argv entry on purpose: portable-pty quotes each with
/// standard Windows (`CommandLineToArgvW`) rules, which for tokens without embedded
/// quotes is exactly what `cmd.exe` parses - a spaced exe path becomes one `"…"`
/// token, and the bare `&` stays cmd's command separator. Building one pre-quoted
/// shell string instead would be double-escaped by portable-pty and mis-parsed by
/// cmd.
fn light_console_command(executable: &str, arguments: &[String]) -> CommandBuilder {
    let mut command = CommandBuilder::new(CONSOLE_HOST);
    command.args(LIGHT_CONSOLE_PREFIX);
    command.arg(executable);
    command.args(arguments);
    command
}

const CONSOLE_HOST: &str = "cmd.exe";
/// `cmd /c color F0 &` - black-on-white, then whatever follows the separator.
const LIGHT_CONSOLE_PREFIX: [&str; 4] = ["/c", "color", "F0", "&"];

fn open_pty(rows: u16, columns: u16) -> Result<PtyPair, String> {
    native_pty_system()
        .openpty(pty_size(rows, columns))
        .map_err(|error| error.to_string())
}

// A PTY spawn is inherently wide (id, command, args, cwd, dimensions) and two of
// these are Tauri-injected (app, state), not caller-supplied - grouping them
// would only obscure the IPC shape the frontend sends.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn pty_spawn(
    app: AppHandle,
    window: WebviewWindow,
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
    scheme: Option<Scheme>,
    // A stable conversation id to pin/resume this session to, for an agent whose
    // CLI supports it (`claude --session-id`). Absent for a shell/editor.
    conversation_id: Option<String>,
    // A model and effort picked from the tab for THIS launch alone (a restart
    // that applies a switch) - passed as launch flags, never saved as the
    // agent's default. See running_model.rs.
    model_choice: Option<crate::running_model::ModelChoice>,
) -> Result<(), String> {
    let owner = window.label().to_string();
    let exceeds_argument_limits = args.as_ref().is_some_and(|arguments| {
        arguments.len() > MAXIMUM_SPAWN_ARGUMENTS
            || arguments
                .iter()
                .any(|argument| argument.len() > MAXIMUM_PATH_BYTES)
    });
    let exceeds_string_limits = id.len() > MAXIMUM_SESSION_ID_BYTES
        || id.chars().any(char::is_control)
        || command
            .as_ref()
            .is_some_and(|value| value.len() > MAXIMUM_PATH_BYTES)
        || cwd
            .as_ref()
            .is_some_and(|value| value.len() > MAXIMUM_PATH_BYTES)
        || conversation_id
            .as_ref()
            .is_some_and(|value| value.len() > MAXIMUM_CONVERSATION_ID_BYTES)
        || model_choice
            .as_ref()
            .is_some_and(|choice| !choice.is_well_formed());
    if exceeds_argument_limits || exceeds_string_limits || !are_valid_dimensions(cols, rows) {
        return Err("PTY request exceeds its input limits".into());
    }
    {
        let sessions = state.0.lock().map_err(|error| error.to_string())?;
        let is_already_live = session_access(&sessions, &id, &owner)?.is_some();
        if is_already_live {
            return Ok(());
        }
        let owner_session_count = sessions.values().filter(|pty| pty.owner == owner).count();
        if owner_session_count >= MAXIMUM_SESSIONS_PER_WINDOW {
            return Err("this window has reached its PTY session limit".into());
        }
    }

    // Opening a PTY, writing theme files and spawning an agent all block on the
    // operating system, so they run on the blocking pool rather than an async
    // worker. Only the registry lock publishes the completed session, so existing
    // panes remain responsive while a new CLI starts.
    let launch = SessionLaunch {
        owner: owner.clone(),
        command,
        cwd,
        columns: cols,
        rows,
        arguments: args.unwrap_or_default(),
        scheme,
        conversation_id,
        model_choice,
    };
    let StartedSession {
        pty,
        reader,
        transcript,
        history,
    } = tauri::async_runtime::spawn_blocking(move || start_session(launch))
        .await
        .map_err(|error| error.to_string())??;

    // A concurrent duplicate is ended here, which terminates the unregistered
    // child through `Pty::drop`.
    let duplicate = {
        let mut sessions = state.0.lock().map_err(|error| error.to_string())?;
        let existing = session_access(&sessions, &id, &owner).map(|found| found.is_some());
        let is_vacant = existing == Ok(false);
        if is_vacant {
            sessions.insert(id.clone(), pty);
            None
        } else {
            Some((pty, existing))
        }
    };
    if let Some((pty, existing)) = duplicate {
        end_session(Some(pty)).await?;
        return existing.map(|_| ());
    }

    spawn_session_output_pump(app.clone(), owner, id.clone(), reader, transcript, history);

    // Reap the agent when it exits on its own. The reader thread's EOF alone
    // can't be relied on for that: on Windows the ConPTY host (conhost) keeps
    // the master pipe open after the child dies, so the read blocks forever, no
    // exit ever reaches the frontend, and the dead session's conhost lingers.
    // Poll the child instead; once it has exited, drop the session - that closes
    // the ConPTY, which EOFs the reader, whose existing end-of-pump emit is the
    // one authoritative source of `pty://exit` (DRY). A hand-closed session
    // (`pty_kill`) is already out of the map, which ends this thread too.
    spawn_session_exit_monitor(app, id);
    Ok(())
}

/// Everything `start_session` needs, moved onto the blocking pool whole.
struct SessionLaunch {
    owner: String,
    command: Option<String>,
    cwd: Option<String>,
    columns: u16,
    rows: u16,
    arguments: Vec<String>,
    scheme: Option<Scheme>,
    conversation_id: Option<String>,
    model_choice: Option<crate::running_model::ModelChoice>,
}

/// A spawned session not yet published, plus the halves its output pump owns.
struct StartedSession {
    pty: Arc<Pty>,
    reader: Box<dyn Read + Send>,
    transcript: Arc<Mutex<String>>,
    history: Arc<Mutex<History>>,
}

/// Open a PTY and spawn the session's program in it. Blocking: run it off the
/// async workers.
fn start_session(launch: SessionLaunch) -> Result<StartedSession, String> {
    let pair = open_pty(launch.rows, launch.columns)?;

    // An explicit cwd (e.g. a per-branch worktree) overrides the process dir.
    let directory = launch
        .cwd
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok());
    let spawn_cwd = directory
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());

    // Remembered on the session so `pty_list` can report what it runs and where
    // - the roster a reloaded frontend re-attaches its panes against.
    let spawn_command = launch.command.clone();
    let mut process = build_command(
        launch.command,
        launch.scheme,
        launch.conversation_id.as_deref(),
        launch.model_choice.as_ref(),
    );
    process.args(launch.arguments);
    if let Some(directory) = directory {
        process.cwd(directory);
    }

    let child = pair
        .slave
        .spawn_command(process)
        .map_err(|error| error.to_string())?;
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| error.to_string())?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| error.to_string())?;
    let transcript = Arc::new(Mutex::new(String::new()));
    let history = Arc::new(Mutex::new(History::default()));
    let pty = Arc::new(Pty {
        owner: launch.owner,
        master: Mutex::new(pair.master),
        writer: Mutex::new(writer),
        child: Mutex::new(child),
        command: spawn_command,
        cwd: spawn_cwd,
        transcript: Arc::clone(&transcript),
        history: Arc::clone(&history),
    });
    Ok(StartedSession {
        pty,
        reader,
        transcript,
        history,
    })
}

/// Release a session handle on the blocking pool: the last handle's `Drop`
/// kills and reaps the process tree, a blocking `taskkill` wait on Windows.
/// Awaited, so the caller still returns only once the cwd is unlocked.
async fn end_session(pty: Option<Arc<Pty>>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || drop(pty))
        .await
        .map_err(|error| error.to_string())
}

/// Pump one session's output after it has been published in the registry. A chunk
/// enters replay history before its event is emitted, so attach/replay handover
/// remains exact even when the frontend starts listening mid-stream.
fn spawn_session_output_pump(
    app: AppHandle,
    owner: String,
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
                    let _ = app.emit_to(
                        &owner,
                        PTY_DATA_EVENT,
                        Chunk {
                            id: &id,
                            data: &data,
                            seq,
                        },
                    );
                }
                // EOF (agent exited) or a read error - end the pump.
                _ => break,
            }
        }
        let _ = app.emit_to(&owner, PTY_EXIT_EVENT, Exit { id });
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
        let is_still_running = pty
            .child
            .lock()
            .is_ok_and(|mut child| matches!(child.try_wait(), Ok(None)));
        if is_still_running {
            continue;
        }
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
    });
}

/// One live PTY session as `pty_list` reports it: what it runs (`command` - the
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

/// Every session the backend still hosts - the roster a reloaded frontend
/// intersects its persisted pane mapping with to re-attach instead of booting
/// blind (the accidental-reload incident: live PTYs kept running invisibly with
/// no way back). A listed session is alive by construction: the reaper drops a
/// dead one from the map. Sorted by id so the order is deterministic.
#[tauri::command]
pub fn pty_list(window: WebviewWindow, state: State<PtyState>) -> Vec<SessionInfo> {
    let owner = window.label();
    let Ok(sessions) = state.0.lock() else {
        return Vec::new();
    };
    let mut roster: Vec<SessionInfo> = sessions
        .iter()
        .filter(|(_, pty)| pty.owner == owner)
        .map(|(id, pty)| SessionInfo {
            id: id.clone(),
            command: pty.command.clone(),
            cwd: pty.cwd.clone(),
        })
        .collect();
    drop(sessions);
    roster.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    roster
}

/// Everything a terminal needs to paint a session it is attaching to mid-flight,
/// plus the sequence number of the last chunk already inside it - so a frontend that
/// was listening while it asked can tell which chunks it caught are duplicates and
/// which are genuinely new. Empty for an unknown session.
#[tauri::command]
pub fn pty_history(
    window: WebviewWindow,
    state: State<PtyState>,
    id: String,
) -> Result<HistorySnapshot, String> {
    let Some(pty) = owned_pty(&state, window.label(), &id)? else {
        return Ok(HistorySnapshot::default());
    };
    Ok(pty
        .history
        .lock()
        .map(|buffer| HistorySnapshot {
            data: buffer.data[buffer.start..].to_string(),
            seq: buffer.seq,
            alternate: buffer.alternate,
        })
        .unwrap_or_default())
}

/// Read a session's current transcript, or an empty string if it is unknown or
/// the lock is poisoned. The context the AI session-namer summarises.
pub fn transcript_of(state: &PtyState, owner: &str, id: &str) -> Result<String, String> {
    let Some(pty) = owned_pty(state, owner, id)? else {
        return Ok(String::new());
    };
    Ok(pty
        .transcript
        .lock()
        .map(|buffer| buffer.clone())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn pty_write(
    window: WebviewWindow,
    state: State<'_, PtyState>,
    id: String,
    data: String,
) -> Result<(), String> {
    write_input(&state, window.label(), &id, &data)
}

/// Type `data` into session `id` as if from its keyboard - the one path every
/// PTY input takes, so the size limit and window ownership hold for all.
pub fn write_input(
    state: &PtyState,
    window_label: &str,
    id: &str,
    data: &str,
) -> Result<(), String> {
    let is_too_large = data.len() > MAXIMUM_PTY_INPUT_BYTES;
    if is_too_large {
        return Err("PTY input exceeds its size limit".into());
    }
    let Some(pty) = owned_pty(state, window_label, id)? else {
        return Ok(());
    };
    let mut writer = pty.writer.lock().map_err(|error| error.to_string())?;
    writer
        .write_all(data.as_bytes())
        .map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn pty_resize(
    window: WebviewWindow,
    state: State<'_, PtyState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    if !are_valid_dimensions(cols, rows) {
        return Err("PTY dimensions are outside the allowed range".into());
    }
    let Some(pty) = owned_pty(&state, window.label(), &id)? else {
        return Ok(());
    };
    let master = pty.master.lock().map_err(|error| error.to_string())?;
    master
        .resize(pty_size(rows, cols))
        .map_err(|error| error.to_string())
}

/// Whether a terminal size is one a PTY can be opened or resized to.
fn are_valid_dimensions(columns: u16, rows: u16) -> bool {
    let range = 1..=MAXIMUM_PTY_DIMENSION;
    range.contains(&columns) && range.contains(&rows)
}

/// A character-cell size; the pixel size is unused by every backend ADE runs.
fn pty_size(rows: u16, columns: u16) -> PtySize {
    PtySize {
        rows,
        cols: columns,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Close a session: dropping it kills and reaps the agent process (see `Pty`'s
/// `Drop`), so by the time this returns the session's cwd is unlocked and its
/// workspace can be deleted or moved.
#[tauri::command]
pub async fn pty_kill(
    window: WebviewWindow,
    state: State<'_, PtyState>,
    id: String,
) -> Result<(), String> {
    let pty = {
        let mut sessions = state.0.lock().map_err(|error| error.to_string())?;
        session_access(&sessions, &id, window.label())?;
        sessions.remove(&id)
    };
    end_session(pty).await
}

/// The agent session's root process id, owner-checked. `None` when the session is gone or
/// has no live process - a caller treats that as "nothing to stop / not running".
/// Resolves under the registry lock and releases it (plus the child lock) before
/// returning, so the caller's blocking process work never holds either.
fn session_root_process_id(state: &PtyState, owner: &str, id: &str) -> Result<Option<u32>, String> {
    let Some(pty) = owned_pty(state, owner, id)? else {
        return Ok(None);
    };
    let child = pty.child.lock().map_err(|error| error.to_string())?;
    Ok(child.process_id())
}

/// Stop a known task an agent started from the Tasks list. PADE never spawned the
/// process - the agent ran it in its own PTY - so this locates the matching
/// subtree under the agent's own process tree and kills it. Agent-agnostic: it
/// keys on the OS process tree and the task command, nothing agent-specific.
/// Returns whether a matching process was found and killed.
#[tauri::command]
pub async fn session_task_stop(
    window: WebviewWindow,
    state: State<'_, PtyState>,
    id: String,
    command: String,
) -> Result<bool, String> {
    let Some(root) = session_root_process_id(state.inner(), window.label(), &id)? else {
        return Ok(false);
    };

    tauri::async_runtime::spawn_blocking(move || stop_task_tree(root, &command))
        .await
        .map_err(|error| error.to_string())?
}

/// Whether a known task the agent started is still running - a live process under
/// the agent's tree whose command line runs `command`. Lets an attached runner
/// reflect a task that outlives the agent's turn (a dev server) and drop itself
/// once the process exits, without PADE ever having owned it.
#[tauri::command]
pub async fn session_task_running(
    window: WebviewWindow,
    state: State<'_, PtyState>,
    id: String,
    command: String,
) -> Result<bool, String> {
    let Some(root) = session_root_process_id(state.inner(), window.label(), &id)? else {
        return Ok(false);
    };

    tauri::async_runtime::spawn_blocking(move || task_running(root, &command))
        .await
        .map_err(|error| error.to_string())?
}

/// Session `id` when `owner` may use it, released from the registry lock so the
/// caller's PTY or process work never holds it. `None` for an unknown id.
fn owned_pty(state: &PtyState, owner: &str, id: &str) -> Result<Option<Arc<Pty>>, String> {
    let sessions = state.0.lock().map_err(|error| error.to_string())?;
    Ok(session_access(&sessions, id, owner)?.cloned())
}

/// The one owner check every session command makes: the session when `owner`
/// spawned it, `None` when no session has the id, an error when another window
/// owns it.
fn session_access<'a>(
    sessions: &'a HashMap<String, Arc<Pty>>,
    id: &str,
    owner: &str,
) -> Result<Option<&'a Arc<Pty>>, String> {
    let session = sessions.get(id);
    match owner_access(session.map(|pty| pty.owner.as_str()), owner) {
        OwnerAccess::Vacant | OwnerAccess::Owned => Ok(session),
        OwnerAccess::Foreign => Err(FOREIGN_SESSION_ERROR.into()),
    }
}

/// Terminate every live session - called as the app exits so no agent lingers
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
    use crate::test_support::assert_empty;

    fn process(process_id: u32, parent_process_id: u32, command_line: &str) -> ProcessInfo {
        ProcessInfo {
            process_id,
            parent_process_id,
            command_line: command_line.to_string(),
        }
    }

    #[test]
    fn command_matches_survives_shell_wrapping() {
        assert!(command_matches(
            "bash -c \"cd app && pnpm dev\"",
            "pnpm dev"
        ));
        assert!(command_matches("pnpm   dev 2>&1", "pnpm dev"));
        assert!(command_matches(
            "cargo check --all-targets | tail",
            "cargo check --all-targets"
        ));
    }

    #[test]
    fn command_matches_rejects_a_longer_sibling() {
        assert!(!command_matches("pnpm dev:prod", "pnpm dev"));
        assert!(!command_matches("pnpm develop", "pnpm dev"));
        assert!(!command_matches("", "pnpm dev"));
    }

    #[test]
    fn task_processes_finds_the_agent_subtree_only() {
        let processes = vec![
            process(100, 1, "claude"),                 // the agent (root)
            process(200, 100, "bash -c \"pnpm dev\""), // the task shell it spawned
            process(300, 200, "node vite"),            // the dev server (no literal match)
            process(400, 1, "pnpm dev"),               // an UNRELATED sibling outside the agent
        ];
        assert_eq!(task_processes(&processes, 100, "pnpm dev"), vec![200]);
    }

    #[test]
    fn task_processes_walks_every_generation_and_survives_a_self_parented_process() {
        let processes = vec![
            process(0, 0, "pnpm dev"),            // the idle process is its own parent
            process(100, 0, "claude"),            // the agent (root)
            process(200, 100, "bash"),            // a child shell
            process(300, 200, "cmd /c pnpm dev"), // a grandchild running the task
            process(400, 300, "pnpm dev"),        // and its own child
        ];
        assert_eq!(task_processes(&processes, 100, "pnpm dev"), vec![300, 400]);
    }

    #[test]
    fn task_processes_never_returns_the_agent_root() {
        let processes = vec![process(100, 1, "claude pnpm dev")];
        assert_empty(&task_processes(&processes, 100, "pnpm dev"));
    }

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
    /// every following arg verbatim - this is the quoting-risk surface the fix
    /// rides on.
    #[test]
    fn light_console_command_round_trips_the_path_and_args() {
        let executable = r"C:\Program Files\codex dir\codex.exe";
        let arguments = vec![
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
            "-c".to_string(),
            "tui.theme=catppuccin-latte".to_string(),
        ];
        let built = light_console_command(executable, &arguments);
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
                executable.to_string(),
                "--dangerously-bypass-approvals-and-sandbox".to_string(),
                "-c".to_string(),
                "tui.theme=catppuccin-latte".to_string(),
            ]
        );
        // The spaced path is a single token (portable-pty quotes it at spawn), not
        // split - the whole point of separate argv entries.
        assert!(assembled.contains(&executable.to_string()));
    }

    /// Only Codex opts into the console workaround; agents themed by file/env and
    /// unknown commands do not.
    #[test]
    fn only_codex_needs_the_light_console_fix() {
        assert!(crate::agents::needs_light_console_fix("codex"));
        assert!(!crate::agents::needs_light_console_fix("claude"));
        assert!(!crate::agents::needs_light_console_fix("aider"));
        assert!(!crate::agents::needs_light_console_fix(DEFAULT_SHELL));
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
        let over_capacity = HISTORY_CAPACITY / filler.len() + 2;
        let (history, seq) = history_with(&vec![filler.as_str(); over_capacity]);
        let (data, _) = snapshot(&history);
        assert!(data.len() <= HISTORY_CAPACITY);
        assert!(data.starts_with('x'), "replay starts on a whole line");
        assert_eq!(seq, u64::try_from(over_capacity).expect("a small count"));
    }

    #[test]
    fn a_tail_with_no_line_break_is_dropped_entirely() {
        let blob = "y".repeat(HISTORY_CAPACITY + 1);
        let (history, _) = history_with(&[blob.as_str()]);
        assert_eq!(snapshot(&history).0, "");
    }

    #[test]
    fn trimming_never_slices_inside_a_multibyte_character() {
        // Overflow the cap so the cut position lands inside the run of
        // multibyte characters - slicing there would panic before the fix.
        let multibyte = "é".repeat(HISTORY_CAPACITY / 2);
        let (history, _) = history_with(&[multibyte.as_str(), multibyte.as_str()]);
        let (data, _) = snapshot(&history);
        assert!(data.len() <= HISTORY_CAPACITY);
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
        assert_empty(&pending);
    }

    #[test]
    fn drain_decoded_replaces_genuinely_invalid_bytes() {
        let mut pending = vec![b'o', b'k', 0xFF, b'!'];
        assert_eq!(drain_decoded(&mut pending), "ok\u{FFFD}!");
        assert_empty(&pending);
    }

    /// The `pty://data` payload is the frontend contract; borrowing its fields
    /// must not change the wire shape.
    #[test]
    fn a_chunk_serializes_as_id_data_and_sequence() {
        let chunk = Chunk {
            id: "session",
            data: "hello",
            seq: 7,
        };
        assert_eq!(
            serde_json::to_value(&chunk).expect("serializable"),
            serde_json::json!({ "id": "session", "data": "hello", "seq": 7 })
        );
    }

    #[test]
    fn strip_ansi_drops_control_and_operating_system_sequences() {
        let painted =
            "\x1b[1;32mgreen\x1b[0m\r\n\x1b]0;title\x07tab\there\x1b]8;;url\x1b\\link\x1bMx\x01";
        assert_eq!(strip_ansi(painted), "green\ntab\therelinkx");
    }

    #[test]
    fn strip_ansi_tolerates_a_sequence_cut_off_at_the_end() {
        assert_eq!(strip_ansi("text\x1b"), "text");
        assert_eq!(strip_ansi("text\x1b[12"), "text");
        assert_eq!(strip_ansi("text\x1b]0;unterminated"), "text");
    }

    #[test]
    fn dimensions_must_be_positive_and_bounded() {
        assert!(are_valid_dimensions(80, 24));
        assert!(are_valid_dimensions(MAXIMUM_PTY_DIMENSION, 1));
        assert!(!are_valid_dimensions(0, 24));
        assert!(!are_valid_dimensions(80, MAXIMUM_PTY_DIMENSION + 1));
    }

    #[test]
    fn transcript_trims_on_a_char_boundary() {
        let transcript = Arc::new(Mutex::new(String::new()));
        let multibyte = "日".repeat(TRANSCRIPT_CAPACITY / 3 + 10);
        append_transcript(&transcript, &multibyte);
        let tail = transcript.lock().expect("transcript lock").clone();
        assert!(tail.len() <= TRANSCRIPT_CAPACITY);
        assert!(tail.chars().all(|ch| ch == '日'));
    }
}
