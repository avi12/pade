//! The model a session's subagents may run while usage pacing has stepped the
//! session down. Pacing switches the session itself; a subagent that inherits
//! its model follows on its own, but one that names a model of its own (in the
//! spawn call, or in its agent definition) would keep spending at the old rate.
//!
//! So while a session is paced, PADE keeps one small cap file for it, rewritten
//! at every pacing step and removed when the session is back on the user's own
//! choice. The agent consults it at every spawn - never at launch, because the
//! cap moves with the usage windows mid-session:
//!
//!   * Claude - a `PreToolUse` hook on its `Agent` tool runs the sink, which
//!     rewrites the call's `model` to the cap unless it already asks for one
//!     the cap allows (`answer_spawn`);
//!   * `OpenCode` - its PADE plugin reads the file when a child session sends
//!     its first message and moves it onto the cap the same way.
//!
//! Codex subagents take the parent's model unless a custom agent pins one, and
//! its spawn tool hides the model override by default, so a paced Codex session
//! carries its subagents with it. Copilot reads hooks only from the user's
//! global config, which PADE does not edit.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::{is_plain_id, write_json};
use crate::agents::{ID_CLAUDE, ID_OPENCODE};

/// PADE's data subfolder holding one cap file per paced session.
const CAP_DIRECTORY_NAME: &str = "subagent-cap";
const CAP_FILE_EXTENSION: &str = "json";
/// Claude's spawn tool, by its current name and the one it had before.
pub(super) const CLAUDE_SPAWN_TOOLS: &str = "Agent|Task";
/// The hook event Claude asks before running a tool.
pub(super) const PRE_TOOL_USE: &str = "PreToolUse";
/// The spawn input field naming the subagent's model.
const MODEL_FIELD: &str = "model";
/// Claude's spawn tool takes a model family, not a model id.
const CLAUDE_MODEL_FAMILIES: [&str; 4] = ["opus", "sonnet", "haiku", "fable"];

/// What pacing caps a session's subagents at, in catalog model ids.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCapRequest {
    /// The model a subagent runs when it asks for nothing the cap allows.
    model: String,
    /// Every model a subagent may keep when it asks for it - the cap's own and
    /// the cheaper ones.
    allowed_models: Vec<String>,
}

/// The cap as the agent's spawn call spells models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SubagentCap {
    model: String,
    allowed: Vec<String>,
}

/// Cap the subagents of `command`'s session `conversation_id`, or lift the cap
/// with `None`. A no-op for an agent PADE can't steer a spawn of. Off the UI
/// thread: it writes a file.
#[tauri::command]
pub async fn agent_set_subagent_cap(
    command: String,
    conversation_id: String,
    cap: Option<SubagentCapRequest>,
) -> Result<(), String> {
    if !is_plain_id(&conversation_id) {
        return Err("that conversation id isn't valid".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let path = cap_file(&conversation_id).ok_or("no PADE data directory")?;
        let Some(cap) = cap.and_then(|request| spawn_cap(&command, &request)) else {
            return match std::fs::remove_file(&path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    Err(error.to_string())
                }
                _ => Ok(()),
            };
        };
        write_json(&path, &cap).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Answer Claude's `PreToolUse` for a spawn: the call moved onto the cap, or
/// nothing - which lets the call run as asked - when the session isn't paced.
pub(super) fn answer_spawn(conversation_id: &str, report: &serde_json::Value) {
    let Some(cap) = read_cap(conversation_id) else {
        return;
    };
    let Some(input) = report.get("tool_input") else {
        return;
    };
    if let Some(capped) = capped_input(input, &cap) {
        let answer = serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": PRE_TOOL_USE,
                "permissionDecision": "allow",
                "updatedInput": capped
            }
        });
        println!("{answer}");
    }
}

/// Where the session's cap lives - also handed to the `OpenCode` plugin.
pub(super) fn cap_file(conversation_id: &str) -> Option<PathBuf> {
    Some(
        crate::util::pade_data_directory()?
            .join(CAP_DIRECTORY_NAME)
            .join(format!("{conversation_id}.{CAP_FILE_EXTENSION}")),
    )
}

fn read_cap(conversation_id: &str) -> Option<SubagentCap> {
    let text = std::fs::read_to_string(cap_file(conversation_id)?).ok()?;
    serde_json::from_str(&text).ok()
}

/// The request in `command`'s own spelling, or `None` for an agent whose
/// spawns PADE can't steer (or a model it can't name).
fn spawn_cap(command: &str, request: &SubagentCapRequest) -> Option<SubagentCap> {
    let model = spawn_model(command, &request.model)?;
    let mut allowed = vec![model.clone()];
    for id in &request.allowed_models {
        let spelled = spawn_model(command, id);
        if let Some(spelled) = spelled.filter(|spelled| !allowed.contains(spelled)) {
            allowed.push(spelled);
        }
    }
    Some(SubagentCap { model, allowed })
}

/// How `command`'s spawn names the model `id`: Claude by family, `OpenCode`
/// by its full `provider/model` id.
fn spawn_model(command: &str, id: &str) -> Option<String> {
    match command {
        ID_CLAUDE => {
            let lowered = id.to_lowercase();
            CLAUDE_MODEL_FAMILIES
                .into_iter()
                .find(|family| lowered.contains(family))
                .map(str::to_string)
        }
        ID_OPENCODE => Some(id.to_string()),
        _ => None,
    }
}

/// The spawn input moved onto the cap, or `None` when it already asks for a
/// model the cap allows. A spawn that names no model would run its agent
/// definition's own, which may be dearer, so it gets the cap's.
fn capped_input(input: &serde_json::Value, cap: &SubagentCap) -> Option<serde_json::Value> {
    let asked = input.get(MODEL_FIELD).and_then(serde_json::Value::as_str);
    let is_allowed = asked.is_some_and(|model| cap.allowed.iter().any(|allowed| allowed == model));
    if is_allowed {
        return None;
    }
    // The tool takes the answer as its whole input, so every other field stays.
    let mut capped = input.as_object()?.clone();
    capped.insert(
        MODEL_FIELD.to_string(),
        serde_json::Value::String(cap.model.clone()),
    );
    Some(serde_json::Value::Object(capped))
}

#[cfg(test)]
mod tests {
    use super::{capped_input, spawn_cap, SubagentCap, SubagentCapRequest};

    fn request(model: &str, allowed_models: &[&str]) -> SubagentCapRequest {
        SubagentCapRequest {
            model: model.to_string(),
            allowed_models: allowed_models.iter().map(|id| (*id).to_string()).collect(),
        }
    }

    fn sonnet_cap() -> SubagentCap {
        SubagentCap {
            model: "sonnet".to_string(),
            allowed: vec!["sonnet".to_string(), "haiku".to_string()],
        }
    }

    #[test]
    fn claude_spells_the_cap_by_family() {
        let cap = spawn_cap(
            "claude",
            &request(
                "claude-sonnet-5-5",
                &["claude-sonnet-5-5", "claude-haiku-4-5-20251001"],
            ),
        );
        assert_eq!(cap, Some(sonnet_cap()));
    }

    #[test]
    fn only_steerable_agents_get_a_cap() {
        assert_eq!(spawn_cap("codex", &request("gpt-5.6-terra", &[])), None);
        assert_eq!(spawn_cap("claude", &request("some-model", &[])), None);
        assert_eq!(
            spawn_cap("opencode", &request("openai/gpt-5.6-sol", &[])).map(|cap| cap.model),
            Some("openai/gpt-5.6-sol".to_string())
        );
    }

    #[test]
    fn a_dearer_or_missing_model_moves_onto_the_cap_and_keeps_the_rest() {
        let input = serde_json::json!({ "description": "d", "prompt": "p", "model": "opus" });
        let capped = capped_input(&input, &sonnet_cap()).expect("capped");
        assert_eq!(capped["model"], "sonnet");
        assert_eq!(capped["prompt"], "p");

        let unnamed = serde_json::json!({ "prompt": "p", "subagent_type": "Plan" });
        assert_eq!(
            capped_input(&unnamed, &sonnet_cap()).expect("capped")["model"],
            "sonnet"
        );
    }

    #[test]
    fn a_model_the_cap_allows_runs_as_asked() {
        let input = serde_json::json!({ "prompt": "p", "model": "haiku" });
        assert_eq!(capped_input(&input, &sonnet_cap()), None);
    }
}
