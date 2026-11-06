//! What usage pacing asks of a session mid-turn: the model its subagents may
//! spawn on, whether the session must stop now, and how each of its running
//! subagents is held back. Pacing decides between turns; this is how its
//! decision reaches the work already running inside one.
//!
//! PADE keeps one small control file per paced conversation, rewritten as
//! pacing decides and removed once nothing in it applies. Three parts, each
//! written by its own owner and replaced whole (`agent_set_pacing_control`):
//!
//!   * `cap` - the model a subagent may spawn on while the session is stepped
//!     down (the owning window's model steps);
//!   * `isStopped` - the session is being stopped (pacing's stop or rest step):
//!     every tool call of the session and of each of its subagents is denied
//!     until it is let go, so the stop lands mid-turn, not only at its end;
//!   * `throttle` - the account-wide round's per-subagent steps (hold before
//!     each tool call, or wrap up) and whether new spawns are refused.
//!
//! The agent consults it on every tool call, never at launch, because pacing
//! moves with the usage windows mid-session:
//!
//!   * Claude - a `PreToolUse` hook on every tool runs the sink, which answers
//!     with `answer_tool_use`. Claude names the subagent a call comes from in
//!     the hook input's `agent_id` (absent on the main thread), so each
//!     subagent is held, wrapped up or let be on its own. With no file the
//!     sink prints nothing and exits;
//!   * `OpenCode` - its PADE plugin reads the file when a child session sends
//!     its first message (the cap) and before every tool (the stop and the
//!     spawn block). PADE can't measure an `OpenCode` subagent, so none is held
//!     or wrapped up one by one.
//!
//! Codex subagents take the parent's model unless a custom agent pins one, and
//! its hooks can't hold or refuse a single subagent's tool, so a paced Codex
//! session is paced whole, between turns. Copilot reads hooks only from the
//! user's global config, which PADE does not edit.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use super::{is_plain_id, write_json};
use crate::agents::{ID_CLAUDE, ID_OPENCODE};

/// PADE's data subfolder holding one control file per paced session. A new
/// name: an `OpenCode` session launched before the file grew its parts still
/// reads the old cap folder, and must not misread this shape.
const CONTROL_DIRECTORY_NAME: &str = "pacing-control";
const CONTROL_FILE_EXTENSION: &str = "json";
/// Claude's spawn tool, by its current name and the one it had before.
const CLAUDE_SPAWN_TOOLS: [&str; 2] = ["Agent", "Task"];
/// The tools a stopped session may still run: the ones that stop work.
const CLAUDE_STOPPING_TOOLS: [&str; 3] = ["TaskStop", "KillShell", "KillBash"];
/// The hook event Claude asks before running a tool.
pub(super) const PRE_TOOL_USE: &str = "PreToolUse";
/// How long Claude waits for the `PreToolUse` hook, in seconds - well past the
/// longest hold, so a held call is never cut off as a failed hook.
pub(super) const PRE_TOOL_USE_TIMEOUT_SECONDS: u64 = 2 * MAXIMUM_HOLD_SECONDS;
/// The longest a subagent is held before one tool call, whatever the file says.
const MAXIMUM_HOLD_SECONDS: u64 = 60;
/// The spawn input field naming the subagent's model.
const MODEL_FIELD: &str = "model";
/// Claude's spawn tool takes a model family, not a model id.
const CLAUDE_MODEL_FAMILIES: [&str; 4] = ["opus", "sonnet", "haiku", "fable"];

/// What the agent reads when a refused call comes back, written for the model.
const STOP_REASON: &str = "PADE usage pacing: the usage limit is close, so this session is \
    stopping now. Do not start new work: stop any background tasks, then end your turn with a \
    short note of where you left off.";
const WRAP_UP_REASON: &str = "PADE usage pacing: the usage limit is close. Finish now with what \
    you have: make no more tool calls and report back what you found and what is left.";
const SPAWN_REASON: &str = "PADE usage pacing: the usage limit is close, so no new subagents \
    can start right now. Do this work yourself, or wait for the subagents already running.";

/// One read-modify-write of a control file at a time - every window writes
/// through this one backend, each its own part.
static CONTROL_WRITES: Mutex<()> = Mutex::new(());

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

/// One running subagent's step, keyed by the agent's own subagent id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "camelCase")]
pub enum SubagentStep {
    /// Held this long before each of its tool calls.
    #[serde(rename_all = "camelCase")]
    Gate { hold_seconds: u64 },
    /// Told to finish now: every further tool call is refused.
    WrapUp,
}

/// The account-wide round's part of a session's control.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Throttle {
    is_blocking_spawns: bool,
    subagents: BTreeMap<String, SubagentStep>,
}

impl Throttle {
    fn is_empty(&self) -> bool {
        !self.is_blocking_spawns && self.subagents.is_empty()
    }
}

/// One part of a session's control, replaced whole by the one who owns it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "part", content = "value", rename_all = "camelCase")]
pub enum ControlPart {
    Cap(Option<SubagentCapRequest>),
    Stop(bool),
    Throttle(Throttle),
}

/// The cap as the agent's spawn call spells models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SubagentCap {
    model: String,
    allowed: Vec<String>,
}

/// The control file's shape.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct PacingControl {
    cap: Option<SubagentCap>,
    is_stopped: bool,
    throttle: Throttle,
}

impl PacingControl {
    fn is_empty(&self) -> bool {
        self.cap.is_none() && !self.is_stopped && self.throttle.is_empty()
    }

    /// This control with `part` replaced, spelled for `command`'s agent.
    fn with_part(self, command: &str, part: ControlPart) -> Self {
        match part {
            ControlPart::Cap(request) => Self {
                cap: request.and_then(|request| spawn_cap(command, &request)),
                ..self
            },
            ControlPart::Stop(is_stopped) => Self { is_stopped, ..self },
            ControlPart::Throttle(throttle) => Self { throttle, ..self },
        }
    }
}

/// Replace one part of the control of `command`'s session `conversation_id`;
/// the file goes once nothing in it applies. A part an agent can't act on
/// still lands - the agent just never reads it. Off the UI thread: it writes a
/// file.
#[tauri::command]
pub async fn agent_set_pacing_control(
    command: String,
    conversation_id: String,
    part: ControlPart,
) -> Result<(), String> {
    if !is_plain_id(&conversation_id) {
        return Err("that conversation id isn't valid".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let path = control_file(&conversation_id).ok_or("no PADE data directory")?;
        let _guard = CONTROL_WRITES
            .lock()
            .map_err(|_| "the pacing control lock is poisoned")?;
        let next = read_control(&conversation_id)
            .unwrap_or_default()
            .with_part(&command, part);
        if next.is_empty() {
            return match std::fs::remove_file(&path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    Err(error.to_string())
                }
                _ => Ok(()),
            };
        }
        write_json(&path, &next).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// What the sink does with one tool call: hold it, then answer (or say
/// nothing, which lets the call run as asked).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ToolAnswer {
    hold: Option<Duration>,
    answer: Option<serde_json::Value>,
}

impl ToolAnswer {
    const RUN_AS_ASKED: Self = Self {
        hold: None,
        answer: None,
    };

    fn deny(reason: &str) -> Self {
        Self {
            hold: None,
            answer: Some(pre_tool_use_answer(serde_json::json!({
                "permissionDecision": "deny",
                "permissionDecisionReason": reason
            }))),
        }
    }
}

/// Answer Claude's `PreToolUse` for any tool, from the session's main thread
/// or one of its subagents. The no-op path - no control file - reads one
/// missing file and returns.
pub(super) fn answer_tool_use(conversation_id: &str, report: &serde_json::Value) {
    let Some(control) = read_control(conversation_id) else {
        return;
    };
    let decision = decide(&control, report);
    if let Some(hold) = decision.hold {
        std::thread::sleep(hold);
    }
    if let Some(answer) = decision.answer {
        println!("{answer}");
    }
}

/// Where the session's control lives - also handed to the `OpenCode` plugin.
pub(super) fn control_file(conversation_id: &str) -> Option<PathBuf> {
    Some(
        crate::util::pade_data_directory()?
            .join(CONTROL_DIRECTORY_NAME)
            .join(format!("{conversation_id}.{CONTROL_FILE_EXTENSION}")),
    )
}

fn read_control(conversation_id: &str) -> Option<PacingControl> {
    let text = std::fs::read_to_string(control_file(conversation_id)?).ok()?;
    serde_json::from_str(&text).ok()
}

/// The answer to one tool call, first rule that applies wins: a stopping
/// session runs nothing but what stops work; a subagent told to wrap up runs
/// nothing; a held subagent waits; a spawn is refused while spawns are
/// blocked, else moved onto the cap.
fn decide(control: &PacingControl, report: &serde_json::Value) -> ToolAnswer {
    let tool = report
        .get("tool_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if control.is_stopped && !CLAUDE_STOPPING_TOOLS.contains(&tool) {
        return ToolAnswer::deny(STOP_REASON);
    }

    let subagent = report.get("agent_id").and_then(serde_json::Value::as_str);
    let step = subagent.and_then(|id| control.throttle.subagents.get(id));
    let hold = match step {
        Some(SubagentStep::WrapUp) => return ToolAnswer::deny(WRAP_UP_REASON),
        Some(SubagentStep::Gate { hold_seconds }) => Some(Duration::from_secs(
            (*hold_seconds).min(MAXIMUM_HOLD_SECONDS),
        )),
        None => None,
    };

    if !CLAUDE_SPAWN_TOOLS.contains(&tool) {
        return ToolAnswer {
            hold,
            ..ToolAnswer::RUN_AS_ASKED
        };
    }
    if control.throttle.is_blocking_spawns {
        return ToolAnswer {
            hold,
            ..ToolAnswer::deny(SPAWN_REASON)
        };
    }
    let capped = control.cap.as_ref().and_then(|cap| {
        let input = report.get("tool_input")?;
        capped_input(input, cap)
    });
    ToolAnswer {
        hold,
        answer: capped.map(|capped| {
            pre_tool_use_answer(serde_json::json!({
                "permissionDecision": "allow",
                "updatedInput": capped
            }))
        }),
    }
}

/// `decision` wrapped as Claude reads a `PreToolUse` answer.
fn pre_tool_use_answer(decision: serde_json::Value) -> serde_json::Value {
    let mut output = serde_json::Map::new();
    output.insert(
        "hookEventName".to_string(),
        serde_json::Value::String(PRE_TOOL_USE.to_string()),
    );
    if let serde_json::Value::Object(fields) = decision {
        output.extend(fields);
    }
    serde_json::json!({ "hookSpecificOutput": output })
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
    use super::{
        capped_input, decide, spawn_cap, ControlPart, PacingControl, SubagentCap,
        SubagentCapRequest, SubagentStep, Throttle, ToolAnswer, MAXIMUM_HOLD_SECONDS, SPAWN_REASON,
        STOP_REASON, WRAP_UP_REASON,
    };
    use std::collections::BTreeMap;
    use std::time::Duration;

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

    fn throttle(is_blocking_spawns: bool, subagents: &[(&str, SubagentStep)]) -> Throttle {
        Throttle {
            is_blocking_spawns,
            subagents: subagents
                .iter()
                .map(|(id, step)| ((*id).to_string(), *step))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    fn call(tool: &str, agent: Option<&str>) -> serde_json::Value {
        let mut report = serde_json::json!({
            "hook_event_name": "PreToolUse",
            "tool_name": tool,
            "tool_input": { "prompt": "p", "model": "opus" }
        });
        if let Some(agent) = agent {
            report["agent_id"] = serde_json::Value::String(agent.to_string());
        }
        report
    }

    fn denial(answer: &ToolAnswer) -> Option<&str> {
        let output = answer.answer.as_ref()?.get("hookSpecificOutput")?;
        (output.get("permissionDecision")? == "deny")
            .then(|| output.get("permissionDecisionReason")?.as_str())
            .flatten()
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

    #[test]
    fn each_part_is_replaced_on_its_own_and_round_trips_through_the_file_shape() {
        let control = PacingControl::default()
            .with_part(
                "claude",
                ControlPart::Cap(Some(request("claude-sonnet-5-5", &[]))),
            )
            .with_part(
                "claude",
                ControlPart::Throttle(throttle(
                    true,
                    &[("a1", SubagentStep::Gate { hold_seconds: 12 })],
                )),
            )
            .with_part("claude", ControlPart::Stop(true));
        let text = serde_json::to_string(&control).expect("serialized");
        let read: PacingControl = serde_json::from_str(&text).expect("read back");
        assert_eq!(read, control);
        assert!(text.contains(r#""a1":{"step":"gate","holdSeconds":12}"#));

        let lifted = read
            .with_part("claude", ControlPart::Stop(false))
            .with_part("claude", ControlPart::Throttle(Throttle::default()));
        assert_eq!(
            lifted.cap.as_ref().map(|cap| cap.model.as_str()),
            Some("sonnet")
        );
        assert!(!lifted.is_empty());
        assert!(lifted
            .with_part("claude", ControlPart::Cap(None))
            .is_empty());
    }

    #[test]
    fn the_wire_names_each_part_the_way_the_frontend_sends_it() {
        let part: ControlPart = serde_json::from_value(serde_json::json!({
            "part": "throttle",
            "value": {
                "isBlockingSpawns": false,
                "subagents": { "a1": { "step": "wrapUp" } }
            }
        }))
        .expect("parsed");
        assert_eq!(
            part,
            ControlPart::Throttle(throttle(false, &[("a1", SubagentStep::WrapUp)]))
        );
        let stop: ControlPart =
            serde_json::from_value(serde_json::json!({ "part": "stop", "value": true }))
                .expect("parsed");
        assert_eq!(stop, ControlPart::Stop(true));
    }

    #[test]
    fn an_unpaced_call_runs_as_asked() {
        let control = PacingControl {
            throttle: throttle(false, &[("a1", SubagentStep::WrapUp)]),
            ..PacingControl::default()
        };
        assert_eq!(
            decide(&control, &call("Bash", None)),
            ToolAnswer::RUN_AS_ASKED
        );
        assert_eq!(
            decide(&control, &call("Bash", Some("a2"))),
            ToolAnswer::RUN_AS_ASKED
        );
    }

    #[test]
    fn a_held_subagent_waits_before_its_call_and_the_hold_is_bounded() {
        let control = PacingControl {
            throttle: throttle(
                false,
                &[
                    ("a1", SubagentStep::Gate { hold_seconds: 12 }),
                    ("a2", SubagentStep::Gate { hold_seconds: 900 }),
                ],
            ),
            ..PacingControl::default()
        };
        assert_eq!(
            decide(&control, &call("Read", Some("a1"))),
            ToolAnswer {
                hold: Some(Duration::from_secs(12)),
                answer: None
            }
        );
        assert_eq!(
            decide(&control, &call("Read", Some("a2"))).hold,
            Some(Duration::from_secs(MAXIMUM_HOLD_SECONDS))
        );
        assert_eq!(decide(&control, &call("Read", None)).hold, None);
    }

    #[test]
    fn a_subagent_told_to_wrap_up_is_refused_every_call() {
        let control = PacingControl {
            throttle: throttle(false, &[("a1", SubagentStep::WrapUp)]),
            ..PacingControl::default()
        };
        assert_eq!(
            denial(&decide(&control, &call("Grep", Some("a1")))),
            Some(WRAP_UP_REASON)
        );
    }

    #[test]
    fn spawns_are_refused_while_blocked_from_the_session_and_its_subagents() {
        let control = PacingControl {
            cap: Some(sonnet_cap()),
            throttle: throttle(true, &[]),
            ..PacingControl::default()
        };
        assert_eq!(
            denial(&decide(&control, &call("Agent", None))),
            Some(SPAWN_REASON)
        );
        assert_eq!(
            denial(&decide(&control, &call("Task", Some("a1")))),
            Some(SPAWN_REASON)
        );
        assert_eq!(denial(&decide(&control, &call("Bash", None))), None);
    }

    #[test]
    fn an_unblocked_spawn_still_moves_onto_the_cap() {
        let control = PacingControl {
            cap: Some(sonnet_cap()),
            ..PacingControl::default()
        };
        let answer = decide(&control, &call("Agent", None))
            .answer
            .expect("an answer");
        assert_eq!(answer["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(
            answer["hookSpecificOutput"]["updatedInput"]["model"],
            "sonnet"
        );
    }

    #[test]
    fn a_stopping_session_refuses_every_call_but_the_ones_that_stop_work() {
        let control = PacingControl {
            is_stopped: true,
            ..PacingControl::default()
        };
        assert_eq!(
            denial(&decide(&control, &call("Edit", None))),
            Some(STOP_REASON)
        );
        assert_eq!(
            denial(&decide(&control, &call("Bash", Some("a1")))),
            Some(STOP_REASON)
        );
        assert_eq!(
            decide(&control, &call("TaskStop", None)),
            ToolAnswer::RUN_AS_ASKED
        );
    }
}
