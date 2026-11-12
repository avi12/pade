//! The engine's scenarios: one test per `decide` branch, ported from the
//! behaviours `usage-pacing.test.ts`, `usage-pace.test.ts` and
//! `account-round.test.ts` hold the TypeScript to.

use super::*;
use crate::pacing::account_round::SubagentReport;
use crate::pacing::facts::{CanStepDown, ModelChoiceFacts, ObservedKind, WindowLabel};
use crate::pacing::samples::RecordParams;
use crate::pacing::test_support::{account, session_window, weekly_window, NOW};
use crate::pacing::units::BurnRate;
use crate::test_support::assert_empty;

const ACCOUNT: &str = "anthropic";
const AGENT: &str = "claude";
const REPORTED_SECONDS_AGO: f64 = 10.0;
const MEASURED_READINGS: u32 = 12;

type Steps = Vec<(String, ObservedKind)>;

fn minutes(count: f64) -> SpanMilliseconds {
    SpanMilliseconds::from_minutes(count)
}

fn account_key() -> AccountKey {
    AccountKey::new(ACCOUNT)
}

/// A working session of the one agent, pacing on, nothing held or stepped.
fn session(id: &str) -> SessionFacts {
    SessionFacts {
        id: SessionId::new(id),
        conversation_id: None,
        agent_id: AgentId::new(AGENT),
        command: AGENT.to_string(),
        name: id.to_string(),
        is_working: true,
        background_agent_count: 0,
        is_pacing_on: true,
        is_handing_off: false,
        has_usage_limit: false,
        is_skipped: false,
        is_continued_run: false,
        is_shielded: false,
        is_model_switch_applying: false,
        working_since: Some(NOW - minutes(30.0)),
        working_share: Fraction::WHOLE,
        own_load: Load::new(1.0),
        subagents: Vec::new(),
        choice: None,
        pace: None,
    }
}

fn choice(can_effort: bool, can_model: bool) -> ChoiceFacts {
    ChoiceFacts {
        model: Some("opus".to_string()),
        effort: Some("high".to_string()),
        original: None,
        can_step_down: CanStepDown {
            effort: can_effort,
            model: can_model,
        },
        can_step_up: false,
        restore_at: None,
        effort_saving: Fraction::new(0.25),
        model_saving: Fraction::new(0.5),
    }
}

/// A session pacing has stepped down, going back at `restore_at`.
fn stepped_down(id: &str, load: f64, restore_at: EpochMilliseconds) -> SessionFacts {
    let mut stepped = session(id);
    stepped.own_load = Load::new(load);
    stepped.choice = Some(ChoiceFacts {
        can_step_up: true,
        restore_at: Some(restore_at),
        original: Some(ModelChoiceFacts {
            model: Some("opus".to_string()),
            effort: Some("xhigh".to_string()),
        }),
        ..choice(true, true)
    });
    stepped
}

fn paused(phase: PacePhase) -> PaceFacts {
    PaceFacts {
        phase,
        window_kind: PacedWindowKind::Session,
        ready_at: NOW + minutes(60.0),
        continue_at: NOW + minutes(170.0),
        rest_until: None,
        has_stopped_work: true,
        is_held: false,
        paused_at: NOW - minutes(5.0),
        rates: BTreeMap::new(),
    }
}

fn paused_session(id: &str, phase: PacePhase) -> SessionFacts {
    let mut held = session(id);
    held.is_working = false;
    held.pace = Some(paused(phase));
    held
}

/// A pause remembering the burn rate of the five-hour window it was held at.
fn paused_with_rate(id: &str, phase: PacePhase, per_minute: f64) -> SessionFacts {
    let mut held = paused_session(id, phase);
    let rate = BurnRate::new(per_minute / 60_000.0);
    held.pace
        .as_mut()
        .expect("a pause")
        .rates
        .insert("five_hour".to_string(), rate);
    held
}

fn live(facts: SessionFacts) -> LiveSession {
    LiveSession {
        window: WindowLabel::new("main"),
        reported_at: NOW - SpanMilliseconds::new(REPORTED_SECONDS_AGO * 1_000.0),
        facts,
    }
}

fn unread() -> BTreeMap<AgentId, AccountReading> {
    BTreeMap::from([(
        AgentId::new(AGENT),
        AccountReading {
            key: account_key(),
            usage: None,
        },
    )])
}

fn accounts_of(windows: Vec<UsageWindow>) -> BTreeMap<AgentId, AccountReading> {
    BTreeMap::from([(
        AgentId::new(AGENT),
        AccountReading {
            key: account_key(),
            usage: Some(account(windows)),
        },
    )])
}

/// The five-hour window at `used`, resetting in three hours (the pace formula
/// frees it ten minutes earlier).
fn session_account(used: f64) -> BTreeMap<AgentId, AccountReading> {
    accounts_of(vec![session_window(
        used,
        SpanMilliseconds::from_hours(3.0),
    )])
}

/// Twelve minutes of working readings ending at `used`, the window rising
/// `per_minute` percent a minute - enough for a measured burn rate.
fn measured(used: f64, per_minute: f64) -> SampleBook {
    let mut book = SampleBook::default();
    for reading in 0..=MEASURED_READINGS {
        let ago = f64::from(MEASURED_READINGS - reading);
        let window = session_window(
            per_minute.mul_add(-ago, used),
            SpanMilliseconds::from_hours(3.0),
        );
        book.record(RecordParams {
            account_key: &account_key(),
            windows: std::slice::from_ref(&window),
            now: NOW - minutes(ago),
        });
    }
    book
}

fn run_with(
    sessions: Vec<SessionFacts>,
    accounts: &BTreeMap<AgentId, AccountReading>,
    samples: &SampleBook,
    memory: &EngineMemory,
) -> EngineOutcome {
    let live_sessions: Vec<LiveSession> = sessions.into_iter().map(live).collect();
    decide(&EngineInputs {
        now: NOW,
        sessions: &live_sessions,
        accounts,
        samples,
        memory,
    })
}

fn run(
    sessions: Vec<SessionFacts>,
    accounts: &BTreeMap<AgentId, AccountReading>,
    samples: &SampleBook,
) -> EngineOutcome {
    run_with(sessions, accounts, samples, &EngineMemory::default())
}

/// What the pass would do, as the TypeScript would have recorded it.
fn steps(outcome: &EngineOutcome) -> Steps {
    outcome
        .decisions
        .iter()
        .filter_map(|decision| {
            decision
                .observed_kind()
                .map(|kind| (decision.session().as_str().to_string(), kind))
        })
        .collect()
}

/// The sessions the pass would put back on their own choice.
fn restored(outcome: &EngineOutcome) -> Vec<String> {
    outcome
        .decisions
        .iter()
        .filter_map(|decision| match decision {
            Decision::RestoreChoice { session } => Some(session.as_str().to_string()),
            _ => None,
        })
        .collect()
}

fn only(id: &str, kind: ObservedKind) -> Steps {
    vec![(id.to_string(), kind)]
}

// --- a working session over pace climbs the ladder ---

#[test]
fn a_session_far_over_pace_with_no_cheaper_choice_is_stopped() {
    let outcome = run(
        vec![session("a")],
        &session_account(80.0),
        &measured(80.0, 1.0),
    );
    assert_eq!(steps(&outcome), only("a", ObservedKind::Stop));
}

#[test]
fn a_session_that_resting_can_save_rests_until_its_rhythm_ends() {
    let outcome = run(
        vec![session("a")],
        &session_account(62.0),
        &measured(62.0, 0.4),
    );
    assert_eq!(steps(&outcome), only("a", ObservedKind::Rest));
    let rest_end = outcome
        .decisions
        .iter()
        .find_map(|decision| match decision {
            Decision::Rest { until, .. } => Some(*until),
            _ => None,
        });
    let until = rest_end.expect("a rest has an end");
    assert!(until > NOW, "the rest ends after now");
    assert!(until <= NOW + minutes(75.0), "never past the stop's end");
}

#[test]
fn a_session_that_rested_just_now_works_its_stretch_before_resting_again() {
    let mut recent = session("a");
    recent.working_since = Some(NOW - minutes(1.0));
    let outcome = run(vec![recent], &session_account(62.0), &measured(62.0, 0.4));
    assert_eq!(steps(&outcome), Steps::new());
}

#[test]
fn a_weekly_window_rests_in_short_bursts_where_a_session_window_would_stop() {
    let accounts = accounts_of(vec![weekly_window(70.0, SpanMilliseconds::from_days(5.0))]);
    let outcome = run(vec![session("a")], &accounts, &SampleBook::default());
    assert_eq!(steps(&outcome), only("a", ObservedKind::Rest));
}

#[test]
fn effort_steps_down_first() {
    let mut stepping = session("a");
    stepping.choice = Some(choice(true, true));
    let outcome = run(vec![stepping], &session_account(80.0), &measured(80.0, 1.0));
    assert_eq!(steps(&outcome), only("a", ObservedKind::StepEffortDown));
}

#[test]
fn the_model_steps_down_once_effort_has_nowhere_to_go() {
    let mut stepping = session("a");
    stepping.choice = Some(choice(false, true));
    let outcome = run(vec![stepping], &session_account(80.0), &measured(80.0, 1.0));
    assert_eq!(steps(&outcome), only("a", ObservedKind::StepModelDown));
}

#[test]
fn a_bottomed_out_choice_goes_on_to_the_stop() {
    let mut bottomed = session("a");
    bottomed.choice = Some(choice(false, false));
    let outcome = run(vec![bottomed], &session_account(80.0), &measured(80.0, 1.0));
    assert_eq!(steps(&outcome), only("a", ObservedKind::Stop));
}

#[test]
fn an_account_on_pace_decides_nothing_for_a_working_session() {
    let outcome = run(
        vec![session("a")],
        &session_account(30.0),
        &measured(30.0, 0.1),
    );
    assert_empty(&outcome.decisions);
}

#[test]
fn a_session_window_without_a_measured_burn_is_not_judged() {
    let outcome = run(
        vec![session("a")],
        &session_account(90.0),
        &SampleBook::default(),
    );
    assert_empty(&outcome.decisions);
}

#[test]
fn an_unreadable_account_decides_nothing() {
    let outcome = run(vec![session("a")], &unread(), &SampleBook::default());
    assert_empty(&outcome.decisions);
}

#[test]
fn an_agent_with_no_reading_at_all_decides_nothing() {
    let outcome = run(vec![session("a")], &BTreeMap::new(), &SampleBook::default());
    assert_empty(&outcome.decisions);
}

#[test]
fn an_idle_session_with_nothing_held_is_not_looked_at() {
    let mut idle = session("a");
    idle.is_working = false;
    let outcome = run(vec![idle], &session_account(80.0), &measured(80.0, 1.0));
    assert_empty(&outcome.decisions);
}

#[test]
fn the_account_sheds_through_its_heaviest_saver_first() {
    let mut light = session("light");
    light.own_load = Load::new(1.0);
    let mut heavy = session("heavy");
    heavy.own_load = Load::new(3.0);
    let outcome = run(
        vec![light, heavy],
        &session_account(62.0),
        &measured(62.0, 0.4),
    );
    assert_eq!(steps(&outcome), only("heavy", ObservedKind::Rest));
}

// --- sessions pacing leaves alone ---

#[test]
fn a_session_pacing_leaves_alone_is_never_stepped() {
    let leave_alone: [fn(&mut SessionFacts); 4] = [
        |session| session.is_skipped = true,
        |session| session.is_model_switch_applying = true,
        |session| session.is_continued_run = true,
        |session| session.is_shielded = true,
    ];
    for tweak in leave_alone {
        let mut left = session("a");
        tweak(&mut left);
        let outcome = run(vec![left], &session_account(80.0), &measured(80.0, 1.0));
        assert_empty(&outcome.decisions);
    }
}

#[test]
fn a_session_owned_elsewhere_gives_up_its_pause_and_is_otherwise_ignored() {
    let owners: [fn(&mut SessionFacts); 2] = [
        |session| session.has_usage_limit = true,
        |session| session.is_handing_off = true,
    ];
    for owner in owners {
        let mut paused_one = paused_session("a", PacePhase::Paused);
        owner(&mut paused_one);
        let outcome = run(
            vec![paused_one],
            &session_account(80.0),
            &measured(80.0, 1.0),
        );
        assert_eq!(steps(&outcome), only("a", ObservedKind::Release));

        let mut working = session("b");
        owner(&mut working);
        let outcome = run(vec![working], &session_account(80.0), &measured(80.0, 1.0));
        assert_empty(&outcome.decisions);
    }
}

#[test]
fn pacing_turned_off_continues_a_pause_and_restores_a_stepped_choice() {
    let mut held = paused_session("held", PacePhase::Paused);
    held.is_pacing_on = false;
    let mut stepped = stepped_down("stepped", 1.0, NOW + minutes(60.0));
    stepped.is_pacing_on = false;
    let mut working = session("working");
    working.is_pacing_on = false;
    let outcome = run(
        vec![held, stepped, working],
        &session_account(80.0),
        &measured(80.0, 1.0),
    );
    assert_eq!(steps(&outcome), only("held", ObservedKind::Resume));
    assert_eq!(restored(&outcome), vec!["stepped".to_string()]);
}

// --- stepped-down sessions go back ---

#[test]
fn a_stepped_choice_is_restored_as_its_window_is_about_to_reset() {
    let due = stepped_down("a", 1.0, NOW - minutes(1.0));
    let outcome = run(vec![due], &session_account(80.0), &measured(80.0, 1.0));
    assert_eq!(restored(&outcome), vec!["a".to_string()]);
}

#[test]
fn a_restored_choice_is_not_also_stepped_up() {
    let due = stepped_down("a", 1.0, NOW - minutes(1.0));
    let outcome = run(vec![due], &session_account(30.0), &measured(30.0, 0.1));
    assert_eq!(restored(&outcome), vec!["a".to_string()]);
    assert_eq!(steps(&outcome), Steps::new());
}

#[test]
fn with_room_to_spare_the_lightest_stepped_session_climbs_back_up() {
    let heavy = stepped_down("heavy", 3.0, NOW + minutes(120.0));
    let light = stepped_down("light", 1.0, NOW + minutes(120.0));
    let outcome = run(
        vec![heavy, light],
        &session_account(30.0),
        &measured(30.0, 0.1),
    );
    assert_eq!(steps(&outcome), only("light", ObservedKind::StepUp));
    assert_eq!(outcome.stepped_accounts, vec![account_key()]);
}

#[test]
fn a_stepped_session_over_pace_does_not_climb_back() {
    let stepped = stepped_down("a", 1.0, NOW + minutes(120.0));
    let outcome = run(vec![stepped], &session_account(80.0), &measured(80.0, 1.0));
    assert_eq!(steps(&outcome), only("a", ObservedKind::StepEffortDown));
}

#[test]
fn a_session_with_no_step_to_undo_does_not_step_up() {
    let outcome = run(
        vec![session("a")],
        &session_account(30.0),
        &measured(30.0, 0.1),
    );
    assert_empty(&outcome.decisions);
    assert_empty(&outcome.stepped_accounts);
}

#[test]
fn an_idle_stepped_session_is_still_looked_at() {
    let mut idle = stepped_down("a", 1.0, NOW + minutes(120.0));
    idle.is_working = false;
    let outcome = run(vec![idle], &session_account(30.0), &measured(30.0, 0.1));
    assert_empty(&outcome.decisions);
}

// --- the round's memory ---

#[test]
fn a_session_stepped_since_its_report_waits_for_a_fresh_one() {
    let mut memory = EngineMemory::default();
    memory.stepped_at.insert("a".to_string(), NOW);
    let outcome = run_with(
        vec![session("a")],
        &session_account(80.0),
        &measured(80.0, 1.0),
        &memory,
    );
    assert_eq!(steps(&outcome), Steps::new());
}

#[test]
fn a_climb_is_remembered_and_its_account_starts_its_burn_over() {
    let outcome = run(
        vec![session("a")],
        &session_account(80.0),
        &measured(80.0, 1.0),
    );
    assert_eq!(outcome.memory.stepped_at.get("a"), Some(&NOW));
    assert_eq!(outcome.stepped_accounts, vec![account_key()]);
}

#[test]
fn the_memory_forgets_sessions_that_are_gone() {
    let mut memory = EngineMemory::default();
    memory.stepped_at.insert("gone".to_string(), NOW);
    let outcome = run_with(
        vec![session("a")],
        &session_account(30.0),
        &SampleBook::default(),
        &memory,
    );
    assert!(outcome.memory.stepped_at.is_empty());
}

#[test]
fn running_subagents_of_a_session_over_pace_are_throttled() {
    let mut busy = session("a");
    busy.subagents = vec![SubagentReport {
        id: "sub".to_string(),
        description: Some("a search".to_string()),
        load: Load::new(2.0),
    }];
    let outcome = run(vec![busy], &session_account(80.0), &measured(80.0, 1.0));
    let throttle = outcome
        .decisions
        .iter()
        .find_map(|decision| match decision {
            Decision::Throttle { session, throttle } => Some((session, throttle)),
            _ => None,
        });
    let (session_id, throttle) = throttle.expect("the round throttles the subagent");
    assert_eq!(session_id.as_str(), "a");
    assert!(is_throttling(throttle));
    assert!(!outcome.memory.subagent_steps.is_empty());
}

// --- paused sessions ---

#[test]
fn a_pause_whose_window_has_come_back_continues() {
    let mut due = paused_session("a", PacePhase::Paused);
    due.pace.as_mut().expect("a pause").continue_at = NOW - minutes(1.0);
    let outcome = run(vec![due], &session_account(80.0), &SampleBook::default());
    assert_eq!(steps(&outcome), only("a", ObservedKind::Resume));
}

#[test]
fn an_overridden_pause_at_its_reset_is_released_not_continued() {
    let mut due = paused_session("a", PacePhase::Overridden);
    due.pace.as_mut().expect("a pause").continue_at = NOW - minutes(1.0);
    let outcome = run(vec![due], &session_account(80.0), &SampleBook::default());
    assert_eq!(steps(&outcome), only("a", ObservedKind::Release));
}

#[test]
fn a_rest_that_has_ended_continues() {
    let mut resting = paused_session("a", PacePhase::Paused);
    resting.pace.as_mut().expect("a pause").rest_until = Some(NOW - minutes(1.0));
    let outcome = run(
        vec![resting],
        &session_account(80.0),
        &SampleBook::default(),
    );
    assert_eq!(steps(&outcome), only("a", ObservedKind::Resume));
}

#[test]
fn a_rest_not_yet_begun_is_not_over() {
    let mut finishing = paused_session("a", PacePhase::Pausing);
    finishing.is_working = true;
    finishing.pace.as_mut().expect("a pause").rest_until = Some(NOW - minutes(1.0));
    let outcome = run(vec![finishing], &BTreeMap::new(), &SampleBook::default());
    assert_empty(&outcome.decisions);
}

#[test]
fn a_pause_still_over_pace_stays_held() {
    let held = paused_with_rate("a", PacePhase::Paused, 1.0);
    let outcome = run(vec![held], &session_account(80.0), &SampleBook::default());
    assert_empty(&outcome.decisions);
}

#[test]
fn a_pause_back_on_pace_continues() {
    let held = paused_with_rate("a", PacePhase::Paused, 1.0);
    let outcome = run(vec![held], &session_account(40.0), &SampleBook::default());
    assert_eq!(steps(&outcome), only("a", ObservedKind::Resume));
    assert_eq!(
        outcome.memory.last_release_at.get(&account_key()),
        Some(&NOW)
    );
}

#[test]
fn an_overridden_pause_back_on_pace_is_released() {
    let held = paused_with_rate("a", PacePhase::Overridden, 1.0);
    let outcome = run(vec![held], &session_account(40.0), &SampleBook::default());
    assert_eq!(steps(&outcome), only("a", ObservedKind::Release));
    assert!(outcome.memory.last_release_at.is_empty());
}

#[test]
fn a_session_still_stopping_is_not_let_go_even_back_on_pace() {
    let mut stopping = paused_with_rate("a", PacePhase::Pausing, 1.0);
    stopping.is_working = true;
    let outcome = run(
        vec![stopping],
        &session_account(40.0),
        &SampleBook::default(),
    );
    assert_empty(&outcome.decisions);
}

#[test]
fn an_unreadable_account_leaves_a_pause_be() {
    let held = paused_with_rate("a", PacePhase::Paused, 1.0);
    let outcome = run(vec![held], &BTreeMap::new(), &SampleBook::default());
    assert_empty(&outcome.decisions);
}

#[test]
fn one_paused_session_per_account_is_let_go_the_lightest_first() {
    let mut heavy = paused_with_rate("heavy", PacePhase::Paused, 1.0);
    heavy.own_load = Load::new(3.0);
    let mut light = paused_with_rate("light", PacePhase::Paused, 1.0);
    light.own_load = Load::new(1.0);
    let outcome = run(
        vec![heavy, light],
        &session_account(40.0),
        &SampleBook::default(),
    );
    assert_eq!(steps(&outcome), only("light", ObservedKind::Resume));
}

#[test]
fn a_pauses_subagents_count_toward_how_light_it_is() {
    let mut crowded = paused_with_rate("crowded", PacePhase::Paused, 1.0);
    crowded.subagents = vec![SubagentReport {
        id: "sub".to_string(),
        description: None,
        load: Load::new(5.0),
    }];
    let mut alone = paused_with_rate("alone", PacePhase::Paused, 1.0);
    alone.own_load = Load::new(2.0);
    let outcome = run(
        vec![crowded, alone],
        &session_account(40.0),
        &SampleBook::default(),
    );
    assert_eq!(steps(&outcome), only("alone", ObservedKind::Resume));
}

#[test]
fn the_next_release_waits_for_the_account_to_show_what_the_last_one_spends() {
    let mut memory = EngineMemory::default();
    memory
        .last_release_at
        .insert(account_key(), NOW - minutes(5.0));
    let held = paused_with_rate("a", PacePhase::Paused, 1.0);
    let outcome = run_with(
        vec![held],
        &session_account(40.0),
        &SampleBook::default(),
        &memory,
    );
    assert_empty(&outcome.decisions);

    memory
        .last_release_at
        .insert(account_key(), NOW - minutes(11.0));
    let held = paused_with_rate("a", PacePhase::Paused, 1.0);
    let outcome = run_with(
        vec![held],
        &session_account(40.0),
        &SampleBook::default(),
        &memory,
    );
    assert_eq!(steps(&outcome), only("a", ObservedKind::Resume));
}

// --- what the wiring samples ---

fn sampled(
    sessions: Vec<SessionFacts>,
    accounts: &BTreeMap<AgentId, AccountReading>,
) -> Vec<AccountKey> {
    let live_sessions: Vec<LiveSession> = sessions.into_iter().map(live).collect();
    accounts_to_sample(&live_sessions, accounts)
}

#[test]
fn an_account_is_sampled_once_however_many_sessions_work_on_it() {
    let found = sampled(vec![session("a"), session("b")], &session_account(50.0));
    assert_eq!(found, vec![account_key()]);
}

#[test]
fn only_work_is_sampled_with_pacing_on_and_a_reading() {
    let mut idle = session("idle");
    idle.is_working = false;
    let mut off = session("off");
    off.is_pacing_on = false;
    assert_empty(&sampled(vec![idle, off], &session_account(50.0)));
    assert_empty(&sampled(vec![session("a")], &unread()));
    assert_empty(&sampled(vec![session("a")], &BTreeMap::new()));
}
