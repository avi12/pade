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
mod opencode;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

use crate::agents::{ID_CLAUDE, ID_CODEX, ID_COPILOT, ID_OPENCODE};

/// The first argument that turns `pade.exe` into the hook sink.
const HOOK_SINK_FLAG: &str = "--agent-hook-sink";
/// The variable naming the PADE conversation a launch belongs to, inherited by
/// every hook and plugin the agent runs.
const CONVERSATION_VARIABLE: &str = "PADE_CONVERSATION_ID";
/// Tells every window a session's model or effort may have changed. The payload
/// names the conversation, or is null when a settings file every session reads
/// changed.
const CHANGED_EVENT: &str = "agent-model://changed";
/// A session file outlives its session so a resumed conversation still has its
/// model; past this age nobody will resume it.
const STATE_RETENTION: Duration = Duration::from_hours(14 * 24);
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChoice {
    model: Option<String>,
    effort: Option<String>,
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
        let running = read(&command, &conversation_id)?;
        let ultracode = command == ID_CLAUDE && claude::is_ultracode_on(&conversation_id);
        Some(SessionModel { running, ultracode })
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
        let pty_state = app.state::<crate::pty::PtyState>();
        let type_keys = |keys: &str| crate::pty::write_input(&pty_state, &owner, &session_id, keys);
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

/// Watch every place a session's model or effort is written, telling the
/// windows the moment one may have moved. Best-effort: without the watch a tab
/// still catches up when its session next goes idle.
pub fn init(app: &AppHandle) {
    let Some(states) = state_directory() else {
        return;
    };
    if let Err(error) = std::fs::create_dir_all(&states) {
        eprintln!("pade: no agent model-state directory: {error}");
        return;
    }
    prune_expired(&states);

    let places = watched_places(states);
    let emitter = app.clone();
    let routes = places.clone();
    let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        for path in &event.paths {
            if let Some(payload) = changed_payload(path, &routes) {
                let _ = emitter.emit(CHANGED_EVENT, payload);
            }
        }
    });
    let Ok(mut watcher) = watcher else {
        return;
    };
    for (directory, place) in &places {
        // Best-effort per place: an agent that isn't installed has no folder.
        let _ = watcher.watch(directory, place.recursive_mode());
    }
    app.manage(ModelWatch {
        _watcher: Mutex::new(watcher),
    });
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

    let mut arguments = Vec::new();
    for (flag, value) in flag_pairs {
        arguments.push(flag.to_string());
        arguments.push(value);
    }
    arguments
}

fn model_and_effort_flags(
    model: Option<&str>,
    effort: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut flags = Vec::new();
    if let Some(model) = model {
        flags.push((MODEL_FLAG, model.to_string()));
    }
    if let Some(effort) = effort {
        flags.push((EFFORT_FLAG, effort.to_string()));
    }
    flags
}

/// The sink's job: read the report the agent piped in and either answer it
/// (a switch request) or fold it into the session's state file.
fn record_hook_report() -> Result<(), String> {
    let report = read_hook_report()?;
    let conversation_id = reporting_conversation(&report).ok_or("the report names no session")?;

    if is_model_switch_request(&report) {
        answer_model_switch_request(&conversation_id);
        return Ok(());
    }

    let path = state_file(&conversation_id).ok_or("no PADE data directory")?;
    let next = fold(read_state(&path).unwrap_or_default(), &report, now_millis());
    write_state(&path, &next).map_err(|error| error.to_string())
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

/// Not a report: Claude asking whether to go ahead with a model switch.
fn is_model_switch_request(report: &serde_json::Value) -> bool {
    report
        .get("hook_event_name")
        .and_then(serde_json::Value::as_str)
        == Some(claude::PRE_MODEL_SWITCH)
}

/// PADE answers only for a switch it started itself; any other one gets
/// Claude's own prompt.
fn answer_model_switch_request(conversation_id: &str) {
    if claude::consume_switch_permission(conversation_id) {
        println!("{}", claude::ALLOW_MODEL_SWITCH);
    }
}

/// Fold one hook report into a session's state. The fields are the ones Claude
/// and Codex hooks share: `model` (or `to_model` on a model switch), an
/// `effort.level`, and the `transcript_path`.
fn fold(state: SessionState, report: &serde_json::Value, now: u64) -> SessionState {
    let text = |pointer: &str| report.pointer(pointer).and_then(serde_json::Value::as_str);
    let reported_model = text("/to_model").or_else(|| text("/model"));
    let reported_effort = text("/effort/level");

    let mut next = state;
    // Only a different model is a switch: re-reporting the same one (every
    // Codex hook, Claude's SessionStart after a launch with `--model`) must not
    // date it later, or the saved effort would outrank the one it runs with.
    let is_model_switch = reported_model.is_some_and(|model| next.model.as_deref() != Some(model));
    if let Some(model) = reported_model.filter(|_| is_model_switch) {
        next.model = Some(model.to_string());
        next.model_at = now;
    }
    if let Some(effort) = reported_effort {
        next.effort = Some(effort.to_string());
        next.effort_at = now;
    }
    if let Some(transcript) = text("/transcript_path") {
        next.transcript = Some(transcript.to_string());
    }
    if let Some(agent_session) = text("/session_id") {
        next.agent_session = Some(agent_session.to_string());
    }
    next
}

/// The shell an agent runs its hook commands through - which decides how a
/// quoted executable path must be invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HookShell {
    /// A POSIX shell (Claude on Windows uses Git Bash): `"path" args`.
    Posix,
    /// `PowerShell` (Codex on Windows): a quoted path is only a string there, so
    /// it takes the call operator - `& 'path' args`.
    PowerShell,
}

/// The command a hook runs to reach the sink, written for `shell`. Forward
/// slashes, because Git Bash reads a backslash as an escape and every shell
/// accepts them.
fn hook_command(shell: HookShell) -> Option<String> {
    let executable = std::env::current_exe().ok()?;
    let path = executable.to_string_lossy().replace('\\', "/");
    Some(match shell {
        HookShell::Posix => format!("\"{path}\" {HOOK_SINK_FLAG}"),
        HookShell::PowerShell => format!("& '{}' {HOOK_SINK_FLAG}", path.replace('\'', "''")),
    })
}

/// What hooks and plugins have reported for one session, each value stamped
/// with when it was reported (milliseconds since the epoch). This is the shape
/// of the per-session JSON file.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SessionState {
    model: Option<String>,
    model_at: u64,
    effort: Option<String>,
    effort_at: u64,
    /// The agent's own transcript for this session, when its hooks name one.
    transcript: Option<String>,
    /// The agent's own id for this session (a Codex thread, an `OpenCode`
    /// session) - what resuming it takes.
    agent_session: Option<String>,
}

/// A value the agent saved to a settings file, dated by that file's last write.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Saved {
    value: String,
    saved_at: u64,
}

/// The newer of a reported value and a saved one. `saved_from` is the earliest
/// moment the saved value can speak for this session - a model switch, say,
/// adopts the new model's saved level at the switch. A tie goes to the report,
/// which is about this session alone; the saved file is shared by every one.
fn newer(reported: Option<(&str, u64)>, saved: Option<&Saved>, saved_from: u64) -> Option<String> {
    let Some(saved) = saved else {
        return reported.map(|(value, _)| value.to_string());
    };
    let saved_applies_at = saved.saved_at.max(saved_from);
    match reported {
        Some((value, reported_at)) if reported_at >= saved_applies_at => Some(value.to_string()),
        _ => Some(saved.value.clone()),
    }
}

/// A conversation id safe to use as a file name.
fn is_plain_id(conversation_id: &str) -> bool {
    !conversation_id.is_empty()
        && conversation_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

/// Where the sink and plugins keep one file per session.
fn state_directory() -> Option<PathBuf> {
    Some(crate::util::pade_data_dir()?.join("agent-model"))
}

fn state_file(conversation_id: &str) -> Option<PathBuf> {
    Some(state_directory()?.join(format!("{conversation_id}.json")))
}

fn read_state(path: &Path) -> Option<SessionState> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn read_session_state(conversation_id: &str) -> Option<SessionState> {
    read_state(&state_file(conversation_id)?)
}

/// Record a model / effort PADE itself put in place (at launch, or by a live
/// switch) as the session's newest report.
fn record_choice(conversation_id: &str, choice: &ModelChoice) {
    let Some(path) = state_file(conversation_id) else {
        return;
    };
    let now = now_millis();
    let mut state = read_state(&path).unwrap_or_default();
    if let Some(model) = &choice.model {
        state.model = Some(model.clone());
        state.model_at = now;
    }
    if let Some(effort) = &choice.effort {
        state.effort = Some(effort.clone());
        state.effort_at = now;
    }
    if let Err(error) = write_state(&path, &state) {
        eprintln!("pade: could not record the model choice: {error}");
    }
}

/// Write through a sibling file and rename, so a reader never sees half a file.
fn write_state(path: &Path, state: &SessionState) -> std::io::Result<()> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)?;
    }
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, serde_json::to_vec(state)?)?;
    std::fs::rename(partial, path)
}

fn now_millis() -> u64 {
    millis_since_epoch(SystemTime::now())
}

fn modified_millis(path: &Path) -> Option<u64> {
    Some(millis_since_epoch(
        std::fs::metadata(path).ok()?.modified().ok()?,
    ))
}

fn millis_since_epoch(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |since| {
        u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
    })
}

/// Milliseconds since the epoch for an RFC 3339 UTC timestamp as agents write
/// them (`2026-10-12T01:28:38.123Z`). Hand-rolled to keep a date crate out.
fn timestamp_millis(timestamp: &str) -> Option<u64> {
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

/// Days from 1970-01-01 to a calendar date, using Howard Hinnant's
/// `days_from_civil` algorithm (years start in March, so leap days fall last).
fn days_since_epoch(year: i64, month: i64, day: i64) -> i64 {
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Keeps the watch alive for the app's lifetime; dropping it ends the watch.
struct ModelWatch {
    _watcher: Mutex<RecommendedWatcher>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangedPayload {
    session_id: Option<String>,
}

/// What one watched place is, so a changed path can be told apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchedPlace {
    /// PADE's own per-session state files.
    SessionStates,
    /// A settings file every session of an agent reads.
    SharedSettings,
    /// An agent's own per-session folders (Copilot's session-state).
    SessionFolders,
}

impl WatchedPlace {
    /// Only per-session folders nest; everything else is one flat directory.
    fn recursive_mode(self) -> RecursiveMode {
        if self == Self::SessionFolders {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        }
    }
}

/// Every directory to watch: PADE's own state files, each agent's settings
/// folder, and Copilot's session folders.
fn watched_places(session_states: PathBuf) -> Vec<(PathBuf, WatchedPlace)> {
    let mut places = vec![(session_states, WatchedPlace::SessionStates)];
    let settings_directories = [claude::settings_directory(), codex::settings_directory()];
    for directory in settings_directories.into_iter().flatten() {
        places.push((directory, WatchedPlace::SharedSettings));
    }
    if let Some(root) = copilot::session_root() {
        places.push((root, WatchedPlace::SessionFolders));
    }
    places
}

/// Which session a changed path speaks for, or `None` when it is noise.
fn changed_payload(path: &Path, places: &[(PathBuf, WatchedPlace)]) -> Option<ChangedPayload> {
    let name = path.file_name()?.to_str()?;
    let place = places
        .iter()
        .find(|(directory, _)| path.starts_with(directory))
        .map(|(_, place)| *place)?;
    match place {
        WatchedPlace::SessionStates => {
            let session_id = name.strip_suffix(".json")?;
            Some(ChangedPayload {
                session_id: Some(session_id.to_string()),
            })
        }
        WatchedPlace::SharedSettings => {
            let is_settings = claude::is_settings_file(name) || codex::is_settings_file(name);
            is_settings.then_some(ChangedPayload { session_id: None })
        }
        WatchedPlace::SessionFolders => {
            let session_id = copilot::session_of_event_log(path)?;
            Some(ChangedPayload {
                session_id: Some(session_id),
            })
        }
    }
}

fn prune_expired(directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let is_expired = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STATE_RETENTION);
        if is_expired {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        days_since_epoch, fold, launch_arguments, newer, timestamp_millis, ModelChoice, Saved,
        SessionState,
    };

    fn choice(model: Option<&str>, effort: Option<&str>) -> ModelChoice {
        ModelChoice {
            model: model.map(str::to_string),
            effort: effort.map(str::to_string),
        }
    }

    fn report(json: &str) -> serde_json::Value {
        serde_json::from_str(json).expect("a valid report")
    }

    #[test]
    fn each_agent_gets_its_own_launch_flags_for_a_choice() {
        let picked = choice(Some("claude-sonnet-5-5"), Some("low"));
        assert_eq!(
            launch_arguments("claude", &picked),
            ["--model", "claude-sonnet-5-5", "--effort", "low"]
        );
        assert_eq!(
            launch_arguments("codex", &choice(Some("gpt-5.6-terra"), Some("high"))),
            [
                "-c",
                "model=\"gpt-5.6-terra\"",
                "-c",
                "model_reasoning_effort=\"high\""
            ]
        );
        assert_eq!(
            launch_arguments(
                "opencode",
                &choice(Some("openai/gpt-5.6-sol"), Some("high"))
            ),
            ["-m", "openai/gpt-5.6-sol"]
        );
        assert_eq!(launch_arguments("aider", &picked), Vec::<String>::new());
    }

    #[test]
    fn only_plain_tokens_make_a_well_formed_choice() {
        assert!(choice(Some("openai/gpt-5.6-sol"), Some("xhigh")).is_well_formed());
        assert!(choice(Some("opus[1m]"), None).is_well_formed());
        assert!(!choice(Some("opus; rm -rf"), None).is_well_formed());
        assert!(!choice(None, Some("")).is_well_formed());
    }

    #[test]
    fn a_report_records_the_agents_own_session_id() {
        let state = fold(
            SessionState::default(),
            &serde_json::json!({ "session_id": "01a12d15", "model": "gpt-5.6-terra" }),
            10,
        );
        assert_eq!(state.agent_session.as_deref(), Some("01a12d15"));
    }

    #[test]
    fn a_start_report_names_the_model_and_transcript() {
        let state = fold(
            SessionState::default(),
            &report(
                r#"{"hook_event_name":"SessionStart","model":"gpt-5.6-terra","transcript_path":"C:\\x.jsonl"}"#,
            ),
            10,
        );
        assert_eq!(state.model.as_deref(), Some("gpt-5.6-terra"));
        assert_eq!(state.model_at, 10);
        assert_eq!(state.transcript.as_deref(), Some("C:\\x.jsonl"));
    }

    #[test]
    fn a_model_switch_reports_the_new_model() {
        let state = fold(
            SessionState::default(),
            &report(r#"{"from_model":"claude-opus-5","to_model":"claude-sonnet-5-5"}"#),
            20,
        );
        assert_eq!(state.model.as_deref(), Some("claude-sonnet-5-5"));
    }

    #[test]
    fn re_reporting_the_same_model_keeps_its_original_time() {
        let launched = SessionState {
            model: Some("claude-opus-5-5".to_string()),
            model_at: 10,
            ..SessionState::default()
        };
        let state = fold(launched, &report(r#"{"model":"claude-opus-5-5"}"#), 50);
        assert_eq!(state.model_at, 10);
    }

    #[test]
    fn a_turn_report_sets_the_effort_and_keeps_the_model() {
        let started = SessionState {
            model: Some("claude-opus-5".to_string()),
            model_at: 10,
            ..SessionState::default()
        };
        let state = fold(started, &report(r#"{"effort":{"level":"low"}}"#), 30);
        assert_eq!(state.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(state.effort.as_deref(), Some("low"));
        assert_eq!(state.effort_at, 30);
    }

    #[test]
    fn with_nothing_reported_the_saved_value_speaks() {
        let saved = Saved {
            value: "high".to_string(),
            saved_at: 5,
        };
        assert_eq!(newer(None, Some(&saved), 0).as_deref(), Some("high"));
    }

    #[test]
    fn a_save_after_the_last_report_wins() {
        let saved = Saved {
            value: "low".to_string(),
            saved_at: 25,
        };
        assert_eq!(
            newer(Some(("high", 20)), Some(&saved), 0).as_deref(),
            Some("low")
        );
    }

    #[test]
    fn a_report_after_the_save_wins() {
        let saved = Saved {
            value: "low".to_string(),
            saved_at: 25,
        };
        assert_eq!(
            newer(Some(("medium", 30)), Some(&saved), 0).as_deref(),
            Some("medium")
        );
    }

    #[test]
    fn a_model_switch_dates_the_saved_level_to_the_switch() {
        let saved = Saved {
            value: "low".to_string(),
            saved_at: 5,
        };
        assert_eq!(
            newer(Some(("high", 30)), Some(&saved), 40).as_deref(),
            Some("low")
        );
    }

    #[test]
    fn reads_an_agent_timestamp() {
        assert_eq!(timestamp_millis("1970-01-01T00:00:01.500Z"), Some(1_500));
        assert_eq!(
            timestamp_millis("2026-10-11T22:28:21.681Z"),
            Some(1_791_757_701_681)
        );
        assert_eq!(
            timestamp_millis("2000-03-01T00:00:00Z"),
            Some(951_868_800_000)
        );
        assert_eq!(timestamp_millis("not a time"), None);
    }

    #[test]
    fn counts_days_from_the_epoch_across_leap_days() {
        assert_eq!(days_since_epoch(1970, 1, 1), 0);
        assert_eq!(days_since_epoch(2000, 2, 29), 11_016);
        assert_eq!(days_since_epoch(2000, 3, 1), 11_017);
    }
}
