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
use tauri::{AppHandle, Emitter, Manager, State};

/// All live runners, keyed by task id.
pub struct RunnerState(pub Mutex<HashMap<String, Runner>>);

/// A live runner: the spawned child (shared so the exit-waiter thread can reap it
/// while `runner_stop` can still kill it) plus its metadata.
pub struct Runner {
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
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Build the platform shell invocation for a command string, mirroring how
/// `ide::ide_open` reaches launchers: `cmd /C <command>` on Windows, else
/// `sh -c <command>` so shell syntax (pipes, `&&`, `$VAR`) works either way.
fn shell_command(command: &str) -> Command {
    let mut cmd = if cfg!(windows) {
        let mut c = crate::util::command("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = crate::util::command("sh");
        c.args(["-c", command]);
        c
    };
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd
}

/// Arguments for [`pump_stream`], bundled so the reader thread takes one param.
struct PumpArgs<R: BufRead> {
    app: AppHandle,
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
        id,
        stream,
        reader,
    }: PumpArgs<R>,
) {
    for line in reader.lines() {
        let Ok(data) = line else { break };
        let _ = app.emit(
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
#[tauri::command]
pub fn runner_start(
    app: AppHandle,
    state: State<RunnerState>,
    id: String,
    command: String,
    cwd: Option<String>,
) -> Result<(), String> {
    let mut runners = state.0.lock().map_err(|e| e.to_string())?;
    if runners.contains_key(&id) {
        return Ok(()); // already running
    }

    let mut cmd = shell_command(&command);
    // An explicit cwd (e.g. a per-branch worktree) overrides the process dir.
    let dir = cwd
        .clone()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok());
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("failed to capture stderr")?;

    // One reader thread per stream so stdout and stderr interleave as they arrive.
    let stdout_args = PumpArgs {
        app: app.clone(),
        id: id.clone(),
        stream: RunnerStream::Stdout,
        reader: BufReader::new(stdout),
    };
    std::thread::spawn(move || pump_stream(stdout_args));

    let stderr_args = PumpArgs {
        app: app.clone(),
        id: id.clone(),
        stream: RunnerStream::Stderr,
        reader: BufReader::new(stderr),
    };
    std::thread::spawn(move || pump_stream(stderr_args));

    // Share the child with the exit-waiter: it reaps via `try_wait()`, `runner_stop`
    // kills via the same handle. The waiter *polls* rather than blocking inside the
    // lock — a blocking `wait()` would hold the lock for the process's whole life and
    // deadlock `runner_stop`, which needs that same lock to `kill()`. Each poll takes
    // the lock only momentarily, then sleeps, so a kill lands promptly.
    let child = Arc::new(Mutex::new(child));
    let waiter_child = Arc::clone(&child);
    let app_exit = app.clone();
    let exit_id = id.clone();
    std::thread::spawn(move || {
        let code = loop {
            let poll = waiter_child.lock().map(|mut c| c.try_wait());
            match poll {
                // Exited: report the code (`None` if killed by a signal).
                Ok(Ok(Some(status))) => break status.code(),
                // Still running — release the lock and poll again shortly.
                Ok(Ok(None)) => std::thread::sleep(std::time::Duration::from_millis(100)),
                // A wait error or a poisoned lock: stop waiting, report unknown code.
                Ok(Err(_)) | Err(_) => break None,
            }
        };
        let _ = app_exit.emit("runner://exit", RunnerExit { id: exit_id, code });
    });

    let info = RunnerInfo {
        id: id.clone(),
        command,
        cwd,
        started_at: now_millis(),
    };
    runners.insert(id, Runner { child, info });
    Ok(())
}

/// Stop a task: kill its process and drop it from the map. Killing is explicit —
/// dropping a `std::process::Child` does *not* terminate the process.
///
/// Windows limitation: `child.kill()` terminates the `cmd /C` process, but a
/// grandchild it spawned (e.g. `node` under `npm run`) can survive, because
/// Windows does not kill process trees by default. Callers that need a hard stop
/// should run their task under a tool that manages its own tree.
#[tauri::command]
pub fn runner_stop(state: State<RunnerState>, id: String) -> Result<(), String> {
    let mut runners = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(runner) = runners.remove(&id) {
        if let Ok(mut child) = runner.child.lock() {
            // Ignore the error: the process may have already exited on its own.
            let _ = child.kill();
        }
    }
    Ok(())
}

/// List every live runner's metadata (id, command, cwd, start time) for the dock.
#[tauri::command]
pub fn runner_list(state: State<RunnerState>) -> Result<Vec<RunnerInfo>, String> {
    let runners = state.0.lock().map_err(|e| e.to_string())?;
    Ok(runners.values().map(|r| r.info.clone()).collect())
}

pub fn init(app: &AppHandle) {
    app.manage(RunnerState(Mutex::new(HashMap::new())));
}
