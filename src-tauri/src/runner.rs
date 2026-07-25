//! Task-runner dock: headless command execution with captured output.
//!
//! Unlike the PTY host (which runs interactive agents in a real pseudo-terminal),
//! a runner is fire-and-forget output capture — dev servers, build scripts, test
//! watchers. It runs the command through the platform shell with stdout and
//! stderr piped *separately*, streaming each line to the frontend tagged with the
//! runner id and which stream it came from. There is no PTY, but a child still
//! writes its own ANSI/SGR colour codes into the pipe (a dev server's coloured
//! banner) — the pipe strips nothing, so the frontend renders those colours
//! (`lib/ansi` → the shared terminal palette) rather than showing raw escapes.
//!
//! Multi-runner: each task runs under a caller-chosen id so several can run at
//! once (a dev server plus a test watcher, say). Events carry the id so the right
//! dock row receives them.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::util::{owner_access, OwnerAccess};

const MAXIMUM_RUNNERS_PER_WINDOW: usize = 32;
const MAX_RUNNER_COMMAND_BYTES: usize = 16 * 1024;

struct RunnerRequest<'a> {
    id: &'a str,
    command: &'a str,
    cwd: Option<&'a str>,
}

fn validate_runner_request(
    RunnerRequest { id, command, cwd }: RunnerRequest<'_>,
) -> Result<(), String> {
    let exceeds_limits = id.len() > 128
        || id.chars().any(char::is_control)
        || command.len() > MAX_RUNNER_COMMAND_BYTES
        || cwd.is_some_and(|value| value.len() > 4096);
    if exceeds_limits {
        return Err("runner request exceeds its input limits".into());
    }
    Ok(())
}

struct RunnerAccessRequest<'a> {
    state: &'a RunnerState,
    owner: &'a str,
    registry_id: &'a str,
}

fn runner_start_access(
    RunnerAccessRequest {
        state,
        owner,
        registry_id,
    }: RunnerAccessRequest<'_>,
) -> Result<OwnerAccess, String> {
    let runners = state.0.lock().map_err(|e| e.to_string())?;
    let access = owner_access(
        runners.get(registry_id).map(|runner| runner.owner.as_str()),
        owner,
    );
    let owner_runner_count = runners
        .values()
        .filter(|runner| runner.owner == owner)
        .count();
    if access == OwnerAccess::Vacant && owner_runner_count >= MAXIMUM_RUNNERS_PER_WINDOW {
        return Err("this window has reached its runner limit".into());
    }
    Ok(access)
}

/// All live runners, keyed by task id.
pub struct RunnerState(pub Mutex<HashMap<String, Runner>>);

/// A live runner: the spawned child (shared so the exit-waiter thread can reap it
/// while `runner_stop` can still kill it) plus its metadata.
pub struct Runner {
    /// The window allowed to inspect and stop this runner.
    owner: String,
    /// The child process, shared with its exit-waiter thread. `runner_stop` locks
    /// it to `kill()`; the waiter locks it to `wait()`.
    child: Arc<Mutex<Child>>,
    info: RunnerInfo,
}

/// Metadata describing a runner, surfaced to the UI (dock rows, restart, etc.).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RunnerInfo {
    id: String,
    command: String,
    cwd: Option<String>,
    /// Unix epoch milliseconds when the task was spawned.
    started_at: u64,
}

/// Which pipe a captured line came from. Mirrors the frontend's
/// `z.enum(["stdout", "stderr"])` — one authoritative home for the two names.
#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum RunnerStream {
    Stdout,
    Stderr,
}

/// One captured line of output from a runner.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RunnerData {
    id: String,
    data: String,
    /// Which stream the line came from.
    stream: RunnerStream,
}

/// Emitted once when a runner's process exits.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RunnerExit {
    id: String,
    /// The process exit code, or `None` if it was terminated by a signal.
    code: Option<i32>,
}

/// Current Unix time in milliseconds (0 if the clock is before the epoch).
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Build the platform shell invocation for a command string, mirroring how
/// `ide::ide_open` reaches launchers: `cmd /C <command>` on Windows, else
/// `sh -c <command>` so shell syntax (pipes, `&&`, `$VAR`) works either way.
fn shell_command(command: &str) -> Command {
    let mut process = if cfg!(windows) {
        let mut command_process = crate::util::command("cmd");
        command_process.args(["/C", command]);
        command_process
    } else {
        let mut command_process = crate::util::command("sh");
        command_process.args(["-c", command]);
        command_process
    };
    process.stdout(Stdio::piped()).stderr(Stdio::piped());
    process
}

/// Arguments for [`pump_stream`], bundled so the reader thread takes one param.
struct PumpArgs<R: BufRead> {
    app: AppHandle,
    owner: String,
    id: String,
    stream: RunnerStream,
    reader: R,
}

/// Pump one piped stream line-by-line to the frontend, tagging each line with the
/// runner id and stream name. Emit errors are swallowed so a closed frontend never
/// panics the reader thread.
fn pump_stream<R: BufRead>(
    PumpArgs {
        app,
        owner,
        id,
        stream,
        reader,
    }: PumpArgs<R>,
) {
    for line in reader.lines() {
        let Ok(data) = line else { break };
        let _ = app.emit_to(
            &owner,
            "runner://data",
            RunnerData {
                id: id.clone(),
                data,
                stream,
            },
        );
    }
}

/// Start a task: spawn `command` through the platform shell in `cwd` (defaulting
/// to the current directory), capturing stdout and stderr as separate `runner://data`
/// line streams and emitting `runner://exit` with the exit code when it ends.
///
/// Idempotent: if a runner with `id` is already live, returns `Ok(())` without
/// spawning a second one (mirrors `pty_spawn`).
// `async`: spawning a process is blocking OS work — a synchronous command runs
// it on the MAIN thread, stalling the message pump.
#[tauri::command]
pub async fn runner_start(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, RunnerState>,
    id: String,
    command: String,
    cwd: Option<String>,
) -> Result<(), String> {
    let owner = window.label().to_string();
    validate_runner_request(RunnerRequest {
        id: &id,
        command: &command,
        cwd: cwd.as_deref(),
    })?;
    let registry_id = format!("{owner}\0{id}");
    match runner_start_access(RunnerAccessRequest {
        state: &state,
        owner: &owner,
        registry_id: &registry_id,
    })? {
        OwnerAccess::Owned => return Ok(()),
        OwnerAccess::Foreign => return Err("runner belongs to another window".into()),
        OwnerAccess::Vacant => {}
    }

    let mut process = shell_command(&command);
    let directory = cwd
        .clone()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok());
    if let Some(directory) = directory {
        process.current_dir(directory);
    }

    let mut child = process.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("failed to capture stderr")?;

    let stdout_args = PumpArgs {
        app: app.clone(),
        owner: owner.clone(),
        id: id.clone(),
        stream: RunnerStream::Stdout,
        reader: BufReader::new(stdout),
    };
    std::thread::spawn(move || pump_stream(stdout_args));

    let stderr_args = PumpArgs {
        app: app.clone(),
        owner: owner.clone(),
        id: id.clone(),
        stream: RunnerStream::Stderr,
        reader: BufReader::new(stderr),
    };
    std::thread::spawn(move || pump_stream(stderr_args));

    // Polling avoids holding the child lock while the process runs.
    let child = Arc::new(Mutex::new(child));
    let waiter_child = Arc::clone(&child);
    let app_exit = app.clone();
    let exit_owner = owner.clone();
    let exit_id = id.clone();
    std::thread::spawn(move || {
        let code = loop {
            let poll = waiter_child.lock().map(|mut child| child.try_wait());
            match poll {
                // Exited: report the code (`None` if killed by a signal).
                Ok(Ok(Some(status))) => break status.code(),
                // Still running — release the lock and poll again shortly.
                Ok(Ok(None)) => std::thread::sleep(std::time::Duration::from_millis(100)),
                // A wait error or a poisoned lock: stop waiting, report unknown code.
                Ok(Err(_)) | Err(_) => break None,
            }
        };
        let _ = app_exit.emit_to(
            &exit_owner,
            "runner://exit",
            RunnerExit { id: exit_id, code },
        );
    });

    let info = RunnerInfo {
        id: id.clone(),
        command,
        cwd,
        started_at: now_millis(),
    };
    let mut runners = state.0.lock().map_err(|e| e.to_string())?;
    match owner_access(
        runners
            .get(&registry_id)
            .map(|runner| runner.owner.as_str()),
        &owner,
    ) {
        OwnerAccess::Owned | OwnerAccess::Foreign => {
            let is_foreign = runners
                .get(&registry_id)
                .is_some_and(|runner| runner.owner != owner);
            drop(runners);
            if let Ok(mut duplicate) = child.lock() {
                stop_process_tree(&mut duplicate);
            }
            if is_foreign {
                return Err("runner belongs to another window".into());
            }
            return Ok(());
        }
        OwnerAccess::Vacant => {}
    }
    runners.insert(registry_id, Runner { owner, child, info });
    Ok(())
}

/// Stop a task's whole process tree. Dropping a `std::process::Child` does *not*
/// terminate it, and on Windows killing only `cmd /C` leaves children such as a
/// Vite `node` process alive in the workspace the user has just left.
fn stop_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let process_id = child.id().to_string();
        // `/T` includes descendants and `/F` handles a runner that does not
        // cooperate with a graceful terminate. `util::command` keeps taskkill
        // itself from flashing a console window in the GUI app.
        let _ = crate::util::command("taskkill")
            .args(["/PID", process_id.as_str(), "/T", "/F"])
            .status();
    }

    #[cfg(not(windows))]
    {
        let _ = child.kill();
    }
}

/// Stop a task and drop it from the map.
// `async`, and the registry lock is released before the process-tree kill (a
// blocking `taskkill` wait on Windows) — only the removed runner's own child
// lock is held while it dies, so other runners stay reachable meanwhile.
#[tauri::command]
pub async fn runner_stop(
    window: WebviewWindow,
    state: State<'_, RunnerState>,
    id: String,
) -> Result<(), String> {
    let registry_id = format!("{}\0{id}", window.label());
    let removed = {
        let mut runners = state.0.lock().map_err(|e| e.to_string())?;
        if owner_access(
            runners
                .get(&registry_id)
                .map(|runner| runner.owner.as_str()),
            window.label(),
        ) == OwnerAccess::Foreign
        {
            return Err("runner belongs to another window".into());
        }
        runners.remove(&registry_id)
    };
    let Some(runner) = removed else {
        return Ok(());
    };
    if let Ok(mut child) = runner.child.lock() {
        stop_process_tree(&mut child);
    }
    Ok(())
}

/// List every live runner's metadata (id, command, cwd, start time) for the dock.
#[tauri::command]
pub fn runner_list(
    window: WebviewWindow,
    state: State<RunnerState>,
) -> Result<Vec<RunnerInfo>, String> {
    let runners = state.0.lock().map_err(|e| e.to_string())?;
    Ok(runners
        .values()
        .filter(|runner| runner.owner == window.label())
        .map(|runner| runner.info.clone())
        .collect())
}

pub fn init(app: &AppHandle) {
    app.manage(RunnerState(Mutex::new(HashMap::new())));
}
