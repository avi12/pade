//! Claude Code: hooks in the one `--settings` layer PADE already passes.
//!
//! Claude honours only the LAST `--settings` flag, so a second one would
//! silently drop the live theme — the hooks join that same JSON instead.
//! `SessionStart` names the model before any prompt, `PostModelSwitch` the new
//! one the moment `/model` runs, and `Stop` the effort a turn ran at. No hook
//! fires on `/effort`, but it saves the level for the current model into
//! `~/.claude/settings.json` at once, so the effort is the newer of the last
//! `Stop` report and that saved level. Measured against Claude Code 2.1.292.

use std::path::PathBuf;

use super::{
    hook_command, modified_millis, newer, read_session_state, HookShell, RunningModel, Saved,
};

/// The flag that hands Claude a settings layer for this launch alone.
const SETTINGS_FLAG: &str = "--settings";
/// Claude's own user settings, where `/model` and `/effort` save their choice.
const SETTINGS_FILE: &str = "settings.json";
/// The hook events PADE registers, by Claude's wire names.
const HOOK_EVENTS: [&str; 3] = ["SessionStart", "PostModelSwitch", "Stop"];

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

/// `{"hooks": {...}}` routing every reported event to the sink. Async, so a
/// hook never holds a turn up.
fn hook_settings() -> Option<serde_json::Map<String, serde_json::Value>> {
    let command = hook_command(HookShell::Posix)?;
    let handler = serde_json::json!([{ "hooks": [{ "type": "command", "command": command, "async": true }] }]);
    let events = HOOK_EVENTS
        .iter()
        .map(|event| ((*event).to_string(), handler.clone()))
        .collect();

    let mut settings = serde_json::Map::new();
    settings.insert("hooks".to_string(), serde_json::Value::Object(events));
    Some(settings)
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
    use super::{effort_for_model, merge_settings};

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
