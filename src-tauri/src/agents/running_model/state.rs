//! What hooks, plugins and PADE itself have reported for each session, kept as
//! one small JSON file per conversation: its shape (`SessionState`), how a
//! report folds into it (`fold`, `newer`) and the read and atomic-write helpers
//! every adapter goes through.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::ModelChoice;
use crate::util::now_milliseconds;

/// PADE's data subfolder holding one state file per session, and that file's
/// extension - the watcher reads the session id back off the name.
pub(super) const STATE_DIRECTORY_NAME: &str = "agent-model";
pub(super) const STATE_FILE_EXTENSION: &str = ".json";
/// Where a state file is written before it is renamed into place.
pub(super) const PARTIAL_STATE_EXTENSION: &str = "json.partial";

/// Fold one hook report into a session's state. The fields are the ones Claude
/// and Codex hooks share: `model` (or `to_model` on a model switch), an
/// `effort.level`, and the `transcript_path`.
pub(super) fn fold(state: SessionState, report: &serde_json::Value, now: u64) -> SessionState {
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

/// What hooks and plugins have reported for one session, each value stamped
/// with when it was reported (milliseconds since the epoch). This is the shape
/// of the per-session JSON file.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct SessionState {
    pub(super) model: Option<String>,
    pub(super) model_at: u64,
    pub(super) effort: Option<String>,
    pub(super) effort_at: u64,
    /// The agent's own transcript for this session, when its hooks name one.
    pub(super) transcript: Option<String>,
    /// The agent's own id for this session (a Codex thread, an `OpenCode`
    /// session) - what resuming it takes.
    pub(super) agent_session: Option<String>,
}

/// A value the agent saved to a settings file, dated by that file's last write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Saved {
    pub(super) value: String,
    pub(super) saved_at: u64,
}

/// The newer of a reported value and a saved one. `saved_from` is the earliest
/// moment the saved value can speak for this session - a model switch, say,
/// adopts the new model's saved level at the switch. A tie goes to the report,
/// which is about this session alone; the saved file is shared by every one.
pub(super) fn newer(
    reported: Option<(&str, u64)>,
    saved: Option<&Saved>,
    saved_from: u64,
) -> Option<String> {
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
pub(super) fn is_plain_id(conversation_id: &str) -> bool {
    !conversation_id.is_empty()
        && conversation_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

/// Where the sink and plugins keep one file per session.
pub(super) fn state_directory() -> Option<PathBuf> {
    Some(crate::util::pade_data_directory()?.join(STATE_DIRECTORY_NAME))
}

pub(super) fn state_file(conversation_id: &str) -> Option<PathBuf> {
    Some(state_directory()?.join(format!("{conversation_id}{STATE_FILE_EXTENSION}")))
}

pub(super) fn read_state(path: &Path) -> Option<SessionState> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

pub(super) fn read_session_state(conversation_id: &str) -> Option<SessionState> {
    read_state(&state_file(conversation_id)?)
}

/// The transcript the agent's own hooks last named for `conversation_id`,
/// when it still exists. A resumed Claude session writes under a new id of its
/// own, so this - not the id PADE pinned at launch - is where it really logs.
pub(crate) fn reported_transcript(conversation_id: &str) -> Option<PathBuf> {
    if !is_plain_id(conversation_id) {
        return None;
    }
    let transcript = PathBuf::from(read_session_state(conversation_id)?.transcript?);
    transcript.is_file().then_some(transcript)
}

/// Record a model / effort PADE itself put in place (at launch, or by a live
/// switch) as the session's newest report.
pub(super) fn record_choice(conversation_id: &str, choice: &ModelChoice) {
    let Some(path) = state_file(conversation_id) else {
        return;
    };
    let now = now_milliseconds();
    let mut state = read_state(&path).unwrap_or_default();
    if let Some(model) = &choice.model {
        state.model = Some(model.clone());
        state.model_at = now;
    }
    if let Some(effort) = &choice.effort {
        state.effort = Some(effort.clone());
        state.effort_at = now;
    }
    if let Err(error) = write_json(&path, &state) {
        eprintln!("pade: could not record the model choice: {error}");
    }
}

/// Write through a sibling file and rename, so a reader never sees half a file.
pub(super) fn write_json(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)?;
    }
    let partial = path.with_extension(PARTIAL_STATE_EXTENSION);
    std::fs::write(&partial, serde_json::to_vec(value)?)?;
    std::fs::rename(partial, path)
}
