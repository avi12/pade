//! Child processes: the windowless `command`, bounded waits, captured output and detached spawns.

use std::ffi::OsStr;
use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// A `Command` that never flashes a console window on Windows. Every background
/// or captured-output spawn - PATH lookups, git, curl, the agent namer, task
/// runners, registry edits - goes through this so a GUI app stays windowless
/// instead of popping a `conhost` window per spawn (e.g. on the 5s agent
/// re-detect). Interactive terminals the user explicitly opens (`os.rs`) are
/// spawned directly so they *do* get a window.
///
/// stdin is closed by default: none of these children are interactive, and a
/// CLI that decides to wait on stdin (the opencode-db hang class) must see EOF
/// immediately rather than block forever. The rare caller that feeds a child on
/// stdin (the usage curl `--config`) overrides this with `Stdio::piped()`.
pub(crate) fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// The child gets no console window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// The platform shell that runs a command line, and the flag that hands it one:
/// `cmd /C` on Windows, so a `.cmd` shim (pnpm, code, idea) resolves the way a
/// terminal would, and `sh -c` elsewhere, so shell syntax (pipes, `&&`, `$VAR`)
/// works either way.
pub(crate) const COMMAND_SHELL: &str = if cfg!(windows) { "cmd" } else { "sh" };

pub(crate) const COMMAND_SHELL_RUN_FLAG: &str = if cfg!(windows) { "/C" } else { "-c" };

/// A windowless [`command`] that runs `command_line` through the platform
/// shell ([`COMMAND_SHELL`]). The caller decides the streams and working
/// directory.
pub(crate) fn shell_command(command_line: &str) -> Command {
    let mut process = command(COMMAND_SHELL);
    process.args([COMMAND_SHELL_RUN_FLAG, command_line]);
    process
}

/// How often [`exits_successfully_within`] re-checks a child it is waiting on. Matches the
/// runner's exit-poll cadence: prompt enough for an interactive check, cheap
/// enough to leave the child undisturbed.
const CHILD_WAIT_POLL: Duration = Duration::from_millis(50);

/// Run `command` to completion under a hard deadline, reporting only whether it
/// exited successfully. All three standard streams are closed (the caller wants
/// a verdict, not output - and an open stdin is exactly what lets a child wait
/// forever). On deadline the child is killed and the answer is `false`: a probe
/// that can hang is never worth a hang.
pub(crate) fn succeeds_within(mut command: Command, timeout: Duration) -> bool {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    exits_successfully_within(&mut child, timeout)
}

/// Wait for a spawned `child` under a hard deadline, reporting whether it exited
/// successfully. On deadline (or a failed wait) the child is killed and reaped
/// and the answer is `false`. The caller owns the child's streams - one that
/// reads its output does so on another thread while this waits.
pub(crate) fn exits_successfully_within(child: &mut Child, timeout: Duration) -> bool {
    exit_within(child, timeout).is_some_and(|status| status.success())
}

/// Wait for a spawned `child` under a hard deadline, answering how it exited. On
/// deadline (or a failed wait) the child is killed and reaped and the answer is
/// `None`.
fn exit_within(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(CHILD_WAIT_POLL),
            // Deadline reached or the wait itself failed: reap and report nothing.
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// How long a cleanly exited child's output may take to finish draining. A
/// descendant that outlived the child can hold the pipe open; the output is
/// then abandoned rather than waited on forever.
const OUTPUT_DRAIN_GRACE: Duration = Duration::from_secs(1);

/// What a [`capture_within`] run printed, and whether it exited successfully.
pub(crate) struct CapturedRun {
    pub(crate) is_success: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// Run `command` under a hard deadline, capturing stdout and stderr. stdin is
/// closed so a CLI that expects input gets EOF instead of hanging. `None` when
/// it couldn't start, or didn't finish in time (it is killed and reaped).
///
/// Both streams drain on their own threads while the child runs: a child
/// blocked on a full pipe never exits, so reading only after the exit would
/// stall any output bigger than the pipe buffer until the deadline killed it.
pub(crate) fn capture_within(mut command: Command, timeout: Duration) -> Option<CapturedRun> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().ok()?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let status = exit_within(&mut child, timeout)?;
    Some(CapturedRun {
        is_success: status.success(),
        stdout: stdout.recv_timeout(OUTPUT_DRAIN_GRACE).ok()?.ok()?,
        stderr: stderr
            .recv_timeout(OUTPUT_DRAIN_GRACE)
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default(),
    })
}

/// Read `stream` to its end on a thread of its own, answering on the returned
/// channel - its text, or the read error (a stream that isn't UTF-8).
fn drain(
    stream: Option<impl Read + Send + 'static>,
) -> std::sync::mpsc::Receiver<std::io::Result<String>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let read = stream
            .map_or(Ok(0), |mut stream| stream.read_to_string(&mut text))
            .map(|_| text);
        let _ = sender.send(read);
    });
    receiver
}

/// Spawn `program args` as a process that fully **outlives** ADE - used only for
/// launching a GUI IDE (see `ide::spawn_launcher`), which the user expects to
/// keep running after they close ADE.
///
/// Why the plain [`command`] spawn is not enough on Windows: a child inherits
/// its parent's console *and* its parent's Job object, and job membership is
/// inherited transitively. A GUI app like ADE is commonly placed in a Job whose
/// limit flags include `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so when ADE exits
/// and its job handle closes, every process still in that job - including an IDE
/// we merely started - is terminated. That is exactly requirement #1's failure.
/// Three creation flags break the child out of both:
///
/// - `DETACHED_PROCESS` (`0x0000_0008`): the child gets no console of its own. A
///   GUI IDE opens its own window and needs none, so this is the right console
///   flag here - and it *replaces* [`command`]'s `CREATE_NO_WINDOW`, with which
///   it is mutually exclusive (both are console-disposition flags). No console
///   means no `conhost` flash, preserving [`command`]'s windowless property.
/// - `CREATE_NEW_PROCESS_GROUP` (`0x0000_0200`): the child leads its own process
///   group, so a Ctrl-C / console signal aimed at ADE never reaches it.
/// - `CREATE_BREAKAWAY_FROM_JOB` (`0x0100_0000`): the child is created *outside*
///   ADE's job, so closing ADE's job on exit cannot kill it.
///
/// Robustness: `CREATE_BREAKAWAY_FROM_JOB` makes `CreateProcess` fail with
/// `ERROR_ACCESS_DENIED` (os error 5) when ADE's job forbids breakaway (its
/// limits lack `JOB_OBJECT_LIMIT_BREAKAWAY_OK`) or ADE is in no job at all. On
/// that one error we retry without breakaway - still detached and in its own
/// group, which is the best we can do inside a no-breakaway job. Any other error
/// is returned unchanged, never swallowed.
///
/// On non-Windows there is no job/console teardown to escape: a spawned child is
/// already independent and is re-parented to init when ADE exits, so a plain
/// spawn suffices.
#[cfg(windows)]
pub(crate) fn spawn_detached(program: impl AsRef<OsStr>, arguments: &[String]) -> std::io::Result<Child> {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    const ERROR_ACCESS_DENIED: i32 = 5;

    let detached = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    let spawn = |flags: u32| {
        Command::new(&program)
            .args(arguments)
            .creation_flags(flags)
            .spawn()
    };
    match spawn(detached | CREATE_BREAKAWAY_FROM_JOB) {
        Ok(child) => Ok(child),
        Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED) => spawn(detached),
        Err(error) => Err(error),
    }
}

#[cfg(not(windows))]
pub fn spawn_detached(program: impl AsRef<OsStr>, arguments: &[String]) -> std::io::Result<Child> {
    Command::new(program).args(arguments).spawn()
}
