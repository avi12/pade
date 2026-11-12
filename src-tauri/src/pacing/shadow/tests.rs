use super::*;
use crate::pacing::account_round::ThrottledSession;
use crate::pacing::units::SpanMilliseconds;
use crate::test_support::assert_empty;

const START: f64 = 5_000_000.0;

fn at(minutes: f64) -> EpochMilliseconds {
    EpochMilliseconds::new(START) + SpanMilliseconds::from_minutes(minutes)
}

fn stop(id: &str) -> Decision {
    Decision::Stop {
        session: SessionId::new(id),
    }
}

fn observed(id: &str, minutes: f64, kind: ObservedKind) -> ObservedDecision {
    ObservedDecision {
        session_id: SessionId::new(id),
        at: at(minutes),
        kind,
    }
}

fn compare(
    state: &mut ShadowState,
    minutes: f64,
    decisions: &[Decision],
    reported: &[ObservedDecision],
) -> Vec<Verdict> {
    state.compare(Comparison {
        now: at(minutes),
        decisions,
        observed: reported,
    })
}

#[test]
fn the_same_step_on_both_sides_agrees() {
    let mut state = ShadowState::default();
    let verdicts = compare(
        &mut state,
        1.0,
        &[stop("a")],
        &[observed("a", 0.5, ObservedKind::Stop)],
    );
    assert_eq!(verdicts.len(), 1);
    assert!(verdicts[0].is_agreement());
    assert_eq!(state.summary().counts[&ObservedKind::Stop].agree, 1);
}

#[test]
fn a_step_only_the_typescript_took_disagrees_at_once() {
    let mut state = ShadowState::default();
    let verdicts = compare(
        &mut state,
        1.0,
        &[],
        &[observed("a", 0.5, ObservedKind::Rest)],
    );
    assert_eq!(verdicts.len(), 1);
    assert_eq!(verdicts[0].observed, Some(ObservedKind::Rest));
    assert_eq!(verdicts[0].decided, None);
    let summary = state.summary();
    assert_eq!(summary.counts[&ObservedKind::Rest].disagree, 1);
    assert_eq!(summary.last_disagreements, verdicts);
}

#[test]
fn a_step_only_the_engine_decided_waits_a_tick_for_the_window() {
    let mut state = ShadowState::default();
    let first = compare(&mut state, 1.0, &[stop("a")], &[]);
    assert_empty(&first);
    assert_empty(&state.summary().last_disagreements);

    let second = compare(&mut state, 2.0, &[], &[]);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].decided, Some(ObservedKind::Stop));
    assert_eq!(second[0].observed, None);
    assert_eq!(state.summary().counts[&ObservedKind::Stop].disagree, 1);
}

#[test]
fn a_step_that_shows_up_in_the_next_report_agrees_with_lag() {
    let mut state = ShadowState::default();
    assert_empty(&compare(&mut state, 1.0, &[stop("a")], &[]));
    let second = compare(
        &mut state,
        2.0,
        &[],
        &[observed("a", 1.5, ObservedKind::Stop)],
    );
    assert_eq!(second.len(), 1);
    assert!(second[0].is_agreement());
    assert_eq!(state.summary().counts[&ObservedKind::Stop].disagree, 0);
}

#[test]
fn a_none_from_the_window_never_counts_on_its_own() {
    let mut state = ShadowState::default();
    let verdicts = compare(
        &mut state,
        1.0,
        &[],
        &[observed("a", 0.5, ObservedKind::None)],
    );
    assert_empty(&verdicts);
    assert!(state.summary().counts.is_empty());
}

#[test]
fn the_same_kind_on_another_session_does_not_agree() {
    let mut state = ShadowState::default();
    let verdicts = compare(
        &mut state,
        1.0,
        &[stop("a")],
        &[observed("b", 0.5, ObservedKind::Stop)],
    );
    assert_eq!(verdicts.len(), 1);
    assert_eq!(verdicts[0].session_id.as_str(), "b");
    assert!(!verdicts[0].is_agreement());
}

#[test]
fn a_different_kind_on_the_same_session_is_two_disagreements() {
    let mut state = ShadowState::default();
    let first = compare(
        &mut state,
        1.0,
        &[stop("a")],
        &[observed("a", 0.5, ObservedKind::Rest)],
    );
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].observed, Some(ObservedKind::Rest));
    let second = compare(&mut state, 2.0, &[], &[]);
    assert_eq!(second[0].decided, Some(ObservedKind::Stop));
}

#[test]
fn the_throttle_and_the_restore_are_not_compared() {
    let mut state = ShadowState::default();
    let decisions = [
        Decision::Throttle {
            session: SessionId::new("a"),
            throttle: ThrottledSession {
                session_id: "a".to_string(),
                name: "a".to_string(),
                is_blocking_spawns: true,
                subagents: Vec::new(),
            },
        },
        Decision::RestoreChoice {
            session: SessionId::new("a"),
        },
    ];
    assert_empty(&compare(&mut state, 1.0, &decisions, &[]));
    assert_empty(&compare(&mut state, 2.0, &[], &[]));
    assert!(state.summary().counts.is_empty());
}

#[test]
fn two_decisions_of_one_kind_claim_one_observation_each() {
    let mut state = ShadowState::default();
    let verdicts = compare(
        &mut state,
        1.0,
        &[stop("a"), stop("a")],
        &[observed("a", 0.5, ObservedKind::Stop)],
    );
    assert_eq!(verdicts.len(), 1);
    let next = compare(&mut state, 2.0, &[], &[]);
    assert_eq!(next.len(), 1);
    assert!(!next[0].is_agreement());
}

#[test]
fn only_the_latest_disagreements_are_kept() {
    let mut state = ShadowState::default();
    let many: Vec<ObservedDecision> = (0..30_u32)
        .map(|index| observed(&format!("s{index}"), 0.5, ObservedKind::Resume))
        .collect();
    let verdicts = compare(&mut state, 1.0, &[], &many);
    assert_eq!(verdicts.len(), 30);
    let summary = state.summary();
    assert_eq!(summary.counts[&ObservedKind::Resume].disagree, 30);
    assert_eq!(summary.last_disagreements.len(), LAST_DISAGREEMENT_LIMIT);
    assert_eq!(summary.last_disagreements[0].session_id.as_str(), "s10");
}

#[test]
fn the_summary_serializes_in_the_shape_the_frontend_reads() {
    let mut state = ShadowState::default();
    compare(
        &mut state,
        1.0,
        &[stop("a")],
        &[observed("a", 0.5, ObservedKind::Stop)],
    );
    compare(
        &mut state,
        1.0,
        &[],
        &[observed("b", 0.5, ObservedKind::Release)],
    );
    let json = serde_json::to_value(state.summary()).expect("serializes");
    assert_eq!(json["counts"]["stop"]["agree"], 1);
    assert_eq!(json["counts"]["release"]["disagree"], 1);
    let last = &json["lastDisagreements"][0];
    assert_eq!(last["sessionId"], "b");
    assert_eq!(last["observed"], "release");
    assert!(last["decided"].is_null());
    assert!(last["at"].is_number());
}

#[test]
fn a_log_line_carries_each_verdict_with_what_the_engine_saw() {
    let verdict = Verdict {
        at: at(1.0),
        session_id: SessionId::new("a"),
        observed: None,
        decided: Some(ObservedKind::Stop),
    };
    let contexts = BTreeMap::from([(
        SessionId::new("a"),
        SessionContext {
            is_working: true,
            pace_phase: None,
            is_stepped_down: false,
            is_pacing_on: true,
        },
    )]);
    let line = log_line(at(1.0), &[verdict], &contexts).expect("a line");
    let json: serde_json::Value = serde_json::from_str(&line).expect("json");
    let entry = &json["entries"][0];
    assert_eq!(entry["isAgreement"], false);
    assert_eq!(entry["decided"], "stop");
    assert_eq!(entry["context"]["isWorking"], true);
    assert!(log_line(at(1.0), &[], &contexts).is_none());
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("pade-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&directory).ok();
    directory
}

#[test]
fn the_log_appends_a_line_per_call() {
    let directory = scratch("shadow-append");
    let log = ShadowLog::new(directory.clone());
    log.append("one").expect("appends");
    log.append("two").expect("appends");
    let written = std::fs::read_to_string(directory.join(SHADOW_LOG_FILE_NAME)).expect("reads");
    std::fs::remove_dir_all(&directory).expect("cleans up");
    assert_eq!(written, "one\ntwo\n");
}

#[test]
fn a_full_log_is_moved_aside_and_started_again() {
    let directory = scratch("shadow-rotate");
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    let full = "x".repeat(usize::try_from(LOG_LIMIT_BYTES).expect("fits"));
    std::fs::write(directory.join(SHADOW_LOG_FILE_NAME), full).expect("fills");

    ShadowLog::new(directory.clone())
        .append("fresh")
        .expect("appends");

    let current = std::fs::read_to_string(directory.join(SHADOW_LOG_FILE_NAME)).expect("reads");
    let rotated = std::fs::metadata(directory.join(ROTATED_LOG_FILE_NAME)).expect("rotated");
    std::fs::remove_dir_all(&directory).expect("cleans up");
    assert_eq!(current, "fresh\n");
    assert_eq!(rotated.len(), LOG_LIMIT_BYTES);
}
