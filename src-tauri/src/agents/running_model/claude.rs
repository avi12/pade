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

use std::cell::OnceCell;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{
    hook_command, newer, read_session_state, record_choice, state_directory, subagent_cap,
    timestamp_milliseconds, EffortOption, HookShell, ModelChoice, ModelOption, RunningModel, Saved,
    SessionState,
};

/// The flag that hands Claude a settings layer for this launch alone.
const SETTINGS_FLAG: &str = "--settings";
/// Claude's own user settings, where `/model` and `/effort` save their choice.
const SETTINGS_FILE: &str = "settings.json";
/// The settings keys `/model` and `/effort` save their choice under: the
/// model, the per-model table, and the level inside it (or at the top level).
const MODEL_KEY: &str = "model";
const MODEL_SETTINGS_KEY: &str = "modelSettings";
const EFFORT_LEVEL_KEY: &str = "effortLevel";
const SWITCH_KEYS: [&str; 2] = [MODEL_KEY, MODEL_SETTINGS_KEY];
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
/// How long a switch permission PADE grants stays good - the switch it was
/// granted for lands well within this.
const SWITCH_PERMISSION_LIFETIME: Duration = Duration::from_secs(15);
const SWITCH_PERMISSION_SUFFIX: &str = ".switch-permission";

/// Ctrl+S: Claude stashes the draft in its input box and brings it back once
/// the next command has run - so a switch never mixes into half a prompt.
const STASH_DRAFT: &str = "\u{13}";
const SUBMIT: &str = "\r";
/// The pause between keystroke groups, so Claude reads typing as typing - a
/// command and its Enter sent as one burst arrive as a paste, whose Enter is a
/// newline rather than a submit.
const KEYSTROKE_SETTLE: Duration = Duration::from_millis(150);
/// How long a `/model` switch may take to land in the settings file. Claude can
/// check the account's access to the model first (a network round trip; Fable
/// measured past 1.5 s), so this is generous - polling returns the moment it lands.
const MODEL_LANDING: Duration = Duration::from_secs(8);
/// How long an `/effort` switch may take to land before PADE takes it to be
/// sitting on Claude's "Change effort level?" dialog and accepts it. Long
/// enough that a slow save is never mistaken for the dialog: an Enter sent
/// with no dialog up would reach the draft Claude restores.
const EFFORT_LANDING: Duration = Duration::from_secs(4);
/// How long the settings file must go unwritten before PADE restores it.
const SETTINGS_QUIET: Duration = Duration::from_millis(300);
const SETTINGS_QUIET_LIMIT: Duration = Duration::from_secs(3);
const LANDING_POLL: Duration = Duration::from_millis(50);

/// The catalog surface Claude Code's own `/model` picker reads.
const CATALOG_SURFACE_SUFFIX: &str = "-cc.json";
/// The badge Claude puts on the level it recommends for a model.
const RECOMMENDED_BADGE: &str = "Recommended";

/// Types a string into the session's terminal, as if from its keyboard.
type TypeKeys<'a> = &'a dyn Fn(&str) -> Result<(), String>;

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

/// Switch a running session live, as if typed: stash the draft, `/model`,
/// then stash again and `/effort`. Each step is confirmed by Claude saving the
/// value - never by reading the screen - and the user's saved default is put
/// back afterwards, so only this session changes. Works mid-turn: the next
/// request already uses the new model (verified on 2.1.292).
pub(super) fn apply_live(
    conversation_id: &str,
    choice: &ModelChoice,
    type_keys: TypeKeys,
) -> Result<(), String> {
    let settings = settings_file().ok_or("no Claude settings directory")?;
    let snapshot = std::fs::read_to_string(&settings).ok();
    let outcome = switch_by_commands(conversation_id, choice, type_keys);
    // Claude can write the file more than once per command; restoring between
    // two of its writes would let the second put the switch back.
    wait_for_quiet_settings();
    restore_saved_defaults(&settings, snapshot.as_deref())?;
    let applied = outcome?;
    // Newer than the default just restored, so the tab shows what now runs.
    record_choice(conversation_id, &applied);
    Ok(())
}

/// Whether PADE granted this session's pending switch - used up by asking.
pub(super) fn consume_switch_permission(conversation_id: &str) -> bool {
    let Some(path) = switch_permission_file(conversation_id) else {
        return false;
    };
    let is_fresh = std::fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|granted| SystemTime::now().duration_since(granted).ok())
        .is_some_and(|age| age <= SWITCH_PERMISSION_LIFETIME);
    let _ = std::fs::remove_file(&path);
    is_fresh
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

/// The models Claude Code itself lists, from the catalog it caches under
/// `~/.claude/cache/model-catalog` - names, effort levels with their labels,
/// and the recommended level, exactly as its picker shows them.
pub(super) fn options() -> Vec<ModelOption> {
    let Some(catalog) = newest_catalog() else {
        return Vec::new();
    };
    let Some(models) = catalog
        .pointer("/catalog/config/models")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    models.iter().filter_map(model_option).collect()
}

pub(super) fn settings_directory() -> Option<PathBuf> {
    Some(crate::util::home_directory()?.join(".claude"))
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
    // In step, because its answer is the spawn's model (`subagent_cap`).
    events.insert(
        subagent_cap::PRE_TOOL_USE.to_string(),
        serde_json::json!([{
            "matcher": subagent_cap::CLAUDE_SPAWN_TOOLS,
            "hooks": [{ "type": "command", "command": command }]
        }]),
    );

    let mut settings = serde_json::Map::new();
    settings.insert("hooks".to_string(), serde_json::Value::Object(events));
    Some(settings)
}

/// Type the commands and wait for each to land; returns what now runs.
fn switch_by_commands(
    conversation_id: &str,
    choice: &ModelChoice,
    type_keys: TypeKeys,
) -> Result<ModelChoice, String> {
    let mut current_model = read(conversation_id).map(|running| running.model);
    if let Some(model) = &choice.model {
        switch_model(conversation_id, model, type_keys)?;
        current_model = Some(model.clone());
    }
    if let Some(effort) = &choice.effort {
        let model = current_model
            .as_deref()
            .ok_or("the session's model isn't known yet")?;
        switch_effort(model, effort, type_keys)?;
    }
    Ok(ModelChoice {
        model: current_model,
        effort: choice.effort.clone(),
    })
}

fn switch_model(conversation_id: &str, model: &str, type_keys: TypeKeys) -> Result<(), String> {
    grant_switch_permission(conversation_id);
    let typed_at = SystemTime::now();
    run_command(type_keys, &format!("/model {model}"))?;
    // Saved SINCE typing, not merely equal: switching to the model a session
    // already runs matches before Claude has written anything.
    let has_landed = wait_until(MODEL_LANDING, || {
        is_settings_saved_since(typed_at) && saved_model().as_deref() == Some(model)
    });
    if !has_landed {
        return Err(format!("Claude didn't switch to {model}"));
    }
    Ok(())
}

/// `model` is the one the session runs now - `/effort` saves its level per model.
fn switch_effort(model: &str, effort: &str, type_keys: TypeKeys) -> Result<(), String> {
    let typed_at = SystemTime::now();
    let is_effort_saved = || {
        is_settings_saved_since(typed_at)
            && saved_effort(model).map(|saved| saved.value).as_deref() == Some(effort)
    };
    run_command(type_keys, &format!("/effort {effort}"))?;
    // No hook can pre-answer Claude's "Change effort level?" dialog; a level
    // that hasn't landed by now is sitting on it, and Enter accepts it.
    let has_landed = wait_until(EFFORT_LANDING, is_effort_saved) || {
        type_keys(SUBMIT)?;
        wait_until(EFFORT_LANDING, is_effort_saved)
    };
    if !has_landed {
        return Err(format!("Claude didn't switch to {effort} effort"));
    }
    Ok(())
}

fn run_command(type_keys: TypeKeys, command: &str) -> Result<(), String> {
    type_keys(STASH_DRAFT)?;
    std::thread::sleep(KEYSTROKE_SETTLE);
    type_keys(command)?;
    std::thread::sleep(KEYSTROKE_SETTLE);
    type_keys(SUBMIT)
}

fn settings_modified_at() -> Option<SystemTime> {
    std::fs::metadata(settings_file()?).ok()?.modified().ok()
}

fn is_settings_saved_since(moment: SystemTime) -> bool {
    settings_modified_at().is_some_and(|modified| modified > moment)
}

/// Wait until the settings file has gone `SETTINGS_QUIET` without a write
/// (at most `SETTINGS_QUIET_LIMIT`).
fn wait_for_quiet_settings() {
    let deadline = std::time::Instant::now() + SETTINGS_QUIET_LIMIT;
    let mut last_seen = settings_modified_at();
    while std::time::Instant::now() < deadline {
        std::thread::sleep(SETTINGS_QUIET);
        let now_seen = settings_modified_at();
        if now_seen == last_seen {
            return;
        }
        last_seen = now_seen;
    }
}

/// Poll `has_landed` until it holds or `within` runs out.
fn wait_until(within: Duration, has_landed: impl Fn() -> bool) -> bool {
    let deadline = std::time::Instant::now() + within;
    while std::time::Instant::now() < deadline {
        if has_landed() {
            return true;
        }
        std::thread::sleep(LANDING_POLL);
    }
    has_landed()
}

fn saved_model() -> Option<String> {
    let settings = read_settings()?;
    Some(settings.get(MODEL_KEY)?.as_str()?.to_string())
}

fn read_settings() -> Option<serde_json::Value> {
    crate::util::read_json(&settings_file()?)
}

/// Put back the default `/model` and `/effort` just saved over - the keys they
/// write, `model` and `modelSettings`. When nothing else changed meanwhile the
/// file goes back byte for byte; otherwise only those two keys are reverted.
fn restore_saved_defaults(settings: &Path, snapshot: Option<&str>) -> Result<(), String> {
    let Some(snapshot) = snapshot else {
        return Ok(());
    };
    let Ok(current) = std::fs::read_to_string(settings) else {
        return Ok(());
    };
    if current == snapshot {
        return Ok(());
    }
    let (Ok(before), Ok(mut after)) = (
        serde_json::from_str::<serde_json::Value>(snapshot),
        serde_json::from_str::<serde_json::Value>(&current),
    ) else {
        return Ok(());
    };
    revert_switch_keys(&before, &mut after);
    let restored = restored_text(snapshot, &before, &after)?;
    std::fs::write(settings, restored).map_err(|error| error.to_string())
}

/// The original bytes when reverting left nothing else changed; otherwise the
/// reverted settings, pretty-printed like Claude writes them.
fn restored_text(
    snapshot: &str,
    before: &serde_json::Value,
    after: &serde_json::Value,
) -> Result<String, String> {
    if after == before {
        return Ok(snapshot.to_string());
    }
    let mut text = serde_json::to_string_pretty(after).map_err(|error| error.to_string())?;
    if snapshot.ends_with('\n') {
        text.push('\n');
    }
    Ok(text)
}

/// The keys a model or effort switch writes, set back to `before`'s values
/// (removed where `before` had none).
fn revert_switch_keys(before: &serde_json::Value, after: &mut serde_json::Value) {
    let Some(after) = after.as_object_mut() else {
        return;
    };
    for key in SWITCH_KEYS {
        match before.get(key) {
            Some(value) => {
                after.insert(key.to_string(), value.clone());
            }
            None => {
                after.remove(key);
            }
        }
    }
}

fn switch_permission_file(conversation_id: &str) -> Option<PathBuf> {
    Some(state_directory()?.join(format!("{conversation_id}{SWITCH_PERMISSION_SUFFIX}")))
}

/// Let the next model switch in this session skip Claude's confirmation - the
/// one PADE is about to type.
fn grant_switch_permission(conversation_id: &str) {
    let Some(path) = switch_permission_file(conversation_id) else {
        return;
    };
    if let Err(error) = std::fs::write(path, []) {
        eprintln!("pade: could not grant the model switch: {error}");
    }
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

fn model_option(model: &serde_json::Value) -> Option<ModelOption> {
    let id = model.get("id")?.as_str()?;
    let effort_options = model
        .pointer("/thinking/effort_options")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let recommended_effort = effort_options
        .iter()
        .find(|option| is_recommended(option))
        .and_then(|option| option.get("id")?.as_str())
        .map(str::to_string);
    Some(ModelOption {
        id: id.to_string(),
        name: text_or(model, "name", id),
        efforts: effort_options.iter().filter_map(effort_option).collect(),
        recommended_effort,
    })
}

fn effort_option(option: &serde_json::Value) -> Option<EffortOption> {
    let id = option.get("id")?.as_str()?;
    Some(EffortOption {
        id: id.to_string(),
        name: text_or(option, "name", id),
    })
}

fn is_recommended(effort_option: &serde_json::Value) -> bool {
    effort_option
        .pointer("/badge/message")
        .and_then(serde_json::Value::as_str)
        == Some(RECOMMENDED_BADGE)
}

/// `value[key]` as text, else `fallback`.
fn text_or(value: &serde_json::Value, key: &str, fallback: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

/// The most recently written catalog for Claude Code's own picker.
fn newest_catalog() -> Option<serde_json::Value> {
    let directory = settings_directory()?.join("cache").join("model-catalog");
    let newest = std::fs::read_dir(directory)
        .ok()?
        .flatten()
        .filter(is_picker_catalog)
        .max_by_key(|entry| {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
        })?;
    crate::util::read_json(&newest.path())
}

fn is_picker_catalog(entry: &std::fs::DirEntry) -> bool {
    entry
        .file_name()
        .to_string_lossy()
        .ends_with(CATALOG_SURFACE_SUFFIX)
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
mod tests {
    use super::{
        effort_for_model, merge_settings, model_option, newest_effort_set, newest_ultracode_entry,
        revert_switch_keys, ULTRACODE_ENTER, ULTRACODE_EXIT,
    };

    #[test]
    fn the_newest_effort_the_transcript_logs_is_the_sessions_own() {
        let set = |level: &str, at: &str| {
            format!(
                r#"{{"type":"user","message":{{"role":"user","content":"<local-command-stdout>Set effort level to {level} (saved as your default for new sessions): Deeper</local-command-stdout>"}},"timestamp":"{at}"}}"#
            )
        };
        let cancelled = r#"{"type":"user","message":{"role":"user","content":"<local-command-stdout>Cancelled</local-command-stdout>"},"timestamp":"2026-10-15T02:28:32.508Z"}"#;
        let lines = [
            set("low", "2026-10-15T02:20:00.000Z"),
            set("xhigh", "2026-10-15T02:27:42.600Z"),
            cancelled.to_string(),
        ]
        .join("\n");

        let (level, set_at) = newest_effort_set(&lines).expect("a logged level");
        assert_eq!(level, "xhigh");
        assert_eq!(set_at, 1_792_031_262_600);
        assert_eq!(newest_effort_set(cancelled), None);
    }

    #[test]
    fn the_newest_ultracode_entry_decides_whether_it_is_on() {
        let enter = r#"{"type":"attachment","attachment":{"type":"ultra_effort_enter","reminderType":"full"}}"#;
        let exit = r#"{"type":"attachment","attachment":{"type":"ultra_effort_exit"}}"#;
        let prompt = r#"{"type":"user","message":{"role":"user","content":"go"}}"#;
        assert_eq!(
            newest_ultracode_entry(&[enter, prompt].join("\n")),
            Some(ULTRACODE_ENTER)
        );
        assert_eq!(
            newest_ultracode_entry(&[enter, exit, prompt].join("\n")),
            Some(ULTRACODE_EXIT)
        );
        assert_eq!(newest_ultracode_entry(prompt), None);
    }

    #[test]
    fn a_prompt_merely_mentioning_ultracode_is_not_an_entry() {
        let mention =
            r#"{"type":"user","message":{"role":"user","content":"what is ultra_effort_enter?"}}"#;
        assert_eq!(newest_ultracode_entry(mention), None);
    }

    #[test]
    fn a_switch_is_reverted_key_by_key_and_other_edits_survive() {
        let before = serde_json::json!({
            "theme": "dark",
            "model": "claude-opus-5",
            "modelSettings": { "claude-opus-5": { "effortLevel": "high" } }
        });
        let mut after = serde_json::json!({
            "theme": "light",
            "model": "claude-sonnet-5-5",
            "modelSettings": {
                "claude-opus-5": { "effortLevel": "high" },
                "claude-sonnet-5-5": { "effortLevel": "low" }
            }
        });
        revert_switch_keys(&before, &mut after);
        assert_eq!(after["model"], "claude-opus-5");
        assert_eq!(after["modelSettings"], before["modelSettings"]);
        assert_eq!(after["theme"], "light");
    }

    #[test]
    fn a_key_absent_before_the_switch_is_removed_again() {
        let before = serde_json::json!({ "theme": "dark" });
        let mut after = serde_json::json!({ "theme": "dark", "model": "sonnet" });
        revert_switch_keys(&before, &mut after);
        assert_eq!(after, before);
    }

    #[test]
    fn reads_a_catalog_model_with_its_labelled_effort_ladder() {
        let model = serde_json::json!({
            "id": "claude-opus-5-5",
            "name": "Opus 5.5",
            "thinking": { "type": "effort", "effort_options": [
                { "id": "low", "name": "Low" },
                { "id": "medium", "name": "Medium", "badge": { "message": "Recommended" } },
                { "id": "xhigh", "name": "Extra" }
            ] }
        });
        let option = model_option(&model).expect("an option");
        let labels: Vec<_> = option
            .efforts
            .iter()
            .map(|effort| effort.name.as_str())
            .collect();
        assert_eq!(option.name, "Opus 5.5");
        assert_eq!(labels, ["Low", "Medium", "Extra"]);
        assert_eq!(option.recommended_effort.as_deref(), Some("medium"));
    }

    #[test]
    fn a_models_own_level_outranks_the_global_one() {
        let settings = serde_json::json!({
            "effortLevel": "xhigh",
            "modelSettings": { "claude-opus-5": { "effortLevel": "high" } }
        });
        assert_eq!(
            effort_for_model(&settings, None, "claude-opus-5").as_deref(),
            Some("high")
        );
        assert_eq!(
            effort_for_model(&settings, None, "claude-fable-5-1").as_deref(),
            Some("xhigh")
        );
    }

    #[test]
    fn a_catalog_model_without_its_own_level_runs_at_the_catalog_default() {
        let settings = serde_json::json!({
            "effortLevel": "xhigh",
            "modelSettings": { "claude-opus-5-5": { "effortLevel": "high" } }
        });
        let catalog = serde_json::json!({ "catalog": { "config": { "models": [
            { "id": "claude-opus-5-5", "runtime": { "default_effort": "medium" } },
            { "id": "claude-sonnet-5-5", "runtime": { "default_effort": "medium" } }
        ] } } });
        let effort = |model: &str| effort_for_model(&settings, Some(&catalog), model);
        assert_eq!(effort("claude-sonnet-5-5").as_deref(), Some("medium"));
        assert_eq!(effort("claude-sonnet-5-5[1m]").as_deref(), Some("medium"));
        assert_eq!(effort("claude-opus-5-5").as_deref(), Some("high"));
        assert_eq!(effort("claude-opus-4-8").as_deref(), Some("xhigh"));
    }

    #[test]
    fn a_context_variant_shares_its_models_level() {
        let settings = serde_json::json!({
            "effortLevel": "xhigh",
            "modelSettings": { "claude-opus-5-5": { "effortLevel": "medium" } }
        });
        assert_eq!(
            effort_for_model(&settings, None, "claude-opus-5-5[1m]").as_deref(),
            Some("medium")
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
