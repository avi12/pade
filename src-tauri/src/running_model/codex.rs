//! Codex adapter: trusted hooks passed with `-c`, its per-turn rollout log, and
//! `config.toml`.
//!
//! Reporting: `SessionStart`, `UserPromptSubmit` and `Stop` hooks carry the
//! model and the path of the session's rollout (Codex's own transcript). Hooks
//! from `-c` only run once trusted, so the launch also trusts exactly PADE's own
//! three - by the fingerprint Codex itself computes - never
//! `--dangerously-bypass-hook-trust`, which would trust a repository's hooks
//! too.
//!
//! Reading: no hook carries the effort, but the rollout's newest `turn_context`
//! line does, written at the start of every turn. `/model` saves the model and
//! effort into `config.toml` at once, so a switch shows before the next turn.
//!
//! Measured against Codex 0.161.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use super::{
    hook_command, modified_millis, newer, read_session_state, timestamp_millis, EffortOption,
    HookShell, ModelOption, RunningModel, Saved,
};

const CONFIG_FLAG: &str = "-c";
const SETTINGS_FILE: &str = "config.toml";
/// Codex's cached model catalog - what its own `/model` picker lists.
const MODELS_CACHE_FILE: &str = "models_cache.json";
/// A catalog model the picker shows (others are hidden or internal).
const LISTED_VISIBILITY: &str = "list";
/// How long Codex may run a hook - fixed, because it is part of the trust
/// fingerprint.
const HOOK_TIMEOUT_SECONDS: u64 = 30;
/// The config layer Codex files every `-c` override under, which the trust
/// keys name.
const OVERRIDE_LAYER: &str = if cfg!(windows) {
    r"C:\<session-flags>\config.toml"
} else {
    "/<session-flags>/config.toml"
};
const TURN_CONTEXT: &str = "turn_context";
/// The `config.toml` keys `/model` saves and a `-c` override sets.
const MODEL_KEY: &str = "model";
const EFFORT_KEY: &str = "model_reasoning_effort";

/// The hook events PADE registers: Codex's config name and the snake-case name
/// its trust keys use.
const HOOK_EVENTS: [(&str, &str); 3] = [
    ("SessionStart", "session_start"),
    ("UserPromptSubmit", "user_prompt_submit"),
    ("Stop", "stop"),
];

/// Add the `-c` overrides that register PADE's hooks and mark them trusted.
pub(super) fn attach(arguments: &mut Vec<String>) {
    let Some(command) = hook_command(hook_shell()) else {
        return;
    };
    arguments.push(CONFIG_FLAG.to_string());
    arguments.push(format!("hooks={{{}}}", hook_entries(&command)));
    arguments.push(CONFIG_FLAG.to_string());
    arguments.push(format!("hooks.state={{{}}}", trust_entries(&command)));
}

/// `-c` overrides that run a picked model and effort for one launch - unlike
/// `/model`, they never reach `config.toml`.
pub(super) fn choice_overrides(
    model: Option<&str>,
    effort: Option<&str>,
) -> Vec<(&'static str, String)> {
    let model_override = model.map(|model| (MODEL_KEY, model));
    let effort_override = effort.map(|effort| (EFFORT_KEY, effort));
    model_override
        .into_iter()
        .chain(effort_override)
        .map(|(key, value)| (CONFIG_FLAG, format!("{key}={}", toml_string(value))))
        .collect()
}

/// The session's model and effort: the newest of what its hooks and rollout
/// reported and what `/model` last saved. Codex opens its session - and fires
/// its first hook - only at the first prompt, so until then the saved settings
/// are what it will run.
pub(super) fn read(conversation_id: &str) -> Option<RunningModel> {
    let state = read_session_state(conversation_id).unwrap_or_default();
    let turn = state
        .transcript
        .as_deref()
        .and_then(|transcript| crate::util::newest_in_tail(Path::new(transcript), newest_turn_in));
    let saved = saved_settings();

    let hook_model = state.model.as_deref().map(|model| (model, state.model_at));
    let turn_model = turn.as_ref().map(|turn| (turn.model.as_str(), turn.at));
    let reported_model = [hook_model, turn_model]
        .into_iter()
        .flatten()
        .max_by_key(|(_, at)| *at);
    let model = newer(reported_model, saved.model.as_ref(), 0)?;

    // A turn's effort, or one PADE launched the session with - whichever is newer.
    let turned_effort = turn
        .as_ref()
        .and_then(|turn| Some((turn.effort.as_deref()?, turn.at)));
    let launched_effort = state
        .effort
        .as_deref()
        .map(|effort| (effort, state.effort_at));
    let reported_effort = [turned_effort, launched_effort]
        .into_iter()
        .flatten()
        .max_by_key(|(_, at)| *at);
    Some(RunningModel {
        effort: newer(reported_effort, saved.effort.as_ref(), 0),
        model,
    })
}

/// The models Codex's own picker lists, each with the reasoning levels it
/// supports and its default level.
pub(super) fn options() -> Vec<ModelOption> {
    let Some(catalog) = read_models_cache() else {
        return Vec::new();
    };
    let Some(models) = catalog.get("models").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    models.iter().filter_map(model_option).collect()
}

/// Codex keeps its settings directly in its home folder.
pub(super) fn settings_directory() -> Option<PathBuf> {
    crate::agents::codex_home()
}

pub(super) fn is_settings_file(name: &str) -> bool {
    name == SETTINGS_FILE
}

fn hook_shell() -> HookShell {
    // Codex runs hook commands through PowerShell on Windows (measured).
    if cfg!(windows) {
        HookShell::PowerShell
    } else {
        HookShell::Posix
    }
}

/// `SessionStart=[...],UserPromptSubmit=[...],Stop=[...]`, each running
/// `command`.
fn hook_entries(command: &str) -> String {
    let quoted_command = toml_string(command);
    let handler = format!(
        "[{{hooks=[{{type=\"command\",command={quoted_command},timeout={HOOK_TIMEOUT_SECONDS}}}]}}]"
    );
    HOOK_EVENTS
        .map(|(event, _)| format!("{event}={handler}"))
        .join(",")
}

/// One `"<layer>:<event>:0:0"={trusted_hash="sha256:..."}` per hook.
fn trust_entries(command: &str) -> String {
    HOOK_EVENTS
        .map(|(_, trust_name)| {
            let key = toml_string(&format!("{OVERRIDE_LAYER}:{trust_name}:0:0"));
            let hash = toml_string(&fingerprint(trust_name, command));
            format!("{key}={{trusted_hash={hash}}}")
        })
        .join(",")
}

/// The fingerprint Codex trusts a hook group by: sha256 of its compact JSON,
/// keys sorted. The keys are written in sorted order so the bytes match
/// whether or not `serde_json` preserves insertion order.
fn fingerprint(trust_name: &str, command: &str) -> String {
    let group = serde_json::json!({
        "event_name": trust_name,
        "hooks": [{
            "async": false,
            "command": command,
            "timeout": HOOK_TIMEOUT_SECONDS,
            "type": "command"
        }]
    });
    let digest = Sha256::digest(group.to_string().as_bytes());
    format!("sha256:{digest:x}")
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_string()).to_string()
}

/// A rollout's newest turn: what it ran on, and when.
#[derive(Debug, PartialEq, Eq)]
struct Turn {
    model: String,
    effort: Option<String>,
    at: u64,
}

fn newest_turn_in(lines: &str) -> Option<Turn> {
    lines.lines().rev().find_map(turn_from_line)
}

/// The turn one rollout line describes, when it is a `turn_context` line.
fn turn_from_line(line: &str) -> Option<Turn> {
    let entry: serde_json::Value = serde_json::from_str(line).ok()?;
    let is_turn_context = entry.get("type")?.as_str()? == TURN_CONTEXT;
    if !is_turn_context {
        return None;
    }
    let payload = entry.get("payload")?;
    let text = |pointer: &str| payload.pointer(pointer).and_then(serde_json::Value::as_str);
    Some(Turn {
        model: text("/model")?.to_string(),
        effort: text("/effort")
            .or_else(|| text("/collaboration_mode/settings/reasoning_effort"))
            .map(str::to_string),
        at: timestamp_millis(entry.get("timestamp")?.as_str()?)?,
    })
}

/// The model and effort `/model` last saved to `config.toml`.
#[derive(Debug, Default)]
struct SavedSettings {
    model: Option<Saved>,
    effort: Option<Saved>,
}

fn saved_settings() -> SavedSettings {
    let Some(path) = settings_directory().map(|directory| directory.join(SETTINGS_FILE)) else {
        return SavedSettings::default();
    };
    let Some(saved_at) = modified_millis(&path) else {
        return SavedSettings::default();
    };
    let Some(config) = read_toml(&path) else {
        return SavedSettings::default();
    };
    let saved = |key: &str| {
        Some(Saved {
            value: config.get(key)?.as_str()?.to_string(),
            saved_at,
        })
    };
    SavedSettings {
        model: saved(MODEL_KEY),
        effort: saved(EFFORT_KEY),
    }
}

fn read_toml(path: &Path) -> Option<toml::Table> {
    std::fs::read_to_string(path).ok()?.parse().ok()
}

fn read_models_cache() -> Option<serde_json::Value> {
    crate::util::read_json(&settings_directory()?.join(MODELS_CACHE_FILE))
}

fn model_option(model: &serde_json::Value) -> Option<ModelOption> {
    let text = |key: &str| model.get(key).and_then(serde_json::Value::as_str);
    let is_listed = text("visibility") == Some(LISTED_VISIBILITY);
    if !is_listed {
        return None;
    }
    let id = text("slug")?;
    Some(ModelOption {
        id: id.to_string(),
        name: text("display_name").unwrap_or(id).to_string(),
        efforts: supported_efforts(model),
        recommended_effort: text("default_reasoning_level").map(str::to_string),
    })
}

fn supported_efforts(model: &serde_json::Value) -> Vec<EffortOption> {
    let Some(levels) = model
        .get("supported_reasoning_levels")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    levels
        .iter()
        .filter_map(|level| level.get("effort")?.as_str().map(EffortOption::plain))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{fingerprint, model_option, newest_turn_in};

    #[test]
    fn lists_only_models_the_picker_shows() {
        let listed = serde_json::json!({
            "slug": "gpt-5.6-terra", "display_name": "GPT-5.6-Terra", "visibility": "list",
            "default_reasoning_level": "medium",
            "supported_reasoning_levels": [{ "effort": "low" }, { "effort": "ultra" }]
        });
        let hidden = serde_json::json!({ "slug": "gpt-reserve", "visibility": "hide" });
        let option = model_option(&listed).expect("a listed model");
        let levels: Vec<_> = option
            .efforts
            .iter()
            .map(|effort| effort.id.as_str())
            .collect();
        assert_eq!(option.name, "GPT-5.6-Terra");
        assert_eq!(levels, ["low", "ultra"]);
        assert_eq!(option.recommended_effort.as_deref(), Some("medium"));
        assert!(model_option(&hidden).is_none());
    }

    #[test]
    fn fingerprints_a_hook_the_way_codex_does() {
        // The hash Codex 0.161 accepted for this exact command (verified live).
        let command = r"C:\Users\Avi\AppData\Local\Programs\Python\Python313\python.exe -I C:\Users\Avi\AppData\Local\Temp\claude\C--repositories-avi-pade\4d1440ab-d9a0-477d-b4e1-456ec87a2ccf\scratchpad\codex-probe\hook.py";
        assert_eq!(
            fingerprint("session_start", command),
            "sha256:e190c60a6efce9cf1ff60bd7cf4234b6aaa43b4cc07cead3bfdc7b3000983128"
        );
    }

    #[test]
    fn the_newest_turn_context_names_model_and_effort() {
        let lines = [
            r#"{"timestamp":"2026-10-12T01:00:00.000Z","type":"turn_context","payload":{"model":"gpt-5.6-sol","effort":"high"}}"#,
            r#"{"timestamp":"2026-10-12T01:05:00.000Z","type":"turn_context","payload":{"model":"gpt-5.6-terra","effort":"low"}}"#,
            r#"{"timestamp":"2026-10-12T01:05:01.000Z","type":"event_msg","payload":{}}"#,
        ]
        .join("\n");
        let turn = newest_turn_in(&lines).expect("a turn");
        assert_eq!(turn.model, "gpt-5.6-terra");
        assert_eq!(turn.effort.as_deref(), Some("low"));
    }

    #[test]
    fn a_turn_without_effort_falls_back_to_the_collaboration_setting() {
        let line = r#"{"timestamp":"2026-10-12T01:00:00.000Z","type":"turn_context","payload":{"model":"gpt-5.6-sol","collaboration_mode":{"settings":{"reasoning_effort":"xhigh"}}}}"#;
        let turn = newest_turn_in(line).expect("a turn");
        assert_eq!(turn.effort.as_deref(), Some("xhigh"));
    }
}
