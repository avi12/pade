use super::*;
use crate::pacing::subagent_steps::{
    hold_for_saving, HoldSeconds, MAXIMUM_HOLD_SECONDS, NO_SPAWN_SHED,
};

const NOW: EpochMilliseconds = EpochMilliseconds::new(1_794_052_800_000.0);
const ACCOUNT: &str = "avi@example.com";

fn session(id: &str) -> SessionReport {
    SessionReport {
        id: id.to_string(),
        name: id.to_string(),
        account_key: ACCOUNT.to_string(),
        command: "claude".to_string(),
        conversation_id: Some(format!("{id}-conversation")),
        reported_at: NOW,
        load: Load::new(10.0),
        over_pace: None,
        can_step_up: false,
        has_room: false,
        subagents: Vec::new(),
    }
}

fn over(shed: f64, saving: f64) -> OverPace {
    OverPace {
        shed: Some(Fraction::new(shed)),
        saving: Fraction::new(saving),
    }
}

fn subagent(id: &str, description: Option<&str>, load: f64) -> SubagentReport {
    SubagentReport {
        id: id.to_string(),
        description: description.map(ToString::to_string),
        load: Load::new(load),
    }
}

fn window_report(label: &str, sessions: Vec<SessionReport>) -> WindowReport {
    WindowReport {
        label: label.to_string(),
        time: NOW,
        sessions,
    }
}

fn key(session_id: &str, subagent_id: &str) -> String {
    subagent_key(SubagentKeyParts {
        session_id,
        subagent_id,
    })
}

fn gate(seconds: u32) -> SubagentStep {
    SubagentStep::Gate {
        hold_seconds: HoldSeconds::new(seconds).expect("a positive hold"),
    }
}

fn round_with(
    reports: &[WindowReport],
    steps: &SubagentSteps,
    stepped_at: &HashMap<String, EpochMilliseconds>,
) -> RoundOutcome {
    let references: Vec<&WindowReport> = reports.iter().collect();
    plan_round(PlanRoundParams {
        reports: &references,
        steps,
        stepped_at,
        leader: "main",
        now: NOW,
    })
}

fn round(reports: &[WindowReport]) -> RoundOutcome {
    round_with(reports, &SubagentSteps::new(), &HashMap::new())
}

#[test]
fn the_leader_is_one_window_the_same_whichever_window_asks() {
    let reports = [
        window_report("window-2", vec![]),
        window_report("main", vec![]),
        window_report("window-1", vec![]),
    ];
    let forward: Vec<&WindowReport> = reports.iter().collect();
    let reversed: Vec<&WindowReport> = reports.iter().rev().collect();
    assert_eq!(round_leader(&forward).as_deref(), Some("main"));
    assert_eq!(round_leader(&reversed).as_deref(), Some("main"));
    assert_eq!(round_leader(&[]), None);
}

#[test]
fn a_window_that_stopped_reporting_is_dropped_so_the_next_one_leads() {
    let stale = WindowReport {
        time: NOW - SpanMilliseconds::from_minutes(10.0),
        ..window_report("main", vec![])
    };
    let reports = [stale, window_report("window-1", vec![])];
    let fresh = fresh_reports(FreshReportsParams {
        reports: &reports,
        now: NOW,
        lifetime: SpanMilliseconds::new(150_000.0),
    });
    assert_eq!(round_leader(&fresh).as_deref(), Some("window-1"));
}

#[test]
fn a_report_lives_two_and_a_half_scans() {
    assert_eq!(
        report_lifetime(SpanMilliseconds::new(60_000.0)),
        SpanMilliseconds::new(150_000.0)
    );
}

#[test]
fn steps_the_biggest_saver_wherever_it_lives_not_the_leaders_own_session_first() {
    let front = SessionReport {
        load: Load::new(10.0),
        over_pace: Some(over(0.1, 0.2)),
        ..session("front")
    };
    let background = SessionReport {
        load: Load::new(40.0),
        over_pace: Some(over(0.1, 0.2)),
        ..session("background")
    };
    let outcome = round(&[
        window_report("main", vec![front]),
        window_report("window-1", vec![background]),
    ]);
    assert_eq!(outcome.plan.climb, ["background"]);
    assert_eq!(outcome.plan.stepped_accounts, [ACCOUNT]);
}

#[test]
fn sheds_the_accounts_fraction_of_every_windows_load_once_not_once_per_window() {
    let sessions: Vec<SessionReport> = ["a", "b", "c", "d"]
        .into_iter()
        .map(|id| SessionReport {
            load: Load::new(25.0),
            over_pace: Some(over(0.2, 0.5)),
            ..session(id)
        })
        .collect();
    let outcome = round(&[
        window_report("main", sessions[..2].to_vec()),
        window_report("window-1", sessions[2..].to_vec()),
    ]);
    // 20% of 100 needs 20 saved; each step saves 12.5, so two - not four.
    assert_eq!(outcome.plan.climb.len(), 2);
}

#[test]
fn ranks_a_background_sessions_running_subagent_against_the_sessions_by_its_own_load() {
    let front = SessionReport {
        load: Load::new(5.0),
        over_pace: Some(over(0.1, 0.2)),
        ..session("front")
    };
    let background = SessionReport {
        load: Load::new(5.0),
        subagents: vec![subagent("a1", Some("Explore the repo"), 60.0)],
        ..session("background")
    };
    let outcome = round(&[
        window_report("main", vec![front]),
        window_report("window-1", vec![background]),
    ]);
    assert_eq!(
        outcome.steps.get(&key("background", "a1")),
        Some(&SubagentStep::Gate {
            hold_seconds: hold_for_saving(Fraction::new(0.1))
        })
    );
    assert_eq!(outcome.plan.climb, Vec::<String>::new());
    let throttle = outcome
        .plan
        .throttles
        .iter()
        .find(|entry| entry.session_id == "background")
        .expect("a throttle for the background session");
    let descriptions: Vec<Option<&str>> = throttle
        .subagents
        .iter()
        .map(|entry| entry.description.as_deref())
        .collect();
    assert_eq!(descriptions, [Some("Explore the repo")]);
    assert!(!throttle.is_blocking_spawns);
}

#[test]
fn wraps_up_a_subagent_already_held_the_longest_and_keeps_the_others_steps() {
    let wrapping = key("s", "a1");
    let kept = key("s", "a2");
    let steps = SubagentSteps::from([
        (
            wrapping.clone(),
            SubagentStep::Gate {
                hold_seconds: MAXIMUM_HOLD_SECONDS,
            },
        ),
        (kept.clone(), gate(10)),
    ]);
    let sessions = vec![SessionReport {
        load: Load::new(1.0),
        over_pace: Some(over(0.1, 0.0)),
        subagents: vec![subagent("a1", None, 50.0), subagent("a2", None, 1.0)],
        ..session("s")
    }];
    let outcome = round_with(
        &[window_report("window-1", sessions)],
        &steps,
        &HashMap::new(),
    );
    assert_eq!(outcome.steps.get(&wrapping), Some(&SubagentStep::WrapUp));
    assert_eq!(outcome.steps.get(&kept), steps.get(&kept));
}

#[test]
fn blocks_new_subagents_on_every_session_of_the_account_only_past_the_threshold() {
    let shedding = |shed: f64| -> Vec<bool> {
        let asking = SessionReport {
            over_pace: Some(over(shed, 0.2)),
            ..session("a")
        };
        round(&[
            window_report("main", vec![asking]),
            window_report("window-1", vec![session("b")]),
        ])
        .plan
        .throttles
        .iter()
        .map(|throttle| throttle.is_blocking_spawns)
        .collect()
    };
    assert_eq!(shedding(NO_SPAWN_SHED.get()), [true, true]);
    assert_eq!(shedding(NO_SPAWN_SHED.get() / 2.0), [false, false]);
}

#[test]
fn keeps_other_accounts_out_of_the_round() {
    let other = SessionReport {
        account_key: "someone@example.com".to_string(),
        subagents: vec![subagent("a1", None, 90.0)],
        ..session("other")
    };
    let over_pace = SessionReport {
        over_pace: Some(over(0.5, 0.5)),
        ..session("over")
    };
    let outcome = round(&[
        window_report("main", vec![over_pace]),
        window_report("window-1", vec![other]),
    ]);
    assert!(outcome.steps.is_empty());
    let throttle = outcome
        .plan
        .throttles
        .iter()
        .find(|entry| entry.session_id == "other")
        .expect("a throttle for the other account's session");
    assert!(!throttle.is_blocking_spawns);
}

#[test]
fn does_not_step_a_session_again_on_a_report_older_than_its_last_step() {
    let stale = SessionReport {
        reported_at: NOW - SpanMilliseconds::new(30_000.0),
        over_pace: Some(over(0.5, 0.5)),
        ..session("s")
    };
    let stepped_at = HashMap::from([("s".to_string(), NOW - SpanMilliseconds::new(10_000.0))]);
    let outcome = round_with(
        &[window_report("window-1", vec![stale])],
        &SubagentSteps::new(),
        &stepped_at,
    );
    assert_eq!(outcome.plan.climb, Vec::<String>::new());
}

#[test]
fn with_room_again_on_every_session_eases_holds_and_restores_the_lightest_stepped_down_session() {
    let steps = SubagentSteps::from([
        (key("a", "a1"), gate(20)),
        (key("a", "finished"), SubagentStep::WrapUp),
    ]);
    let roomy = SessionReport {
        has_room: true,
        subagents: vec![subagent("a1", None, 5.0)],
        ..session("a")
    };
    let light = SessionReport {
        load: Load::new(2.0),
        has_room: true,
        can_step_up: true,
        ..session("light")
    };
    let heavy = SessionReport {
        load: Load::new(20.0),
        has_room: true,
        can_step_up: true,
        ..session("heavy")
    };
    let outcome = round_with(
        &[
            window_report("main", vec![roomy]),
            window_report("window-1", vec![light, heavy]),
        ],
        &steps,
        &HashMap::new(),
    );
    assert_eq!(
        outcome.steps,
        SubagentSteps::from([(key("a", "a1"), gate(10))])
    );
    assert_eq!(outcome.plan.restore, ["light"]);
}

#[test]
fn holds_the_steps_while_some_session_on_the_account_has_no_room_yet() {
    let steps = SubagentSteps::from([(key("a", "a1"), gate(20))]);
    let roomy = SessionReport {
        has_room: true,
        subagents: vec![subagent("a1", None, 5.0)],
        ..session("a")
    };
    let outcome = round_with(
        &[
            window_report("main", vec![roomy]),
            window_report("window-1", vec![session("b")]),
        ],
        &steps,
        &HashMap::new(),
    );
    assert_eq!(outcome.steps, steps);
}

#[test]
fn a_well_formed_plan_crosses_the_wire_and_a_malformed_one_is_refused() {
    let outcome = round(&[window_report("main", vec![session("s")])]);
    let message = PacingMessage::Plan { plan: outcome.plan };
    let wire = serde_json::to_value(&message).expect("serializes");
    assert_eq!(wire["kind"], "plan");
    let parsed: Result<PacingMessage, _> = serde_json::from_value(wire);
    assert_eq!(parsed.expect("parses back"), message);
    let malformed = serde_json::json!({ "kind": "plan", "plan": { "leader": 1 } });
    assert!(serde_json::from_value::<PacingMessage>(malformed).is_err());
}

#[test]
fn writes_a_throttle_as_the_control_file_holds_it() {
    let throttle = ThrottledSession {
        session_id: "s".to_string(),
        name: "s".to_string(),
        is_blocking_spawns: false,
        subagents: vec![ThrottledSubagent {
            id: "a1".to_string(),
            description: None,
            step: SubagentStep::WrapUp,
        }],
    };
    assert!(is_throttling(&throttle));
    let wire = serde_json::to_value(control_throttle(&throttle)).expect("serializes");
    assert_eq!(
        wire,
        serde_json::json!({
            "isBlockingSpawns": false,
            "subagents": { "a1": { "step": "wrapUp" } }
        })
    );
    let idle = ThrottledSession {
        subagents: Vec::new(),
        ..throttle
    };
    assert!(!is_throttling(&idle));
}
