//! The model and effort each PADE-launched Claude session is running, reported
//! by Claude itself rather than read off its terminal.
//!
//! Claude's session log only learns the model once a reply has been written,
//! and a `/model` or `/effort` switch made between replies leaves it stale. So
//! every Claude session PADE launches carries three hooks — inside the one
//! `--settings` PADE already passes, because Claude honours only the LAST
//! `--settings` flag and a second one would silently drop the theme:
//!
//!   * `SessionStart` names the model before any prompt;
//!   * `PostModelSwitch` names the new one the moment `/model` runs;
//!   * `Stop` carries the effort the turn actually ran at.
//!
//! Each hook runs PADE's own executable as a sink (`--claude-hook-sink`), which
//! folds the event into a small per-session file and exits without opening a
//! window. No hook fires on `/effort`, but `/effort` immediately saves the
//! level for the current model into the user's settings — so the effort is the
//! newer of the last `Stop` report and that saved per-model level.
//!
//! All of it was measured against Claude Code 2.1.292 before being relied on.

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

use crate::agents::ID_CLAUDE;

/// The first argument that turns `pade.exe` into the hook sink.
const HOOK_SINK_FLAG: &str = "--claude-hook-sink";
/// Tells every window a session's model or effort may have changed. The payload
/// names the session, or is null when the user's settings changed (which can
/// move the effort of any session).
const CHANGED_EVENT: &str = "claude-state://changed";
/// The flag that hands Claude a settings layer for this launch alone.
const SETTINGS_FLAG: &str = "--settings";
/// Claude's own settings file, where `/model` and `/effort` save their choice.
const USER_SETTINGS_FILE: &str = "settings.json";
/// A session file outlives its session so a resumed conversation still has its
/// model; past this age nobody will resume it.
const STATE_RETENTION: Duration = Duration::from_hours(14 * 24);

/// The hook events PADE registers. Their names are Claude's wire names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
enum HookEvent {
    SessionStart,
    PostModelSwitch,
    Stop,
}

impl HookEvent {
    const ALL: [Self; 3] = [Self::SessionStart, Self::PostModelSwitch, Self::Stop];

    fn as_str(self) -> &'static str {
        match self {
            Self::SessionStart => "SessionStart",
            Self::PostModelSwitch => "PostModelSwitch",
            Self::Stop => "Stop",
        }
    }
}

/// The part of a hook's stdin PADE reads.
#[derive(Debug, Deserialize)]
struct HookInput {
    session_id: String,
    hook_event_name: HookEvent,
    /// `SessionStart` only.
    model: Option<String>,
    /// `PostModelSwitch` only.
    to_model: Option<String>,
    /// Turn events, when the model takes an effort level.
    effort: Option<HookEffort>,
}

#[derive(Debug, Deserialize)]
struct HookEffort {
    level: String,
}

/// What the hooks have reported for one session, each value stamped with when
/// it was reported (milliseconds since the epoch).
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionState {
    model: Option<String>,
    model_at: u64,
    effort: Option<String>,
    effort_at: u64,
}

/// Fold one hook report into a session's state.
fn apply(state: SessionState, input: &HookInput, now: u64) -> SessionState {
    let reported_model = match input.hook_event_name {
        HookEvent::SessionStart => input.model.clone(),
        HookEvent::PostModelSwitch => input.to_model.clone(),
        HookEvent::Stop => None,
    };
    let reported_effort = input.effort.as_ref().map(|effort| effort.level.clone());

    let mut next = state;
    if let Some(model) = reported_model {
        next.model = Some(model);
        next.model_at = now;
    }
    if let Some(effort) = reported_effort {
        next.effort = Some(effort);
        next.effort_at = now;
    }
    next
}

/// The effort level the user's settings hold for `model`, with when that file
/// was last written.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SavedEffort {
    level: String,
    saved_at: u64,
}

/// The effort a session runs at: the newer of what its last turn reported and
/// what the settings hold for its current model. A model switch dates the saved
/// level to the switch, because switching is exactly when Claude adopts the new
/// model's saved level.
fn resolve_effort(state: &SessionState, saved: Option<&SavedEffort>) -> Option<String> {
    let Some(saved) = saved else {
        return state.effort.clone();
    };
    let saved_applies_from = saved.saved_at.max(state.model_at);
    let reported_is_newer = state.effort.is_some() && state.effort_at >= saved_applies_from;
    if reported_is_newer {
        return state.effort.clone();
    }
    Some(saved.level.clone())
}

/// The model and effort a PADE-launched Claude session is running, or `None`
/// when no hook has reported for it (a session started before this existed).
pub fn running_model(conversation_id: &str) -> Option<(String, Option<String>)> {
    let state = read_state(&state_file(conversation_id)?)?;
    let model = state.model.clone()?;
    let saved = saved_effort(&model);
    Some((model, resolve_effort(&state, saved.as_ref())))
}

/// Run as the hook sink when PADE was launched as one, returning whether it
/// did — the caller must then exit instead of starting the app. A failure is
/// reported on stderr, which Claude shows in its hook log; the hook is async,
/// so it never holds up the session.
pub fn run_hook_sink_if_requested() -> bool {
    let is_sink = std::env::args().nth(1).as_deref() == Some(HOOK_SINK_FLAG);
    if !is_sink {
        return false;
    }

    if let Err(error) = record_hook_report() {
        eprintln!("pade: could not record the Claude hook report: {error}");
    }
    true
}

fn record_hook_report() -> Result<(), String> {
    let mut stdin = String::new();
    std::io::stdin()
        .read_to_string(&mut stdin)
        .map_err(|error| error.to_string())?;
    let input: HookInput = serde_json::from_str(&stdin).map_err(|error| error.to_string())?;
    let path = state_file(&input.session_id).ok_or("no PADE data directory")?;
    let previous = read_state(&path).unwrap_or_default();
    let next = apply(previous, &input, now_millis());
    write_state(&path, &next).map_err(|error| error.to_string())
}

/// Give a Claude launch the hooks. They join the `--settings` layer already in
/// `arguments` (the live theme), or form one when there is none.
pub fn attach_hooks(program: &str, arguments: &mut Vec<String>) {
    if program != ID_CLAUDE {
        return;
    }
    let Some(hooks) = hook_settings() else {
        return;
    };

    let settings_index = arguments
        .iter()
        .position(|argument| argument == SETTINGS_FLAG)
        .map(|index| index + 1)
        .filter(|&index| index < arguments.len());
    let Some(index) = settings_index else {
        arguments.push(SETTINGS_FLAG.to_string());
        arguments.push(serde_json::Value::Object(hooks).to_string());
        return;
    };

    arguments[index] = merge_settings(&arguments[index], hooks);
}

/// `base` (a settings JSON string) with `hooks` added; `base` unchanged when it
/// isn't a JSON object, since then it is a file path Claude will read itself.
fn merge_settings(base: &str, hooks: serde_json::Map<String, serde_json::Value>) -> String {
    let Ok(serde_json::Value::Object(mut settings)) = serde_json::from_str(base) else {
        return base.to_string();
    };
    settings.extend(hooks);
    serde_json::Value::Object(settings).to_string()
}

/// `{"hooks": {...}}` routing every reported event to this executable.
fn hook_settings() -> Option<serde_json::Map<String, serde_json::Value>> {
    let executable = std::env::current_exe().ok()?;
    // Claude runs hooks through Git Bash when it is installed, which reads a
    // backslash as an escape — forward slashes work under both shells.
    let command = format!(
        "\"{}\" {HOOK_SINK_FLAG}",
        executable.to_string_lossy().replace('\\', "/")
    );
    let handler = serde_json::json!([{ "hooks": [{ "type": "command", "command": command, "async": true }] }]);
    let events = HookEvent::ALL
        .iter()
        .map(|event| (event.as_str().to_string(), handler.clone()))
        .collect();

    let mut settings = serde_json::Map::new();
    settings.insert("hooks".to_string(), serde_json::Value::Object(events));
    Some(settings)
}

/// Where the sink keeps one file per session.
fn state_directory() -> Option<PathBuf> {
    Some(crate::util::pade_data_dir()?.join("claude-sessions"))
}

fn state_file(conversation_id: &str) -> Option<PathBuf> {
    let is_plain_id = !conversation_id.is_empty()
        && conversation_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-');
    if !is_plain_id {
        return None;
    }
    Some(state_directory()?.join(format!("{conversation_id}.json")))
}

fn read_state(path: &Path) -> Option<SessionState> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
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

fn user_settings_path() -> Option<PathBuf> {
    Some(
        crate::util::home_dir()?
            .join(".claude")
            .join(USER_SETTINGS_FILE),
    )
}

/// The effort the user's settings hold for `model`: its own
/// `modelSettings.<model>.effortLevel`, else the global `effortLevel`.
fn saved_effort(model: &str) -> Option<SavedEffort> {
    let path = user_settings_path()?;
    let saved_at = modified_millis(&path)?;
    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
    let level = effort_for_model(&settings, model)?;
    Some(SavedEffort { level, saved_at })
}

fn effort_for_model(settings: &serde_json::Value, model: &str) -> Option<String> {
    settings
        .get("modelSettings")
        .and_then(|models| models.get(model))
        .and_then(|entry| entry.get("effortLevel"))
        .or_else(|| settings.get("effortLevel"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
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

/// Keeps the watch alive for the app's lifetime; dropping it ends the watch.
struct StateWatch {
    _watcher: Mutex<RecommendedWatcher>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangedPayload {
    session_id: Option<String>,
}

/// Watch the session files and the user's settings, telling the windows the
/// moment a session's model or effort may have moved. Best-effort: without the
/// watch a tab still catches up when its session next goes idle.
pub fn init(app: &AppHandle) {
    let Some(directory) = state_directory() else {
        return;
    };
    if let Err(error) = std::fs::create_dir_all(&directory) {
        eprintln!("pade: no Claude session-state directory: {error}");
        return;
    }
    prune_expired(&directory);

    let emitter = app.clone();
    let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        for path in &event.paths {
            if let Some(payload) = changed_payload(path) {
                let _ = emitter.emit(CHANGED_EVENT, payload);
            }
        }
    });
    let Ok(mut watcher) = watcher else {
        return;
    };
    if let Err(error) = watcher.watch(&directory, RecursiveMode::NonRecursive) {
        eprintln!("pade: could not watch Claude session state: {error}");
        return;
    }
    if let Some(settings_directory) = user_settings_path().as_deref().and_then(Path::parent) {
        // Best-effort: without it an `/effort` switch shows after the next turn.
        let _ = watcher.watch(settings_directory, RecursiveMode::NonRecursive);
    }
    app.manage(StateWatch {
        _watcher: Mutex::new(watcher),
    });
}

/// Which session a changed path speaks for: a session file names its own, the
/// user's settings speak for every session, and anything else is noise.
fn changed_payload(path: &Path) -> Option<ChangedPayload> {
    let name = path.file_name()?.to_str()?;
    if name == USER_SETTINGS_FILE {
        return Some(ChangedPayload { session_id: None });
    }
    let session_id = name.strip_suffix(".json")?;
    let in_state_directory = path.parent() == state_directory().as_deref();
    in_state_directory.then(|| ChangedPayload {
        session_id: Some(session_id.to_string()),
    })
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
    use super::{
        apply, effort_for_model, merge_settings, resolve_effort, HookInput, SavedEffort,
        SessionState,
    };

    fn hook(json: &str) -> HookInput {
        serde_json::from_str(json).expect("a valid hook payload")
    }

    #[test]
    fn session_start_names_the_model() {
        let state = apply(
            SessionState::default(),
            &hook(
                r#"{"session_id":"a","hook_event_name":"SessionStart","source":"startup","model":"claude-opus-5"}"#,
            ),
            10,
        );
        assert_eq!(state.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(state.model_at, 10);
    }

    #[test]
    fn a_model_switch_replaces_the_model() {
        let started = SessionState {
            model: Some("claude-opus-5".to_string()),
            model_at: 10,
            ..SessionState::default()
        };
        let switched = apply(
            started,
            &hook(
                r#"{"session_id":"a","hook_event_name":"PostModelSwitch","from_model":"claude-opus-5","to_model":"claude-sonnet-5-5"}"#,
            ),
            20,
        );
        assert_eq!(switched.model.as_deref(), Some("claude-sonnet-5-5"));
        assert_eq!(switched.model_at, 20);
    }

    #[test]
    fn a_finished_turn_reports_the_effort_and_keeps_the_model() {
        let started = SessionState {
            model: Some("claude-opus-5".to_string()),
            model_at: 10,
            ..SessionState::default()
        };
        let turned = apply(
            started,
            &hook(r#"{"session_id":"a","hook_event_name":"Stop","effort":{"level":"low"}}"#),
            30,
        );
        assert_eq!(turned.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(turned.effort.as_deref(), Some("low"));
        assert_eq!(turned.effort_at, 30);
    }

    #[test]
    fn before_any_turn_the_saved_level_is_the_effort() {
        let state = SessionState {
            model: Some("claude-opus-5".to_string()),
            model_at: 10,
            ..SessionState::default()
        };
        let saved = SavedEffort {
            level: "high".to_string(),
            saved_at: 5,
        };
        assert_eq!(
            resolve_effort(&state, Some(&saved)).as_deref(),
            Some("high")
        );
    }

    #[test]
    fn an_effort_switch_saved_after_the_last_turn_wins() {
        let state = SessionState {
            model: Some("claude-opus-5".to_string()),
            model_at: 10,
            effort: Some("high".to_string()),
            effort_at: 20,
        };
        let saved = SavedEffort {
            level: "low".to_string(),
            saved_at: 25,
        };
        assert_eq!(resolve_effort(&state, Some(&saved)).as_deref(), Some("low"));
    }

    #[test]
    fn the_turn_after_a_settings_edit_wins_over_it() {
        let state = SessionState {
            model: Some("claude-opus-5".to_string()),
            model_at: 10,
            effort: Some("medium".to_string()),
            effort_at: 30,
        };
        let saved = SavedEffort {
            level: "low".to_string(),
            saved_at: 25,
        };
        assert_eq!(
            resolve_effort(&state, Some(&saved)).as_deref(),
            Some("medium")
        );
    }

    #[test]
    fn a_model_switch_adopts_the_new_models_saved_level() {
        let state = SessionState {
            model: Some("claude-sonnet-5-5".to_string()),
            model_at: 40,
            effort: Some("high".to_string()),
            effort_at: 30,
        };
        let saved = SavedEffort {
            level: "low".to_string(),
            saved_at: 5,
        };
        assert_eq!(resolve_effort(&state, Some(&saved)).as_deref(), Some("low"));
    }

    #[test]
    fn a_models_own_level_outranks_the_global_one() {
        let settings = serde_json::json!({
            "effortLevel": "xhigh",
            "modelSettings": { "claude-opus-5": { "effortLevel": "high" } }
        });
        assert_eq!(
            effort_for_model(&settings, "claude-opus-5").as_deref(),
            Some("high")
        );
        assert_eq!(
            effort_for_model(&settings, "claude-fable-5-1").as_deref(),
            Some("xhigh")
        );
    }

    #[test]
    fn hooks_join_the_existing_settings_layer() {
        let mut hooks = serde_json::Map::new();
        hooks.insert("hooks".to_string(), serde_json::json!({}));
        let merged = merge_settings(r#"{"theme":"custom:pade"}"#, hooks);
        let value: serde_json::Value = serde_json::from_str(&merged).expect("valid JSON");
        assert_eq!(value["theme"], "custom:pade");
        assert!(value.get("hooks").is_some());
    }
}
