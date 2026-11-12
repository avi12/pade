use super::{
    capped_input, decide, hold_through_rest, spawn_cap, ControlPart, EpochMilliseconds,
    PacingControl, RestClock, SubagentCap, SubagentCapRequest, SubagentStep, Throttle, ToolAnswer,
    MAXIMUM_HOLD_SECONDS, MAXIMUM_REST_HOLD, SPAWN_REASON, STOP_REASON, WRAP_UP_REASON,
};
use std::cell::Cell;
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

const NOW: EpochMilliseconds = EpochMilliseconds(1_000_000);
const SLICE: Duration = Duration::from_secs(1);
const SHORT_REST: Duration = Duration::from_millis(3500);
const LONG_REST: Duration = Duration::from_mins(50);

fn resting_until(rest_until: EpochMilliseconds) -> PacingControl {
    PacingControl {
        rest_until: Some(rest_until),
        ..PacingControl::default()
    }
}

fn resting_for(rest: Duration) -> PacingControl {
    resting_until(NOW.after(rest))
}

/// Hold a `Bash` call against `control`, re-reading `rereads` in turn (a
/// `None` is a vanished file); returns the outcome and how long it waited.
fn held_for(
    control: PacingControl,
    rereads: Vec<Option<PacingControl>>,
) -> (Option<PacingControl>, Duration) {
    let elapsed = Cell::new(Duration::ZERO);
    let mut pending = rereads.into_iter();
    let outcome = hold_through_rest(
        control,
        "Bash",
        &mut RestClock {
            now: || NOW.after(elapsed.get()),
            pause: |slice| elapsed.set(elapsed.get() + slice),
            reread: || pending.next().unwrap_or(Some(resting_for(SHORT_REST))),
        },
    );
    (outcome, elapsed.get())
}

#[test]
fn the_rest_part_has_its_wire_shape_and_round_trips() {
    let part: ControlPart =
        serde_json::from_value(serde_json::json!({ "part": "rest", "value": 5000 }))
            .expect("parsed");
    assert_eq!(part, ControlPart::Rest(Some(EpochMilliseconds(5000))));
    let lifted: ControlPart =
        serde_json::from_value(serde_json::json!({ "part": "rest", "value": null }))
            .expect("parsed");
    assert_eq!(lifted, ControlPart::Rest(None));

    let control = PacingControl::default().with_part("claude", part);
    let text = serde_json::to_string(&control).expect("serialized");
    assert!(text.contains(r#""restUntil":5000"#));
    let read: PacingControl = serde_json::from_str(&text).expect("read back");
    assert_eq!(read, control);
}

#[test]
fn a_rest_keeps_the_control_alive_until_lifted() {
    let control = resting_until(NOW);
    assert!(!control.is_empty());
    assert!(control.is_resting(EpochMilliseconds(0)));
    assert!(control
        .with_part("claude", ControlPart::Rest(None))
        .is_empty());
}

#[test]
fn only_a_live_rest_holds_and_never_the_tools_that_stop_work() {
    let control = resting_for(Duration::from_millis(1));
    assert!(control.is_holding("Bash", NOW));
    assert!(!control.is_holding("Bash", NOW.after(Duration::from_millis(1))));
    assert!(!control.is_holding("TaskStop", NOW));
    let stopped = PacingControl {
        is_stopped: true,
        ..control
    };
    assert!(!stopped.is_holding("Bash", NOW));
}

#[test]
fn a_held_call_waits_out_the_rest_then_runs() {
    let (control, waited) = held_for(resting_for(SHORT_REST), vec![]);
    assert_eq!(waited, Duration::from_secs(4));
    assert!(control.is_some_and(|control| !control.is_resting(NOW.after(waited))));
}

#[test]
fn a_lifted_rest_wakes_a_held_call_at_once() {
    let (control, waited) = held_for(resting_for(LONG_REST), vec![Some(PacingControl::default())]);
    assert_eq!(waited, SLICE);
    assert_eq!(control, Some(PacingControl::default()));
}

#[test]
fn a_stop_wakes_a_held_call_to_be_denied() {
    let stopped = PacingControl {
        is_stopped: true,
        rest_until: Some(NOW.after(LONG_REST)),
        ..PacingControl::default()
    };
    let (control, waited) = held_for(resting_for(LONG_REST), vec![Some(stopped.clone())]);
    assert_eq!(waited, SLICE);
    assert_eq!(control.as_ref(), Some(&stopped));
    assert_eq!(
        denial(&decide(&stopped, &call("Edit", None))),
        Some(STOP_REASON)
    );
}

#[test]
fn a_vanished_control_file_ends_the_hold_with_nothing_to_answer() {
    let (control, _) = held_for(resting_for(LONG_REST), vec![None]);
    assert_eq!(control, None);
}

#[test]
fn no_hold_outlasts_the_maximum() {
    let endless = resting_until(EpochMilliseconds(u64::MAX));
    let rereads = vec![Some(endless.clone()); 4000];
    let (control, waited) = held_for(endless, rereads);
    assert!(control.is_some());
    assert_eq!(waited, MAXIMUM_REST_HOLD);
}

#[test]
fn a_call_no_rest_holds_runs_without_waiting() {
    let (control, waited) = held_for(PacingControl::default(), vec![]);
    assert_eq!(waited, Duration::ZERO);
    assert_eq!(control, Some(PacingControl::default()));
}
