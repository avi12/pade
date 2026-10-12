//! Claude Code: hooks in the one `--settings` layer PADE already passes.
//!
//! Claude honours only the LAST `--settings` flag, so a second one would
//! silently drop the live theme — the hooks join that same JSON instead.
//! `SessionStart` names the model before any prompt, `PostModelSwitch` the new
//! one the moment `/model` runs, and `Stop` the effort a turn ran at. No hook
//! fires on `/effort`, but it saves the level for the current model into
//! `~/.claude/settings.json` at once, so the effort is the newer of the last
//! `Stop` report and that saved level. Measured against Claude Code 2.1.292.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{
    hook_command, modified_millis, newer, now_millis, read_session_state, read_state,
    state_directory, state_file, write_state, EffortOption, HookShell, ModelChoice, ModelOption,
    RunningModel, Saved,
};

/// The flag that hands Claude a settings layer for this launch alone.
const SETTINGS_FLAG: &str = "--settings";
/// Claude's own user settings, where `/model` and `/effort` save their choice.
const SETTINGS_FILE: &str = "settings.json";
/// The hook events that only report, by Claude's wire names — run async, so a
/// hook never holds a turn up.
const REPORTING_EVENTS: [&str; 3] = ["SessionStart", "PostModelSwitch", "Stop"];
/// Claude asks this hook before switching models; its answer can skip the
/// "Switch model?" confirmation, so it runs in step (not async).
pub(super) const PRE_MODEL_SWITCH: &str = "PreModelSwitch";
/// The answer that lets a switch PADE started go ahead without the prompt.
pub(super) const ALLOW_MODEL_SWITCH: &str =
    r#"{"hookSpecificOutput":{"hookEventName":"PreModelSwitch","permissionDecision":"allow"}}"#;
/// How long a switch permission PADE grants stays good — the switch it was
/// granted for lands well within this.
const SWITCH_PERMISSION_LIFETIME: Duration = Duration::from_secs(15);
const SWITCH_PERMISSION_SUFFIX: &str = ".switch-permission";

/// Join the hooks to the `--settings` layer in `arguments` (the live theme),
/// or form one when there is none.
pub(super) fn attach(arguments: &mut Vec<String>) {
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
    let events = REPORTING_EVENTS
        .iter()
        .map(|event| ((*event).to_string(), handler(true)))
        .chain(std::iter::once((
            PRE_MODEL_SWITCH.to_string(),
            handler(false),
        )))
        .collect();

    let mut settings = serde_json::Map::new();
    settings.insert("hooks".to_string(), serde_json::Value::Object(events));
    Some(settings)
}

/// Ctrl+S: Claude stashes the draft in its input box and brings it back once
/// the next command has run — so a switch never mixes into half a prompt.
const STASH_DRAFT: &str = "\u{13}";
const SUBMIT: &str = "\r";
/// The pause between keystroke groups, so Claude reads typing as typing — a
/// command and its Enter sent as one burst arrive as a paste, whose Enter is a
/// newline rather than a submit.
const KEYSTROKE_SETTLE: Duration = Duration::from_millis(150);
/// How long a switch may take to land in the settings file before PADE takes
/// it to be waiting on Claude's confirmation dialog.
const SWITCH_LANDING: Duration = Duration::from_millis(1500);
const LANDING_POLL: Duration = Duration::from_millis(50);

/// Switch a running session live, as if typed: stash the draft, `/model`,
/// then stash again and `/effort`. Each step is confirmed by Claude saving the
/// value — never by reading the screen — and the user's saved default is put
/// back afterwards, so only this session changes. Works mid-turn: the next
/// request already uses the new model (verified on 2.1.292).
pub(super) fn apply_live(
    conversation_id: &str,
    choice: &ModelChoice,
    type_keys: &dyn Fn(&str) -> Result<(), String>,
) -> Result<(), String> {
    let settings = settings_directory()
        .map(|directory| directory.join(SETTINGS_FILE))
        .ok_or("no Claude settings directory")?;
    let snapshot = std::fs::read_to_string(&settings).ok();
    let outcome = switch_by_commands(conversation_id, choice, type_keys);
    restore_saved_defaults(&settings, snapshot.as_deref())?;
    let applied = outcome?;
    record_applied(conversation_id, &applied);
    Ok(())
}

/// Type the commands and wait for each to land; returns what now runs.
fn switch_by_commands(
    conversation_id: &str,
    choice: &ModelChoice,
    type_keys: &dyn Fn(&str) -> Result<(), String>,
) -> Result<ModelChoice, String> {
    let mut current_model = read(conversation_id).map(|running| running.model);

    if let Some(model) = &choice.model {
        grant_switch_permission(conversation_id);
        run_command(type_keys, &format!("/model {model}"))?;
        let landed = wait_until(|| saved_model().as_deref() == Some(model.as_str()));
        if !landed {
            return Err(format!("Claude didn't switch to {model}"));
        }
        current_model = Some(model.clone());
    }

    if let Some(effort) = &choice.effort {
        let model = current_model
            .clone()
            .ok_or("the session's model isn't known yet")?;
        let effort_saved =
            || saved_effort(&model).map(|saved| saved.value).as_deref() == Some(effort.as_str());
        run_command(type_keys, &format!("/effort {effort}"))?;
        // No hook can pre-answer Claude's "Change effort level?" dialog; a level
        // that didn't land at once is sitting on it, and Enter accepts it.
        let landed = wait_until(effort_saved) || {
            type_keys(SUBMIT)?;
            wait_until(effort_saved)
        };
        if !landed {
            return Err(format!("Claude didn't switch to {effort} effort"));
        }
    }

    Ok(ModelChoice {
        model: current_model,
        effort: choice.effort.clone(),
    })
}

fn run_command(
    type_keys: &dyn Fn(&str) -> Result<(), String>,
    command: &str,
) -> Result<(), String> {
    type_keys(STASH_DRAFT)?;
    std::thread::sleep(KEYSTROKE_SETTLE);
    type_keys(command)?;
    std::thread::sleep(KEYSTROKE_SETTLE);
    type_keys(SUBMIT)
}

fn wait_until(landed: impl Fn() -> bool) -> bool {
    let deadline = std::time::Instant::now() + SWITCH_LANDING;
    while std::time::Instant::now() < deadline {
        if landed() {
            return true;
        }
        std::thread::sleep(LANDING_POLL);
    }
    landed()
}

fn saved_model() -> Option<String> {
    let settings = read_settings()?;
    Some(settings.get("model")?.as_str()?.to_string())
}

fn read_settings() -> Option<serde_json::Value> {
    let path = settings_directory()?.join(SETTINGS_FILE);
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Put back the default `/model` and `/effort` just saved over — the keys they
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
    let restored = if after == before {
        snapshot.to_string()
    } else {
        let mut text = serde_json::to_string_pretty(&after).map_err(|error| error.to_string())?;
        if snapshot.ends_with('\n') {
            text.push('\n');
        }
        text
    };
    std::fs::write(settings, restored).map_err(|error| error.to_string())
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

/// The settings keys `/model` and `/effort` save their choice under.
const SWITCH_KEYS: [&str; 2] = ["model", "modelSettings"];

/// Note what PADE just applied as this session's newest report, so the tab
/// shows it rather than the default restored a moment earlier.
fn record_applied(conversation_id: &str, applied: &ModelChoice) {
    let Some(path) = state_file(conversation_id) else {
        return;
    };
    let now = now_millis();
    let mut state = read_state(&path).unwrap_or_default();
    if let Some(model) = &applied.model {
        state.model = Some(model.clone());
        state.model_at = now;
    }
    if let Some(effort) = &applied.effort {
        state.effort = Some(effort.clone());
        state.effort_at = now;
    }
    if let Err(error) = write_state(&path, &state) {
        eprintln!("pade: could not record the applied model: {error}");
    }
}

fn switch_permission_file(conversation_id: &str) -> Option<PathBuf> {
    Some(state_directory()?.join(format!("{conversation_id}{SWITCH_PERMISSION_SUFFIX}")))
}

/// Let the next model switch in this session skip Claude's confirmation — the
/// one PADE is about to type.
fn grant_switch_permission(conversation_id: &str) {
    let Some(path) = switch_permission_file(conversation_id) else {
        return;
    };
    if let Err(error) = std::fs::write(path, []) {
        eprintln!("pade: could not grant the model switch: {error}");
    }
}

/// Whether PADE granted this session's pending switch — used up by asking.
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

pub(super) fn read(conversation_id: &str) -> Option<RunningModel> {
    let Some(state) = read_session_state(conversation_id) else {
        return logged_turn(conversation_id);
    };
    let Some(model) = state.model.clone() else {
        return logged_turn(conversation_id);
    };

    let saved = saved_effort(&model);
    let reported = state
        .effort
        .as_deref()
        .map(|effort| (effort, state.effort_at));
    Some(RunningModel {
        effort: newer(reported, saved.as_ref(), state.model_at),
        model,
    })
}

/// A session launched before PADE attached its hooks: the newest turn Claude's
/// own log records.
fn logged_turn(conversation_id: &str) -> Option<RunningModel> {
    let turn = crate::session_context::logged_claude_turn(conversation_id)?;
    Some(RunningModel {
        model: turn.model,
        effort: turn.effort,
    })
}

/// The catalog surface Claude Code's own `/model` picker reads.
const CATALOG_SURFACE_SUFFIX: &str = "-cc.json";

/// The models Claude Code itself lists, from the catalog it caches under
/// `~/.claude/cache/model-catalog` — names, effort levels with their labels,
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

fn model_option(model: &serde_json::Value) -> Option<ModelOption> {
    let id = model.get("id")?.as_str()?;
    let effort_options = model
        .pointer("/thinking/effort_options")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let efforts = effort_options
        .iter()
        .filter_map(|option| {
            let effort = option.get("id")?.as_str()?;
            Some(EffortOption {
                id: effort.to_string(),
                name: option
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(effort)
                    .to_string(),
            })
        })
        .collect();
    let recommended_effort = effort_options
        .iter()
        .find(|option| {
            option
                .pointer("/badge/message")
                .and_then(serde_json::Value::as_str)
                == Some(RECOMMENDED_BADGE)
        })
        .and_then(|option| option.get("id")?.as_str())
        .map(str::to_string);
    Some(ModelOption {
        id: id.to_string(),
        name: model
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(id)
            .to_string(),
        efforts,
        recommended_effort,
    })
}

/// The badge Claude puts on the level it recommends for a model.
const RECOMMENDED_BADGE: &str = "Recommended";

fn newest_catalog() -> Option<serde_json::Value> {
    let directory = settings_directory()?.join("cache").join("model-catalog");
    let newest = std::fs::read_dir(directory)
        .ok()?
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(CATALOG_SURFACE_SUFFIX)
        })
        .max_by_key(|entry| {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
        })?;
    serde_json::from_str(&std::fs::read_to_string(newest.path()).ok()?).ok()
}

pub(super) fn settings_directory() -> Option<PathBuf> {
    Some(crate::util::home_dir()?.join(".claude"))
}

pub(super) fn is_settings_file(name: &str) -> bool {
    name == SETTINGS_FILE
}

/// The effort the user's settings hold for `model`: its own
/// `modelSettings.<model>.effortLevel`, else the global `effortLevel`.
fn saved_effort(model: &str) -> Option<Saved> {
    let path = settings_directory()?.join(SETTINGS_FILE);
    let saved_at = modified_millis(&path)?;
    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
    Some(Saved {
        value: effort_for_model(&settings, model)?,
        saved_at,
    })
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

#[cfg(test)]
mod tests {
    use super::{effort_for_model, merge_settings, model_option, revert_switch_keys};

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
