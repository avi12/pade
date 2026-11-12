//! The hook sink's own job and the hook command that reaches it: the report an
//! agent's hook pipes in is either answered (a switch or tool-use question) or
//! folded into the session's state file, and `hook_command` spells how each
//! hook shell invokes the stable sink executable.

use std::io::Read;

use super::state::{fold, read_state, state_file, write_json};
use super::{claude, hook_sink, pacing_control, CONVERSATION_VARIABLE};
use crate::util::now_milliseconds;

/// The first argument that turns `pade.exe` into the hook sink.
const HOOK_SINK_FLAG: &str = "--agent-hook-sink";

/// Run as the hook sink when PADE was launched as one, returning whether it
/// did - the caller must then exit instead of starting the app. A failure goes
/// to stderr, which the agent shows in its hook log; it never holds a session
/// up.
pub fn run_hook_sink_if_requested() -> bool {
    let is_sink = std::env::args().nth(1).as_deref() == Some(HOOK_SINK_FLAG);
    if !is_sink {
        return false;
    }

    if let Err(error) = record_hook_report() {
        eprintln!("pade: could not record the agent's hook report: {error}");
    }
    true
}

/// The sink's job: read the report the agent piped in and either answer it
/// (a switch request) or fold it into the session's state file.
fn record_hook_report() -> Result<(), String> {
    let report = read_hook_report()?;
    let conversation_id = reporting_conversation(&report).ok_or("the report names no session")?;

    match hook_event(&report) {
        Some(claude::PRE_MODEL_SWITCH) => {
            answer_model_switch_request(&conversation_id);
            return Ok(());
        }
        Some(pacing_control::PRE_TOOL_USE) => {
            pacing_control::answer_tool_use(&conversation_id, &report);
            return Ok(());
        }
        _ => {}
    }

    let path = state_file(&conversation_id).ok_or("no PADE data directory")?;
    let next = fold(
        read_state(&path).unwrap_or_default(),
        &report,
        now_milliseconds(),
    );
    write_json(&path, &next).map_err(|error| error.to_string())
}

fn read_hook_report() -> Result<serde_json::Value, String> {
    let mut stdin = String::new();
    std::io::stdin()
        .read_to_string(&mut stdin)
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&stdin).map_err(|error| error.to_string())
}

/// PADE's conversation id from the environment, else the agent's own session id
/// from the report.
fn reporting_conversation(report: &serde_json::Value) -> Option<String> {
    std::env::var(CONVERSATION_VARIABLE)
        .ok()
        .or_else(|| report.get("session_id")?.as_str().map(str::to_string))
}

/// The event a hook report is for - some are questions to answer, not reports.
fn hook_event(report: &serde_json::Value) -> Option<&str> {
    report
        .get("hook_event_name")
        .and_then(serde_json::Value::as_str)
}

/// PADE answers only for a switch it started itself; any other one gets
/// Claude's own prompt.
fn answer_model_switch_request(conversation_id: &str) {
    if claude::has_switch_permission(conversation_id) {
        println!("{}", claude::ALLOW_MODEL_SWITCH);
    }
}

/// The shell an agent runs its hook commands through - which decides how a
/// quoted executable path must be invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HookShell {
    /// A POSIX shell (Claude on Windows uses Git Bash): `"path" args`.
    Posix,
    /// `PowerShell` (Codex on Windows): a quoted path is only a string there, so
    /// it takes the call operator - `& 'path' args`.
    PowerShell,
}

/// The command a hook runs to reach the sink, written for `shell`: the stable
/// sink the app keeps current (`hook_sink`), never the PTY host's own frozen
/// copy. Forward slashes, because Git Bash reads a backslash as an escape and
/// every shell accepts them.
pub(super) fn hook_command(shell: HookShell) -> Option<String> {
    let executable = hook_sink::executable()?;
    let path = executable.to_string_lossy().replace('\\', "/");
    Some(match shell {
        HookShell::Posix => format!("\"{path}\" {HOOK_SINK_FLAG}"),
        HookShell::PowerShell => format!("& '{}' {HOOK_SINK_FLAG}", path.replace('\'', "''")),
    })
}
