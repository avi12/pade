//! The terminal commands the frontend calls, and the `pty://` events it hears.
//!
//! The sessions themselves live in the PTY host process (`pty_host`), so an
//! agent survives an app restart; this module is the app's thin face on them.
//! It validates every request at the IPC boundary, names the calling window as
//! the owner the host checks, and turns the host's output back into events for
//! that window. The command names, arguments and replies are exactly what they
//! were when the app owned the PTYs itself - the frontend cannot tell.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::agents::running_model::ModelChoice;
use crate::terminal::pty_host::client::{HostClient, HostError};
use crate::terminal::session::launch::SessionLaunch;
use crate::terminal::session::output::HistorySnapshot;
use crate::terminal::session::registry::EXIT_TEARDOWN_TIMEOUT;
use crate::terminal::theming::Scheme;
use crate::util::{MAXIMUM_PATH_BYTES, MAXIMUM_SESSION_ID_BYTES};

const MAXIMUM_PTY_DIMENSION: u16 = 1_000;
const MAXIMUM_PTY_INPUT_BYTES: usize = 1024 * 1024;
/// Bounds on what a spawn request may carry, so a malformed IPC call can never
/// hand the OS an unbounded command line (the id and path bounds live in `util`).
const MAXIMUM_SPAWN_ARGUMENTS: usize = 64;
const MAXIMUM_CONVERSATION_ID_BYTES: usize = 256;

/// How long the app's exit waits for the host to end every session: the host's
/// own teardown bound, plus the round trip.
const EXIT_KILL_TIMEOUT: Duration = EXIT_TEARDOWN_TIMEOUT.saturating_add(Duration::from_secs(2));

/// The event carrying a session's raw output chunk to its owning window. This name
/// is the frontend contract (`src/lib/bridge.ts`) - the two sides must agree.
const PTY_DATA_EVENT: &str = "pty://data";

/// The event announcing a session's process exited (EOF or read error).
const PTY_EXIT_EVENT: &str = "pty://exit";

/// The `pty://data` payload. Borrowed, so emitting copies neither the session
/// id nor the chunk.
#[derive(Serialize, Clone)]
struct Chunk<'a> {
    id: &'a str,
    data: &'a str,
    /// This chunk's position in the session's stream - see `History`.
    #[serde(rename = "seq")]
    sequence: u64,
}

/// The `pty://exit` payload.
#[derive(Serialize, Clone)]
struct Exit<'a> {
    id: &'a str,
}

/// Hand a session's output chunk to its owner window.
pub(crate) fn emit_output(app: &AppHandle, owner: &str, id: &str, data: &str, sequence: u64) {
    let _ = app.emit_to(owner, PTY_DATA_EVENT, Chunk { id, data, sequence });
}

/// Tell a session's owner window its process exited.
pub(crate) fn emit_exit(app: &AppHandle, owner: &str, id: &str) {
    let _ = app.emit_to(owner, PTY_EXIT_EVENT, Exit { id });
}

/// One live PTY session as `pty_list` reports it: what it runs and where.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    id: String,
    command: Option<String>,
    cwd: Option<String>,
}

fn to_message(error: HostError) -> String {
    error.to_string()
}

// A PTY spawn is inherently wide (id, command, args, cwd, dimensions) and two of
// these are Tauri-injected (window, host), not caller-supplied - grouping them
// would only obscure the IPC shape the frontend sends.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn pty_spawn(
    window: WebviewWindow,
    host: State<'_, HostClient>,
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
    model_choice: Option<ModelChoice>,
) -> Result<(), String> {
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

    // The host starts the process, so whatever the app would have given it - its
    // working directory as the fallback cwd, its environment - travels along.
    let launch = SessionLaunch {
        command,
        cwd: cwd.or_else(app_directory),
        columns: cols,
        rows,
        arguments: args.unwrap_or_default(),
        scheme,
        conversation_id,
        model_choice,
        environment: app_environment(),
    };
    host.spawn(&id, window.label(), launch)
        .await
        .map_err(to_message)
}

/// The app's working directory, the cwd of a spawn that names none.
fn app_directory() -> Option<String> {
    std::env::current_dir()
        .ok()
        .map(|directory| directory.to_string_lossy().into_owned())
}

/// The app's environment, which every session starts from. A variable that is
/// not valid Unicode cannot cross the protocol and is left out.
fn app_environment() -> Vec<(String, String)> {
    std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect()
}

/// Every session the host runs for the calling window - the roster a reloaded
/// frontend intersects its persisted pane mapping with to re-attach instead of
/// booting blind (the accidental-reload incident: live PTYs kept running
/// invisibly with no way back). Sorted by id.
#[tauri::command]
pub async fn pty_list(
    window: WebviewWindow,
    host: State<'_, HostClient>,
) -> Result<Vec<SessionInfo>, String> {
    let sessions = host.list(Some(window.label())).await.map_err(to_message)?;
    Ok(sessions
        .into_iter()
        .map(|session| SessionInfo {
            id: session.id,
            command: session.command,
            cwd: session.cwd,
        })
        .collect())
}

/// Everything a terminal needs to paint a session it is attaching to mid-flight,
/// plus the sequence number of the last chunk already inside it - so a frontend that
/// was listening while it asked can tell which chunks it caught are duplicates and
/// which are genuinely new. Empty for an unknown session.
#[tauri::command]
pub async fn pty_history(
    window: WebviewWindow,
    host: State<'_, HostClient>,
    id: String,
) -> Result<HistorySnapshot, String> {
    host.history(window.label(), &id).await.map_err(to_message)
}

/// A session's current transcript, or an empty string if it is unknown. The
/// context the AI session-namer summarises.
pub async fn transcript_of(host: &HostClient, owner: &str, id: &str) -> Result<String, String> {
    host.transcript(owner, id).await.map_err(to_message)
}

#[tauri::command]
pub async fn pty_write(
    window: WebviewWindow,
    host: State<'_, HostClient>,
    id: String,
    data: String,
) -> Result<(), String> {
    write_input(&host, window.label(), &id, &data).await
}

/// Type `data` into session `id` as if from its keyboard - the one path every
/// PTY input takes, so the size limit and window ownership hold for all.
pub async fn write_input(
    host: &HostClient,
    window_label: &str,
    id: &str,
    data: &str,
) -> Result<(), String> {
    let is_too_large = data.len() > MAXIMUM_PTY_INPUT_BYTES;
    if is_too_large {
        return Err("PTY input exceeds its size limit".into());
    }
    host.write(window_label, id, data).await.map_err(to_message)
}

#[tauri::command]
pub async fn pty_resize(
    window: WebviewWindow,
    host: State<'_, HostClient>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    if !are_valid_dimensions(cols, rows) {
        return Err("PTY dimensions are outside the allowed range".into());
    }
    host.resize(window.label(), &id, cols, rows)
        .await
        .map_err(to_message)
}

/// Whether a terminal size is one a PTY can be opened or resized to.
fn are_valid_dimensions(columns: u16, rows: u16) -> bool {
    let range = 1..=MAXIMUM_PTY_DIMENSION;
    range.contains(&columns) && range.contains(&rows)
}

/// Close a session: the host kills and reaps the agent's whole process tree
/// before answering, so by the time this returns the session's cwd is unlocked
/// and its workspace can be deleted or moved.
#[tauri::command]
pub async fn pty_kill(
    window: WebviewWindow,
    host: State<'_, HostClient>,
    id: String,
) -> Result<(), String> {
    host.kill(window.label(), &id).await.map_err(to_message)
}

/// Stop a known task an agent started from the Tasks list. PADE never spawned the
/// process - the agent ran it in its own PTY - so this locates the matching
/// subtree under the agent's own process tree and kills it. Agent-agnostic: it
/// keys on the OS process tree and the task command, nothing agent-specific.
/// Returns whether a matching process was found and killed.
#[tauri::command]
pub async fn session_task_stop(
    window: WebviewWindow,
    host: State<'_, HostClient>,
    id: String,
    command: String,
) -> Result<bool, String> {
    let Some(root) = host
        .root_process_id(window.label(), &id)
        .await
        .map_err(to_message)?
    else {
        return Ok(false);
    };

    tauri::async_runtime::spawn_blocking(move || {
        crate::terminal::process_tree::stop_task_tree(root, &command)
    })
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
    host: State<'_, HostClient>,
    id: String,
    command: String,
) -> Result<bool, String> {
    let Some(root) = host
        .root_process_id(window.label(), &id)
        .await
        .map_err(to_message)?
    else {
        return Ok(false);
    };

    tauri::async_runtime::spawn_blocking(move || {
        crate::terminal::process_tree::task_running(root, &command)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Hand live sessions from window `from` to window `to` - the move behind
/// "open this session in a new window" and "merge this window into another".
/// All or nothing; the processes keep running untouched, and the receiving
/// window re-attaches through the usual spawn-is-a-no-op plus `pty_history`.
pub async fn transfer_sessions(
    host: &HostClient,
    from: &str,
    to: &str,
    ids: &[String],
) -> Result<(), String> {
    host.transfer(from, to, ids).await.map_err(to_message)
}

/// End every session `owner` holds - a window closed for good, so nothing it
/// ran may keep running unreachable. Returns at once; the host ends them.
pub fn kill_owned(app: &AppHandle, owner: &str) {
    let app = app.clone();
    let owner = owner.to_string();
    tauri::async_runtime::spawn(end_sessions_of_closed_window(app, owner));
}

async fn end_sessions_of_closed_window(app: AppHandle, owner: String) {
    let host = app.state::<HostClient>();
    if let Err(error) = host.kill_owned(&owner).await {
        eprintln!("[pade] could not end the sessions of closed window {owner}: {error}");
    }
}

/// Terminate every live session as the app exits, so no agent lingers after the
/// window is gone and every workspace cwd-lock is freed - and wait for it, since
/// the process ends right after this returns. A process that is killed instead
/// never gets here, which is exactly what keeps agents alive across a rebuild.
pub fn kill_all(host: &HostClient) {
    // The timer is made inside the runtime: tokio's clock exists only there.
    let ended = tauri::async_runtime::block_on(async {
        tokio::time::timeout(EXIT_KILL_TIMEOUT, host.kill_all()).await
    });
    match ended {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("[pade] could not end the sessions on exit: {error}"),
        Err(_) => eprintln!("[pade] the terminal host did not end the sessions in time"),
    }
}

/// Manage the host client, and connect at once: a restarted app recreates the
/// windows whose sessions outlived it, so none is left running out of reach.
pub fn init(app: &AppHandle) {
    app.manage(HostClient::new(app.clone()));
    tauri::async_runtime::spawn(reattach_at_startup(app.clone()));
}

async fn reattach_at_startup(app: AppHandle) {
    let host = app.state::<HostClient>();
    match host.list(None).await {
        Ok(sessions) => crate::platform::window::recreate_session_windows(
            &app,
            sessions.iter().map(|session| session.owner.as_str()),
        ),
        Err(error) => eprintln!("[pade] the terminal host is unavailable: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `pty://data` payload is the frontend contract; borrowing its fields
    /// must not change the wire shape.
    #[test]
    fn a_chunk_serializes_as_id_data_and_sequence() {
        let chunk = Chunk {
            id: "session",
            data: "hello",
            sequence: 7,
        };
        assert_eq!(
            serde_json::to_value(&chunk).expect("serializable"),
            serde_json::json!({ "id": "session", "data": "hello", "seq": 7 })
        );
        assert_eq!(
            serde_json::to_value(Exit { id: "session" }).expect("serializable"),
            serde_json::json!({ "id": "session" })
        );
    }

    #[test]
    fn dimensions_must_be_positive_and_bounded() {
        assert!(are_valid_dimensions(80, 24));
        assert!(are_valid_dimensions(MAXIMUM_PTY_DIMENSION, 1));
        assert!(!are_valid_dimensions(0, 24));
        assert!(!are_valid_dimensions(80, MAXIMUM_PTY_DIMENSION + 1));
    }

    #[test]
    fn the_app_environment_reaches_a_session() {
        let environment = app_environment();
        assert!(environment
            .iter()
            .any(|(key, _)| key.eq_ignore_ascii_case("PATH")));
    }
}
