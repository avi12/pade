//! GitHub Copilot CLI adapter: reads Copilot's own per-session event log.
//!
//! PADE pins each launch to its conversation id (`--resume=<id>` starts a new
//! session with that id), so the log is found by name:
//! `~/.copilot/session-state/<id>/events.jsonl`. Copilot appends a
//! `session.model_change` about a second after start and on every `/model`
//! switch, carrying the new model and its reasoning effort. Hooks don't help -
//! no Copilot hook payload names the model. Measured against Copilot CLI 1.0.34.

use std::path::{Path, PathBuf};

use super::{EffortOption, ModelOption, RunningModel};

const EVENT_LOG: &str = "events.jsonl";
const MODEL_CHANGE: &str = "session.model_change";
/// The events that name the model the session opened on (`--model`).
const OPENING_EVENTS: [&str; 2] = ["session.start", "session.resume"];
/// The reasoning levels `copilot --effort` accepts (its `--help`, 1.0.34).
const EFFORT_LEVELS: [&str; 4] = ["low", "medium", "high", "xhigh"];
/// How many recent sessions to mine for models - enough to cover what the
/// user switches between without reading every log ever written.
const RECENT_SESSIONS: usize = 20;

pub(super) fn read(conversation_id: &str) -> Option<RunningModel> {
    let log = session_root()?.join(conversation_id).join(EVENT_LOG);
    crate::util::newest_in_tail(&log, newest_model_in)
}

/// Copilot keeps no model catalog on disk, so the options are the models its
/// recent sessions actually ran, newest first.
pub(super) fn options() -> Vec<ModelOption> {
    let mut models: Vec<String> = Vec::new();
    for log in recent_event_logs().into_iter().take(RECENT_SESSIONS) {
        let Some(running) = crate::util::newest_in_tail(&log, newest_model_in) else {
            continue;
        };
        if !models.contains(&running.model) {
            models.push(running.model);
        }
    }
    models.into_iter().map(model_option).collect()
}

/// `~/.copilot/session-state`, one folder per session.
pub(super) fn session_root() -> Option<PathBuf> {
    Some(
        crate::util::home_directory()?
            .join(".copilot")
            .join("session-state"),
    )
}

/// The session a changed path under the session root belongs to, when it is a
/// session's event log.
pub(super) fn session_of_event_log(path: &Path) -> Option<String> {
    let is_event_log = path.file_name()? == EVENT_LOG;
    if !is_event_log {
        return None;
    }
    Some(path.parent()?.file_name()?.to_str()?.to_string())
}

fn newest_model_in(lines: &str) -> Option<RunningModel> {
    lines.lines().rev().find_map(model_from_event_line)
}

/// The model one event-log line names: the new one on a model change, or the
/// one the session opened on.
fn model_from_event_line(line: &str) -> Option<RunningModel> {
    let event: serde_json::Value = serde_json::from_str(line).ok()?;
    let kind = event.get("type")?.as_str()?;
    let data = event.get("data")?;
    let text = |key: &str| data.get(key).and_then(serde_json::Value::as_str);
    if kind == MODEL_CHANGE {
        return Some(RunningModel {
            model: text("newModel")?.to_string(),
            effort: text("reasoningEffort").map(str::to_string),
        });
    }
    if OPENING_EVENTS.contains(&kind) {
        return Some(RunningModel {
            model: text("selectedModel")?.to_string(),
            effort: None,
        });
    }
    None
}

/// Every session's event log, most recently written first.
fn recent_event_logs() -> Vec<PathBuf> {
    let Some(root) = session_root() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let log = entry.path().join(EVENT_LOG);
        let Ok(modified) = std::fs::metadata(&log).and_then(|metadata| metadata.modified()) else {
            continue;
        };
        sessions.push((modified, log));
    }
    sessions.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    sessions.into_iter().map(|(_, log)| log).collect()
}

/// Copilot names no display labels, so the id is the name and every model
/// offers the full effort ladder.
fn model_option(model: String) -> ModelOption {
    ModelOption {
        name: model.clone(),
        id: model,
        efforts: EFFORT_LEVELS
            .iter()
            .copied()
            .map(EffortOption::plain)
            .collect(),
        recommended_effort: None,
    }
}

#[cfg(test)]
mod tests {
    use super::newest_model_in;

    #[test]
    fn a_model_switch_names_model_and_effort() {
        let lines = [
            r#"{"type":"session.start","data":{"sessionId":"a"}}"#,
            r#"{"type":"session.model_change","data":{"newModel":"gpt-5.4-mini"}}"#,
            r#"{"type":"session.model_change","data":{"previousModel":"gpt-5.4-mini","newModel":"gpt-5.3-codex","reasoningEffort":"medium"}}"#,
            r#"{"type":"assistant.message","data":{}}"#,
        ]
        .join("\n");
        let running = newest_model_in(&lines).expect("a model");
        assert_eq!(running.model, "gpt-5.3-codex");
        assert_eq!(running.effort.as_deref(), Some("medium"));
    }

    #[test]
    fn before_any_switch_the_opening_model_speaks() {
        let line =
            r#"{"type":"session.start","data":{"sessionId":"a","selectedModel":"claude-opus-5"}}"#;
        assert_eq!(
            newest_model_in(line)
                .map(|running| running.model)
                .as_deref(),
            Some("claude-opus-5")
        );
    }
}
