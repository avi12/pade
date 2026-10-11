//! The model and reasoning effort each agent session PADE launched is running,
//! learned from the agent itself — never read off its terminal.
//!
//! Every agent exposes this differently, so each has an adapter that knows how
//! to (a) prepare a launch so the agent reports and (b) read what it reported:
//!
//!   * Claude — hooks inside PADE's one `--settings` layer (`claude`);
//!   * Codex — trusted hooks passed with `-c`, plus its per-turn rollout and
//!     `config.toml` (`codex`);
//!   * Copilot — its own per-session event log, the id pinned at launch
//!     (`copilot`);
//!   * `OpenCode` — a per-launch plugin selected through its env (`opencode`).
//!
//! Hooks run PADE's own executable as a sink (`--agent-hook-sink`). It folds
//! the report into a small per-session file named by the conversation id PADE
//! hands every launch in `PADE_CONVERSATION_ID`, and exits without opening a
//! window. A watch on those files — and on the settings files where `/model`
//! and `/effort` save their choice — tells the windows the moment a session's
//! model or effort may have moved.
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

/// A session's model and effort as the agent reported them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningModel {
    model: String,
    effort: Option<String>,
}

/// The model and effort `command`'s session `conversation_id` is running, or
/// `None` when the agent has reported nothing PADE can read. Off the UI thread:
/// it reads files.
#[tauri::command]
pub async fn agent_running_model(command: String, conversation_id: String) -> Option<RunningModel> {
    tauri::async_runtime::spawn_blocking(move || read(&command, &conversation_id))
        .await
        .ok()
        .flatten()
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

/// Prepare a launch of `program` so it reports its model: adapter arguments
/// are added to `arguments`, and the environment to set is returned.
pub fn attach(
    program: &str,
    conversation_id: Option<&str>,
    arguments: &mut Vec<String>,
) -> Vec<(String, String)> {
    let Some(conversation_id) = conversation_id.filter(|id| is_plain_id(id)) else {
        return Vec::new();
    };

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

/// What hooks and plugins have reported for one session, each value stamped
/// with when it was reported (milliseconds since the epoch).
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SessionState {
    model: Option<String>,
    model_at: u64,
    effort: Option<String>,
    effort_at: u64,
    /// The agent's own transcript for this session, when its hooks name one.
    transcript: Option<String>,
}

/// A value the agent saved to a settings file, dated by that file's last write.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Saved {
    value: String,
    saved_at: u64,
}

/// The newer of a reported value and a saved one. `saved_from` is the earliest
/// moment the saved value can speak for this session — a model switch, say,
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

/// Run as the hook sink when PADE was launched as one, returning whether it
/// did — the caller must then exit instead of starting the app. A failure goes
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

fn record_hook_report() -> Result<(), String> {
    let mut stdin = String::new();
    std::io::stdin()
        .read_to_string(&mut stdin)
        .map_err(|error| error.to_string())?;
    let report: serde_json::Value =
        serde_json::from_str(&stdin).map_err(|error| error.to_string())?;
    let conversation_id = std::env::var(CONVERSATION_VARIABLE)
        .ok()
        .or_else(|| report.get("session_id")?.as_str().map(str::to_string))
        .ok_or("the report names no session")?;
    let path = state_file(&conversation_id).ok_or("no PADE data directory")?;
    let next = fold(read_state(&path).unwrap_or_default(), &report, now_millis());
    write_state(&path, &next).map_err(|error| error.to_string())
}

/// Fold one hook report into a session's state. The fields are the ones Claude
/// and Codex hooks share: `model` (or `to_model` on a model switch), an
/// `effort.level`, and the `transcript_path`.
fn fold(state: SessionState, report: &serde_json::Value, now: u64) -> SessionState {
    let text = |pointer: &str| report.pointer(pointer).and_then(serde_json::Value::as_str);
    let reported_model = text("/to_model").or_else(|| text("/model"));
    let reported_effort = text("/effort/level");

    let mut next = state;
    if let Some(model) = reported_model {
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
    next
}

/// The shell an agent runs its hook commands through — which decides how a
/// quoted executable path must be invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HookShell {
    /// A POSIX shell (Claude on Windows uses Git Bash): `"path" args`.
    Posix,
    /// `PowerShell` (Codex on Windows): a quoted path is only a string there, so
    /// it takes the call operator — `& 'path' args`.
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
    let mut date_parts = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (
        date_parts.next()?.ok()?,
        date_parts.next()?.ok()?,
        date_parts.next()?.ok()?,
    );
    let (clock, fraction) = time.split_once('.').unwrap_or((time, "0"));
    let mut clock_parts = clock.split(':').map(str::parse::<i64>);
    let (hour, minute, second) = (
        clock_parts.next()?.ok()?,
        clock_parts.next()?.ok()?,
        clock_parts.next()?.ok()?,
    );
    let milliseconds: i64 = format!("{fraction:0<3}").get(..3)?.parse().ok()?;

    // Days from the civil date (Howard Hinnant's algorithm).
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;

    let total = ((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + milliseconds;
    u64::try_from(total).ok()
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

    let places: Vec<(PathBuf, WatchedPlace)> =
        std::iter::once((states, WatchedPlace::SessionStates))
            .chain(
                [claude::settings_directory(), codex::settings_directory()]
                    .into_iter()
                    .flatten()
                    .map(|directory| (directory, WatchedPlace::SharedSettings)),
            )
            .chain(copilot::session_root().map(|root| (root, WatchedPlace::SessionFolders)))
            .collect();

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
        let mode = if *place == WatchedPlace::SessionFolders {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        // Best-effort per place: an agent that isn't installed has no folder.
        let _ = watcher.watch(directory, mode);
    }
    app.manage(ModelWatch {
        _watcher: Mutex::new(watcher),
    });
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
        let expired = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STATE_RETENTION);
        if expired {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fold, newer, timestamp_millis, Saved, SessionState};

    fn report(json: &str) -> serde_json::Value {
        serde_json::from_str(json).expect("a valid report")
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
}
