//! What usage pacing asks of a session mid-turn: the model its subagents may
//! spawn on, whether the session must stop now, and how each of its running
//! subagents is held back. Pacing decides between turns; this is how its
//! decision reaches the work already running inside one.
//!
//! PADE keeps one small control file per paced conversation, rewritten as
//! pacing decides and removed once nothing in it applies. Four parts, each
//! written by its own owner and replaced whole (`agent_set_pacing_control`):
//!
//!   * `cap` - the model a subagent may spawn on while the session is stepped
//!     down (the owning window's model steps);
//!   * `isStopped` - the session is being stopped (pacing's stop or rest step):
//!     every tool call of the session and of each of its subagents is denied
//!     until it is let go, so the stop lands mid-turn, not only at its end;
//!   * `restUntil` - the session is resting until then (epoch ms): every tool
//!     call of the session and of each of its subagents is held, not refused,
//!     until the rest ends or is lifted, so the work pauses where it stands and
//!     nothing is killed;
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
use crate::util::now_milliseconds;

/// PADE's data subfolder holding one control file per paced session. A new
/// name: an `OpenCode` session launched before the file grew its parts still
/// reads the old cap folder, and must not misread this shape.
const CONTROL_DIRECTORY_NAME: &str = "pacing-control";
const CONTROL_FILE_EXTENSION: &str = "json";
/// Beside a control file: when the session's hook last answered a call while
/// it was being stopped (epoch ms) - how PADE tells a hook that takes the stop
/// from a session that predates it, or one that hasn't called a tool since.
const STOP_SEEN_EXTENSION: &str = "stop-seen";
/// Claude's spawn tool, by its current name and the one it had before.
const CLAUDE_SPAWN_TOOLS: [&str; 2] = ["Agent", "Task"];
/// The tools a stopped session may still run: the ones that stop work.
const CLAUDE_STOPPING_TOOLS: [&str; 3] = ["TaskStop", "KillShell", "KillBash"];
/// The hook event Claude asks before running a tool.
pub(super) const PRE_TOOL_USE: &str = "PreToolUse";
/// How long Claude waits for the `PreToolUse` hook, in seconds - well past the
/// longest rest hold plus a subagent's own hold, so a held call is never cut
/// off as a failed hook.
pub(super) const PRE_TOOL_USE_TIMEOUT_SECONDS: u64 =
    MAXIMUM_REST_HOLD_SECONDS + 2 * MAXIMUM_HOLD_SECONDS;
const _: () = assert!(
    PRE_TOOL_USE_TIMEOUT_SECONDS > MAXIMUM_REST_HOLD_SECONDS + MAXIMUM_HOLD_SECONDS,
    "the hook timeout must outlast the longest hold"
);
/// The longest a subagent is held before one tool call, whatever the file says.
const MAXIMUM_HOLD_SECONDS: u64 = 60;
/// The longest one tool call is held through a rest, whatever the file says.
/// Mirrored by `MAXIMUM_REST_HOLD_MS` in `usagePacing.svelte.ts`, which writes
/// no longer rest.
const MAXIMUM_REST_HOLD_SECONDS: u64 = 55 * 60;
/// How often a held call re-reads the control, so a lifted rest or a stop wakes
/// it within this long.
const REST_POLL_INTERVAL: Duration = Duration::from_secs(1);
const MAXIMUM_REST_HOLD: Duration = Duration::from_secs(MAXIMUM_REST_HOLD_SECONDS);

/// A moment as epoch milliseconds - the unit pacing writes a rest's end in and
/// the control file keeps it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct EpochMilliseconds(u64);

impl EpochMilliseconds {
    fn now() -> Self {
        Self(now_milliseconds())
    }

    /// This moment pushed on by `duration`, saturating at the far end of time.
    fn after(self, duration: Duration) -> Self {
        let milliseconds = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        Self(self.0.saturating_add(milliseconds))
    }
}
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
pub(crate) struct SubagentCapRequest {
    /// The model a subagent runs when it asks for nothing the cap allows.
    model: String,
    /// Every model a subagent may keep when it asks for it - the cap's own and
    /// the cheaper ones.
    allowed_models: Vec<String>,
}

/// One running subagent's step, keyed by the agent's own subagent id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "camelCase")]
pub(crate) enum SubagentStep {
    /// Held this long before each of its tool calls.
    #[serde(rename_all = "camelCase")]
    Gate { hold_seconds: u64 },
    /// Told to finish now: every further tool call is refused.
    WrapUp,
}

/// The account-wide round's part of a session's control.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Throttle {
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
pub(crate) enum ControlPart {
    Cap(Option<SubagentCapRequest>),
    Stop(bool),
    /// Resting until this epoch ms; `None` lifts the rest.
    Rest(Option<EpochMilliseconds>),
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
    rest_until: Option<EpochMilliseconds>,
    throttle: Throttle,
}

impl PacingControl {
    fn is_empty(&self) -> bool {
        self.cap.is_none()
            && !self.is_stopped
            && self.rest_until.is_none()
            && self.throttle.is_empty()
    }

    /// Whether a rest is on at `now` (epoch ms).
    fn is_resting(&self, now: EpochMilliseconds) -> bool {
        self.rest_until.is_some_and(|until| until > now)
    }

    /// Whether `tool`'s call waits out the rest: a stopping session denies
    /// instead, and the tools that stop work are never held.
    fn is_holding(&self, tool: &str, now: EpochMilliseconds) -> bool {
        !self.is_stopped && self.is_resting(now) && !CLAUDE_STOPPING_TOOLS.contains(&tool)
    }

    /// This control with `part` replaced, spelled for `command`'s agent.
    fn with_part(self, command: &str, part: ControlPart) -> Self {
        match part {
            ControlPart::Cap(request) => Self {
                cap: request.and_then(|request| spawn_cap(command, &request)),
                ..self
            },
            ControlPart::Stop(is_stopped) => Self { is_stopped, ..self },
            ControlPart::Rest(rest_until) => Self { rest_until, ..self },
            ControlPart::Throttle(throttle) => Self { throttle, ..self },
        }
    }
}

/// Replace one part of the control of `command`'s session `conversation_id`;
/// the file goes once nothing in it applies. A part an agent can't act on
/// still lands - the agent just never reads it. Off the UI thread: it writes a
/// file.
#[tauri::command]
pub(crate) async fn agent_set_pacing_control(
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
        if !next.is_stopped && next.rest_until.is_none() {
            forget_stop_seen(&conversation_id);
        }
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
    let Some(first) = read_control(conversation_id) else {
        return;
    };
    if first.is_holding(&tool_name(report), EpochMilliseconds::now()) {
        stamp_stop_seen(conversation_id);
    }
    let Some(control) = hold_through_rest(
        first,
        &tool_name(report),
        &mut RestClock {
            now: EpochMilliseconds::now,
            pause: std::thread::sleep,
            reread: || read_control(conversation_id),
        },
    ) else {
        return;
    };
    let decision = decide(&control, report);
    if control.is_stopped {
        stamp_stop_seen(conversation_id);
    }
    if let Some(hold) = decision.hold {
        std::thread::sleep(hold);
    }
    if let Some(answer) = decision.answer {
        println!("{answer}");
    }
}

/// How a held call tells the time, waits a slice and re-reads the control -
/// each swappable so the hold is testable without sleeping.
struct RestClock<Now, Pause, Reread> {
    now: Now,
    pause: Pause,
    reread: Reread,
}

/// `control` once `tool`'s call is no longer held: at once when no rest holds
/// it, else after the rest ends, is lifted, turns into a stop, or the longest
/// hold passes. `None` when the control file went away meanwhile.
fn hold_through_rest(
    mut control: PacingControl,
    tool: &str,
    clock: &mut RestClock<
        impl FnMut() -> EpochMilliseconds,
        impl FnMut(Duration),
        impl FnMut() -> Option<PacingControl>,
    >,
) -> Option<PacingControl> {
    let give_up_at = (clock.now)().after(MAXIMUM_REST_HOLD);
    loop {
        let now = (clock.now)();
        let is_held = control.is_holding(tool, now) && now < give_up_at;
        if !is_held {
            return Some(control);
        }
        (clock.pause)(REST_POLL_INTERVAL);
        control = (clock.reread)()?;
    }
}

fn tool_name(report: &serde_json::Value) -> String {
    report
        .get("tool_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Where the session's control lives - also handed to the `OpenCode` plugin.
pub(super) fn control_file(conversation_id: &str) -> Option<PathBuf> {
    Some(
        crate::util::pade_data_directory()?
            .join(CONTROL_DIRECTORY_NAME)
            .join(format!("{conversation_id}.{CONTROL_FILE_EXTENSION}")),
    )
}

/// When the session's hook last answered a call while it was being stopped
/// (epoch ms), or `None` when it hasn't since the stop was written - usage
/// pacing's cue to stop the session through its terminal instead. Off the UI
/// thread: it reads a file.
#[tauri::command]
pub(crate) async fn agent_pacing_stop_seen(conversation_id: String) -> Option<u64> {
    if !is_plain_id(&conversation_id) {
        return None;
    }
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::read_to_string(stop_seen_file(&conversation_id)?)
            .ok()?
            .trim()
            .parse()
            .ok()
    })
    .await
    .ok()
    .flatten()
}

fn stop_seen_file(conversation_id: &str) -> Option<PathBuf> {
    Some(control_file(conversation_id)?.with_extension(STOP_SEEN_EXTENSION))
}

/// Best-effort: a missed stamp only means PADE also stops the session through
/// its terminal, which the hook's own refusal already made harmless.
fn stamp_stop_seen(conversation_id: &str) {
    let Some(path) = stop_seen_file(conversation_id) else {
        return;
    };
    if let Err(error) = std::fs::write(path, now_milliseconds().to_string()) {
        eprintln!("pade: could not note the hook took the stop: {error}");
    }
}

fn forget_stop_seen(conversation_id: &str) {
    let Some(path) = stop_seen_file(conversation_id) else {
        return;
    };
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            eprintln!("pade: could not clear the hook's stop note: {error}");
        }
    }
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
    let tool = tool_name(report);
    let tool = tool.as_str();
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
mod tests;
