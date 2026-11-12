//! Claude Code adapter: reports through hooks, switches live by typing.
//!
//! Reporting: the hooks join the one `--settings` JSON PADE already passes,
//! because Claude honours only the LAST `--settings` flag and a second one would
//! silently drop the live theme. `SessionStart` names the model before any
//! prompt, `PostModelSwitch` the new one the moment `/model` runs, and `Stop`
//! the effort a turn ran at. No hook fires on `/effort`, but the session's own
//! transcript logs the level it set, so the effort is the newer of the last
//! `Stop` report and that logged level. The level saved in
//! `~/.claude/settings.json` is shared by every session (and rewritten whenever
//! PADE puts the user's default back), so it only stands in until the session
//! reports, and from a model switch on.
//!
//! Switching: PADE types `/model` and `/effort` into the session, waits until
//! Claude saves each value to its settings file, then puts the user's saved
//! default back so only this session changed.
//!
//! Measured against Claude Code 2.1.292.

mod catalog;
mod switch;

use std::cell::OnceCell;
use std::path::{Path, PathBuf};

use super::{
    hook_command, newer, pacing_control, read_session_state, state_directory,
    timestamp_milliseconds, EffortOption, HookShell, ModelChoice, ModelOption, RunningModel, Saved,
    SessionState,
};

use catalog::newest_catalog;
pub(super) use catalog::options;
pub(super) use switch::{apply_live, has_switch_permission};

/// The flag that hands Claude a settings layer for this launch alone.
const SETTINGS_FLAG: &str = "--settings";
/// Claude's own user settings, where `/model` and `/effort` save their choice.
const SETTINGS_FILE: &str = "settings.json";
/// The settings keys `/model` and `/effort` save their choice under: the
/// model, the per-model table, and the level inside it (or at the top level).
const MODEL_KEY: &str = "model";
const MODEL_SETTINGS_KEY: &str = "modelSettings";
const EFFORT_LEVEL_KEY: &str = "effortLevel";
/// The hook events that only report, by Claude's wire names - run async, so a
/// hook never holds a turn up.
/// `UserPromptSubmit` reports nothing new itself; its write is what makes the tab
/// re-read right after a prompt, when Claude records whether Ultracode is on.
const REPORTING_EVENTS: [&str; 4] = [
    "SessionStart",
    "UserPromptSubmit",
    "PostModelSwitch",
    "Stop",
];
/// The transcript entries Claude writes when Ultracode turns on (repeated every
/// few turns while it stays on) and off. Only written as a prompt is sent.
const ULTRACODE_ENTER: &str = "ultra_effort_enter";
const ULTRACODE_EXIT: &str = "ultra_effort_exit";
/// What `/effort <level>` logs to the transcript once it took the level.
const EFFORT_SET_OUTPUT: &str = "<local-command-stdout>Set effort level to ";
/// Claude asks this hook before switching models; its answer can skip the
/// "Switch model?" confirmation, so it runs in step (not async).
pub(super) const PRE_MODEL_SWITCH: &str = "PreModelSwitch";
/// The answer that lets a switch PADE started go ahead without the prompt.
pub(super) const ALLOW_MODEL_SWITCH: &str =
    r#"{"hookSpecificOutput":{"hookEventName":"PreModelSwitch","permissionDecision":"allow"}}"#;

/// Join the hooks to the `--settings` layer in `arguments` (the live theme),
/// or form one when there is none.
pub(super) fn attach(arguments: &mut Vec<String>) {
    let Some(hooks) = hook_settings() else {
        return;
    };
    let Some(index) = settings_value_index(arguments) else {
        arguments.push(SETTINGS_FLAG.to_string());
        arguments.push(serde_json::Value::Object(hooks).to_string());
        return;
    };
    arguments[index] = merge_settings(&arguments[index], hooks);
}

/// The session's model from its hook reports, and its effort as the newest of
/// the last report and the last `/effort` its transcript logs - or the level
/// saved for the model, before either and from a model switch on.
pub(super) fn read(conversation_id: &str) -> Option<RunningModel> {
    Session::load(conversation_id).running_model()
}

/// What `read` answers, plus whether Ultracode is on - from one read of the
/// session's state and one lookup of its transcript.
pub(super) fn read_with_ultracode(conversation_id: &str) -> Option<(RunningModel, bool)> {
    let session = Session::load(conversation_id);
    let running = session.running_model()?;
    Some((running, session.is_ultracode_on()))
}

/// One session's hook state and transcript, each read at most once however
/// many questions are asked of them.
struct Session<'a> {
    conversation_id: &'a str,
    state: Option<SessionState>,
    transcript: OnceCell<Option<PathBuf>>,
}

impl<'a> Session<'a> {
    fn load(conversation_id: &'a str) -> Self {
        Self {
            conversation_id,
            state: read_session_state(conversation_id),
            transcript: OnceCell::new(),
        }
    }

    /// The session's transcript: the one its hooks named, else found by id.
    fn transcript(&self) -> Option<&Path> {
        self.transcript
            .get_or_init(|| {
                self.state
                    .as_ref()
                    .and_then(|state| state.transcript.as_ref())
                    .map(PathBuf::from)
                    .or_else(|| {
                        crate::agents::session_context::claude_transcript(self.conversation_id)
                    })
            })
            .as_deref()
    }

    fn running_model(&self) -> Option<RunningModel> {
        let Some((state, model)) = self
            .state
            .as_ref()
            .and_then(|state| Some((state, state.model.as_ref()?)))
        else {
            return logged_turn(self.conversation_id);
        };

        let reported = state
            .effort
            .as_deref()
            .map(|effort| (effort, state.effort_at));
        let typed = self.typed_effort();
        let typed = typed.as_ref().map(|(effort, at)| (effort.as_str(), *at));
        let session_effort = [reported, typed]
            .into_iter()
            .flatten()
            .max_by_key(|(_, at)| *at);
        let saved = saved_effort(model);
        Some(RunningModel {
            effort: newer(session_effort, saved.as_ref(), state.model_at),
            model: model.clone(),
        })
    }

    /// The level the session's newest `/effort` set, and when.
    fn typed_effort(&self) -> Option<(String, u64)> {
        crate::util::newest_in_tail(self.transcript()?, newest_effort_set)
    }

    /// Whether Ultracode is on for the session: the newest Ultracode entry in
    /// its transcript is an enter (Claude Code uses the same rule to decide
    /// when to write the exit). No entry means it was never turned on.
    fn is_ultracode_on(&self) -> bool {
        let Some(transcript) = self.transcript() else {
            return false;
        };
        crate::util::newest_in_tail(transcript, newest_ultracode_entry)
            .is_some_and(|entry| entry == ULTRACODE_ENTER)
    }
}

fn newest_effort_set(lines: &str) -> Option<(String, u64)> {
    lines.lines().rev().find_map(|line| {
        if !line.contains(EFFORT_SET_OUTPUT) {
            return None;
        }
        let entry: serde_json::Value = serde_json::from_str(line).ok()?;
        let output = entry.pointer("/message/content")?.as_str()?;
        let level = output
            .strip_prefix(EFFORT_SET_OUTPUT)?
            .split([' ', '<'])
            .next()
            .filter(|level| !level.is_empty())?;
        let set_at = timestamp_milliseconds(entry.get("timestamp")?.as_str()?)?;
        Some((level.to_string(), set_at))
    })
}

pub(super) fn settings_directory() -> Option<PathBuf> {
    crate::agents::claude_directory()
}

pub(super) fn is_settings_file(name: &str) -> bool {
    name == SETTINGS_FILE
}

fn settings_file() -> Option<PathBuf> {
    Some(settings_directory()?.join(SETTINGS_FILE))
}

/// Where the JSON value after `--settings` sits in `arguments`, if it is there.
fn settings_value_index(arguments: &[String]) -> Option<usize> {
    let flag_index = arguments
        .iter()
        .position(|argument| argument == SETTINGS_FLAG)?;
    let value_index = flag_index + 1;
    (value_index < arguments.len()).then_some(value_index)
}

/// `base` (a settings JSON string) with `hooks` added; `base` unchanged when it
/// isn't a JSON object, since then it is a file path Claude reads itself.
fn merge_settings(base: &str, hooks: serde_json::Map<String, serde_json::Value>) -> String {
    let Ok(serde_json::Value::Object(mut settings)) = serde_json::from_str(base) else {
        return base.to_string();
    };
    settings.extend(hooks);
    serde_json::Value::Object(settings).to_string()
}

/// `{"hooks": {...}}` routing every event to the sink.
fn hook_settings() -> Option<serde_json::Map<String, serde_json::Value>> {
    let command = hook_command(HookShell::Posix)?;
    let handler = |is_async: bool| serde_json::json!([{ "hooks": [{ "type": "command", "command": command, "async": is_async }] }]);

    let mut events = serde_json::Map::new();
    for event in REPORTING_EVENTS {
        events.insert(event.to_string(), handler(true));
    }
    events.insert(PRE_MODEL_SWITCH.to_string(), handler(false));
    // In step and on every tool, because its answer is usage pacing's say over
    // the call - held, refused, or a spawn moved onto the cap
    // (`pacing_control`). Hooks are fixed at launch, so it is always there;
    // the sink says nothing while the session isn't paced.
    events.insert(
        pacing_control::PRE_TOOL_USE.to_string(),
        serde_json::json!([{
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": pacing_control::PRE_TOOL_USE_TIMEOUT_SECONDS
            }]
        }]),
    );

    let mut settings = serde_json::Map::new();
    settings.insert("hooks".to_string(), serde_json::Value::Object(events));
    Some(settings)
}

/// Type the commands and wait for each to land; returns what now runs.
pub(super) fn read_settings() -> Option<serde_json::Value> {
    crate::util::read_json(&settings_file()?)
}

fn newest_ultracode_entry(lines: &str) -> Option<&'static str> {
    lines.lines().rev().find_map(|line| {
        let is_candidate = line.contains(ULTRACODE_ENTER) || line.contains(ULTRACODE_EXIT);
        if !is_candidate {
            return None;
        }
        let entry: serde_json::Value = serde_json::from_str(line).ok()?;
        match entry.pointer("/attachment/type")?.as_str()? {
            ULTRACODE_ENTER => Some(ULTRACODE_ENTER),
            ULTRACODE_EXIT => Some(ULTRACODE_EXIT),
            _ => None,
        }
    })
}

/// A session launched before PADE attached its hooks: the newest turn Claude's
/// own log records.
fn logged_turn(conversation_id: &str) -> Option<RunningModel> {
    let turn = crate::agents::session_context::logged_claude_turn(conversation_id)?;
    Some(RunningModel {
        model: turn.model,
        effort: turn.effort,
    })
}

/// The effort Claude starts `model` at when the session hasn't set one: the
/// level saved for it, else the catalog's default for it. Undated: any session,
/// or PADE restoring the default, rewrites the settings file, so its last write
/// says nothing about this session.
fn saved_effort(model: &str) -> Option<Saved> {
    let settings = read_settings().unwrap_or_default();
    Some(Saved {
        value: effort_for_model(&settings, newest_catalog().as_ref(), model)?,
        saved_at: 0,
    })
}

/// A model's own level is looked up under its exact id, then without a
/// context-window variant: `claude-opus-5-5[1m]` shares `claude-opus-5-5`'s.
/// Without one, a model the catalog knows runs at the catalog's default - the
/// user's top-level `effortLevel` is a legacy key Claude Code no longer applies
/// to current models (measured on 2.1.302), so it only stands in for a model
/// the catalog doesn't list.
fn effort_for_model(
    settings: &serde_json::Value,
    catalog: Option<&serde_json::Value>,
    model: &str,
) -> Option<String> {
    let model_settings = settings.get(MODEL_SETTINGS_KEY);
    let own_level = |id: &str| {
        model_settings
            .and_then(|models| models.get(id))
            .and_then(|entry| entry.get(EFFORT_LEVEL_KEY))
    };
    own_level(model)
        .or_else(|| own_level(without_context_variant(model)))
        .or_else(|| catalog_default_effort(catalog?, without_context_variant(model)))
        .or_else(|| settings.get(EFFORT_LEVEL_KEY))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// The level the catalog says `model` starts at.
fn catalog_default_effort<'a>(
    catalog: &'a serde_json::Value,
    model: &str,
) -> Option<&'a serde_json::Value> {
    catalog
        .pointer("/catalog/config/models")?
        .as_array()?
        .iter()
        .find(|entry| entry.get("id").and_then(serde_json::Value::as_str) == Some(model))?
        .pointer("/runtime/default_effort")
}

/// `claude-opus-5-5[1m]` → `claude-opus-5-5`.
fn without_context_variant(model: &str) -> &str {
    model
        .strip_suffix(']')
        .and_then(|rest| rest.rsplit_once('['))
        .map_or(model, |(base, _variant)| base)
}

#[cfg(test)]
mod tests;
