//! Which model and reasoning effort each agent session is running, so its tab
//! can show it and switch it. PADE learns this from the agent itself, never by
//! reading the terminal screen.
//!
//! The flow, start to finish:
//!
//! 1. Launch: `attach` adds arguments and environment variables so the agent
//!    reports its model (and runs a model picked from the tab, if any).
//! 2. Report: the agent's hooks run PADE's own executable as a "sink"
//!    (`--agent-hook-sink`), or its plugin writes the report itself.
//! 3. Record: the sink folds the report into one small JSON file per session,
//!    named by the conversation id PADE passed in `PADE_CONVERSATION_ID`, then
//!    exits without opening a window.
//! 4. Notify: a file watcher (`init`) sees that file - or a settings file where
//!    `/model` and `/effort` save their choice - change, and emits an event.
//! 5. Read: the tab calls `agent_running_model`, which asks the agent's adapter
//!    for the newest model and effort.
//!
//! Every agent reports differently, so each has an adapter module:
//!
//!   * Claude - hooks inside PADE's one `--settings` layer (`claude`);
//!   * Codex - trusted hooks passed with `-c`, plus its per-turn rollout log
//!     and `config.toml` (`codex`);
//!   * Copilot - its own per-session event log, found by the id PADE pins at
//!     launch (`copilot`);
//!   * `OpenCode` - a per-launch plugin selected through its environment
//!     (`opencode`).
//!
//! The other registered agents (Grok, Antigravity, Cursor, aider) expose no
//! per-session source PADE could use without editing the user's own global
//! config, so they report nothing yet.

mod claude;
mod codex;
mod copilot;
pub(crate) use copilot::session_root as copilot_session_root;
mod hook_sink;
mod opencode;
pub mod pacing_control;
mod sink;
mod state;
mod watch;

pub use sink::run_hook_sink_if_requested;
pub(crate) use state::reported_transcript;
pub use watch::init;

use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, Manager};

use crate::agents::{ID_CLAUDE, ID_CODEX, ID_COPILOT, ID_OPENCODE};
use crate::util::{days_since_epoch, milliseconds_since_unix_epoch};
use sink::{hook_command, HookShell};
use state::{
    is_plain_id, newer, read_session_state, record_choice, state_directory, state_file, write_json,
    Saved, SessionState, STATE_FILE_EXTENSION,
};

/// The variable naming the PADE conversation a launch belongs to, inherited by
/// every hook and plugin the agent runs.
const CONVERSATION_VARIABLE: &str = "PADE_CONVERSATION_ID";
/// Longest model id or effort level a choice may carry. Catalog ids are a few
/// dozen characters; this only bounds what reaches a command line.
const MAXIMUM_CHOICE_LENGTH: usize = 128;
/// The launch flags Claude and Copilot share for a picked model and effort.
const MODEL_FLAG: &str = "--model";
const EFFORT_FLAG: &str = "--effort";
/// `OpenCode`'s launch flag for a picked model.
const OPENCODE_MODEL_FLAG: &str = "-m";
/// How Codex and `OpenCode` take the session id to resume.
const CODEX_RESUME_SUBCOMMAND: &str = "resume";
const OPENCODE_SESSION_FLAG: &str = "--session";

/// A session's model and effort as the agent reported them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningModel {
    model: String,
    effort: Option<String>,
}

/// One model an agent offers, as its own catalog describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    id: String,
    /// The agent's own display name ("Opus 5.5"), or the id when it has none.
    name: String,
    /// The effort levels this model takes, lowest first - empty for a model
    /// without one.
    efforts: Vec<EffortOption>,
    /// The level the agent recommends for this model, when it names one.
    recommended_effort: Option<String>,
}

/// One effort level a model takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffortOption {
    id: String,
    /// The agent's own label ("Extra" for Claude's `xhigh`), else the id.
    name: String,
}

impl EffortOption {
    /// A level whose label is its id.
    fn plain(id: &str) -> Self {
        Self {
            id: id.to_string(),
            name: id.to_string(),
        }
    }
}

/// A model and effort picked from the tab, for one session alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChoice {
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
}

impl ModelChoice {
    /// Whether every part is a short, plain token - safe to put on a command
    /// line or type into a session.
    pub fn is_well_formed(&self) -> bool {
        [&self.model, &self.effort]
            .into_iter()
            .flatten()
            .all(|value| is_plain_token(value))
    }
}

/// How a picked model reaches a running session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SwitchOutcome {
    /// Switched in place - the process, its draft and its running work kept.
    Applied,
    /// The agent can't switch in place; relaunch it on its conversation with
    /// the choice as launch flags (`attach`) once it is idle.
    NeedsRestart,
}

/// What a session runs, plus the session-wide mode that rides on its effort.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionModel {
    #[serde(flatten)]
    running: RunningModel,
    /// Claude's Ultracode (multi-agent orchestration) is on for the session.
    ultracode: bool,
}

/// The model and effort `command`'s session `conversation_id` is running, or
/// `None` when the agent has reported nothing PADE can read. Off the UI thread:
/// it reads files.
#[tauri::command]
pub async fn agent_running_model(command: String, conversation_id: String) -> Option<SessionModel> {
    tauri::async_runtime::spawn_blocking(move || {
        let is_claude = command == ID_CLAUDE;
        if is_claude && is_plain_id(&conversation_id) {
            let (running, ultracode) = claude::read_with_ultracode(&conversation_id)?;
            return Some(SessionModel { running, ultracode });
        }
        let running = read(&command, &conversation_id)?;
        Some(SessionModel {
            running,
            ultracode: false,
        })
    })
    .await
    .ok()
    .flatten()
}

/// The models `command` offers to switch to, from the agent's own local
/// catalog. Empty for an agent PADE can't read one for. Off the UI thread: it
/// reads files.
#[tauri::command]
pub async fn agent_model_options(command: String) -> Vec<ModelOption> {
    tauri::async_runtime::spawn_blocking(move || match command.as_str() {
        ID_CLAUDE => claude::options(),
        ID_CODEX => codex::options(),
        ID_COPILOT => copilot::options(),
        ID_OPENCODE => opencode::options(),
        _ => Vec::new(),
    })
    .await
    .unwrap_or_default()
}

/// Apply a model and effort picked from the tab to a running session. Claude
/// switches in place (typed commands, confirmed through its settings file);
/// every other agent answers `NeedsRestart`. Off the UI thread: it types,
/// waits on a file, and restores the user's saved default.
#[tauri::command]
pub async fn agent_apply_model(
    app: AppHandle,
    window: tauri::WebviewWindow,
    session_id: String,
    command: String,
    conversation_id: String,
    choice: ModelChoice,
) -> Result<SwitchOutcome, String> {
    let can_switch_in_place = command == ID_CLAUDE;
    if !can_switch_in_place {
        return Ok(SwitchOutcome::NeedsRestart);
    }
    if !choice.is_well_formed() || !is_plain_id(&conversation_id) {
        return Err("that model choice isn't valid".into());
    }

    let owner = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let host = app.state::<crate::terminal::pty_host::client::HostClient>();
        // On the blocking pool, so waiting here for each keystroke's round trip
        // to the PTY host holds no async worker.
        let type_keys = |keys: &str| {
            tauri::async_runtime::block_on(crate::terminal::pty::write_input(
                &host,
                &owner,
                &session_id,
                keys,
            ))
        };
        claude::apply_live(&conversation_id, &choice, &type_keys).map(|()| SwitchOutcome::Applied)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Prepare a launch of `program` so it reports its model - and, when a model
/// was picked for this session, runs it: adapter arguments are added to
/// `arguments`, and the environment to set is returned.
pub fn attach(
    program: &str,
    conversation_id: Option<&str>,
    choice: Option<&ModelChoice>,
    arguments: &mut Vec<String>,
) -> Vec<(String, String)> {
    let choice = choice.filter(|choice| choice.is_well_formed());
    if let Some(choice) = choice {
        arguments.extend(launch_arguments(program, choice));
    }
    let Some(conversation_id) = conversation_id.filter(|id| is_plain_id(id)) else {
        return Vec::new();
    };
    // The launch runs the choice from its first moment - before any hook could
    // report it - so it is this session's newest report already.
    if let Some(choice) = choice {
        record_choice(conversation_id, choice);
    }

    let mut environment = vec![(
        CONVERSATION_VARIABLE.to_string(),
        conversation_id.to_string(),
    )];
    match program {
        ID_CLAUDE => claude::attach(arguments),
        ID_CODEX => codex::attach(arguments),
        ID_OPENCODE => environment.extend(opencode::attach(conversation_id)),
        _ => {}
    }
    environment
}

/// The arguments that reopen `command`'s session for `conversation_id` by the
/// agent's own session id, as its hooks or plugin reported it - exact, unlike
/// guessing the newest conversation in the working directory. `None` when the
/// agent pins sessions by PADE's id already (Claude, Copilot) or never reported.
pub fn resume_arguments(command: &str, conversation_id: &str) -> Option<Vec<String>> {
    let agent_session = read_session_state(conversation_id)?.agent_session?;
    session_resume_arguments(command, agent_session)
}

/// The arguments that reopen `command`'s session `agent_session` by the
/// agent's own id - the one place each agent's resume flag is spelled. `None`
/// for an agent that doesn't resume that way.
pub fn session_resume_arguments(command: &str, agent_session: String) -> Option<Vec<String>> {
    match command {
        ID_CODEX => Some(vec![CODEX_RESUME_SUBCOMMAND.to_string(), agent_session]),
        ID_OPENCODE => Some(vec![OPENCODE_SESSION_FLAG.to_string(), agent_session]),
        _ => None,
    }
}

fn read(command: &str, conversation_id: &str) -> Option<RunningModel> {
    if !is_plain_id(conversation_id) {
        return None;
    }
    match command {
        ID_CLAUDE => claude::read(conversation_id),
        ID_CODEX => codex::read(conversation_id),
        ID_COPILOT => copilot::read(conversation_id),
        ID_OPENCODE => opencode::read(conversation_id),
        _ => None,
    }
}

/// A non-empty, bounded value of plain characters - nothing a shell or a TUI
/// would read as anything but text.
fn is_plain_token(value: &str) -> bool {
    let has_valid_length = !value.is_empty() && value.len() <= MAXIMUM_CHOICE_LENGTH;
    has_valid_length && value.chars().all(is_token_character)
}

fn is_token_character(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(character, '-' | '_' | '.' | '/' | ':' | '[' | ']')
}

/// The launch flags that run `choice` for one session without saving it as
/// the agent's default (each verified on the versions in the adapter notes).
fn launch_arguments(program: &str, choice: &ModelChoice) -> Vec<String> {
    let model = choice.model.as_deref();
    let effort = choice.effort.as_deref();
    let flag_pairs = match program {
        ID_CLAUDE | ID_COPILOT => model_and_effort_flags(model, effort),
        ID_CODEX => codex::choice_overrides(model, effort),
        // OpenCode's TUI takes no effort flag; the variant follows the model's
        // own default until picked inside it.
        ID_OPENCODE => model
            .map(|model| (OPENCODE_MODEL_FLAG, model.to_string()))
            .into_iter()
            .collect(),
        _ => Vec::new(),
    };

    flag_pairs
        .into_iter()
        .flat_map(|(flag, value)| [flag.to_string(), value])
        .collect()
}

fn model_and_effort_flags(
    model: Option<&str>,
    effort: Option<&str>,
) -> Vec<(&'static str, String)> {
    let model_flag = model.map(|model| (MODEL_FLAG, model.to_string()));
    let effort_flag = effort.map(|effort| (EFFORT_FLAG, effort.to_string()));
    model_flag.into_iter().chain(effort_flag).collect()
}

pub(super) fn modified_milliseconds(path: &Path) -> Option<u64> {
    Some(milliseconds_since_unix_epoch(
        std::fs::metadata(path).ok()?.modified().ok()?,
    ))
}

/// Milliseconds since the epoch for an RFC 3339 UTC timestamp as agents write
/// them (`2026-10-12T01:28:38.123Z`). Hand-rolled to keep a date crate out.
pub(super) fn timestamp_milliseconds(timestamp: &str) -> Option<u64> {
    let (date, time) = timestamp.strip_suffix('Z')?.split_once('T')?;
    let [year, month, day] = three_numbers(date, '-')?;
    let (clock, fraction) = time.split_once('.').unwrap_or((time, "0"));
    let [hour, minute, second] = three_numbers(clock, ':')?;
    let milliseconds: i64 = format!("{fraction:0<3}").get(..3)?.parse().ok()?;

    let days = days_since_epoch(year, month, day);
    let total = ((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + milliseconds;
    u64::try_from(total).ok()
}

/// The first three `separator`-separated numbers in `text` (`2026-10-12`).
fn three_numbers(text: &str, separator: char) -> Option<[i64; 3]> {
    let mut parts = text.split(separator).map(str::parse::<i64>);
    Some([
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    ])
}

#[cfg(test)]
mod tests;
