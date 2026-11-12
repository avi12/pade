//! Holds the Rust port to the TypeScript's own answers. The fixtures in
//! `fixtures/` are written by `scripts/pacing-fixtures.ts`, which sweeps a grid
//! (and a seeded scatter) of inputs through the TypeScript functions and records
//! what they return - `null` for a `None`, a missing key for an absent forecast.
//! Each test here feeds the same inputs to the Rust function and compares the
//! JSON of its answer with `test_support::assert_json_close`, which allows a
//! relative `1e-9` on every float.

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::agents::running_model::ModelChoice;
use crate::pacing::account_round::{
    plan_round, PlanRoundParams, RoundOutcome, SubagentSteps, WindowReport,
};
use crate::pacing::load_shedding::{
    cost_weight, plan_shedding, shed_fraction, CostWeightParams, Load, PlanSheddingParams,
    ShedCandidate, ShedFractionParams,
};
use crate::pacing::policy::pacing_margin;
use crate::pacing::subagent_steps::{
    eased_subagent_step, hold_for_saving, next_subagent_step, NextStepParams, SubagentStep,
};
use crate::pacing::test_support::{account, assert_json_close, session_window, NOW};
use crate::pacing::units::{BurnRate, EpochMilliseconds, Fraction, SpanMilliseconds};
use crate::pacing::usage_pace::{
    assess_pace, burn_rate, even_spend_reached_at, has_room_to_spare, is_forecast_over_pace,
    is_rest_enough, pace_forecasts, slow_rest_until, work_rhythm, working_runway, AssessPaceParams,
    BindingWindow, ForecastOverPaceParams, HasRoomParams, PaceForecast, PaceForecastsParams,
    PacedWindowKind, RestEnoughParams, SlowRestParams, UsageSample, WorkRhythmParams,
    WorkingRunwayParams,
};
use crate::usage::UsageWindow;

const USAGE_PACE: &str = include_str!("fixtures/usage_pace.json");
const LOAD_SHEDDING: &str = include_str!("fixtures/load_shedding.json");
const SUBAGENT_STEPS: &str = include_str!("fixtures/subagent_steps.json");
const ACCOUNT_ROUND: &str = include_str!("fixtures/account_round.json");

fn fixture<Fixture: DeserializeOwned>(text: &str) -> Fixture {
    serde_json::from_str(text).expect("a fixture the generator wrote")
}

fn to_json<Value: serde::Serialize>(value: &Value) -> serde_json::Value {
    serde_json::to_value(value).expect("serializes")
}

/// A number the fixtures hold per paced kind.
#[derive(Deserialize)]
struct PerKind<Number> {
    session: Option<Number>,
    weekly: Option<Number>,
}

impl<Number: Copy> PerKind<Number> {
    fn of(&self, kind: PacedWindowKind) -> Option<Number> {
        match kind {
            PacedWindowKind::Session => self.session,
            PacedWindowKind::Weekly => self.weekly,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaceInputs {
    windows: Vec<UsageWindow>,
    rates: PerKind<f64>,
    margins: PerKind<f64>,
    is_paused: bool,
}

impl PaceInputs {
    fn rate_of(&self, window: &UsageWindow) -> Option<BurnRate> {
        PacedWindowKind::from_window_kind(window.kind)
            .and_then(|kind| self.rates.of(kind))
            .map(BurnRate::new)
    }

    fn margin_of(&self, kind: PacedWindowKind) -> Option<SpanMilliseconds> {
        self.margins.of(kind).map(SpanMilliseconds::new)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaceCase {
    inputs: PaceInputs,
    assess_pace: Value,
    has_room_to_spare: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForecastCase {
    index: usize,
    forecasts: Value,
}

#[derive(Deserialize)]
struct ForecastOverPaceCase {
    forecast: PaceForecast,
    margin: f64,
    expected: bool,
}

#[derive(Deserialize)]
struct BurnRateCase {
    samples: Vec<UsageSample>,
    expected: Option<f64>,
}

#[derive(Deserialize)]
struct EvenSpendCase {
    window: UsageWindow,
    expected: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkingRunwayCase {
    window: UsageWindow,
    rate: f64,
    ready_at: f64,
    continue_at: f64,
    expected: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkRhythmCase {
    runway: f64,
    time_left: f64,
    expected: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SlowRestCase {
    runway: f64,
    ready_at: f64,
    continue_at: f64,
    now: f64,
    expected_rest_until: f64,
    expected_is_rest_enough: bool,
}

#[derive(Deserialize)]
struct MarginCase {
    kind: PacedWindowKind,
    expected: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsagePaceFixture {
    now: f64,
    pace_cases: Vec<PaceCase>,
    pace_forecasts: Vec<ForecastCase>,
    is_forecast_over_pace: Vec<ForecastOverPaceCase>,
    burn_rate: Vec<BurnRateCase>,
    even_spend_reached_at: Vec<EvenSpendCase>,
    working_runway: Vec<WorkingRunwayCase>,
    work_rhythm: Vec<WorkRhythmCase>,
    slow_rest: Vec<SlowRestCase>,
    pacing_margin: Vec<MarginCase>,
}

fn usage_pace_fixture() -> UsagePaceFixture {
    let fixture: UsagePaceFixture = fixture(USAGE_PACE);
    assert!(
        (fixture.now - NOW.get()).abs() < f64::EPSILON,
        "the fixture clock is the tests' clock"
    );
    fixture
}

fn at(milliseconds: f64) -> EpochMilliseconds {
    EpochMilliseconds::new(milliseconds)
}

fn span(milliseconds: f64) -> SpanMilliseconds {
    SpanMilliseconds::new(milliseconds)
}

/// What the TypeScript reports of the binding window: everything but the window
/// it echoes.
fn binding_json(binding: Option<&BindingWindow>) -> Value {
    let Some(binding) = binding else {
        return Value::Null;
    };

    json!({
        "kind": binding.kind,
        "rate": binding.rate,
        "readyAt": binding.ready_at,
        "continueAt": binding.continue_at,
    })
}

#[test]
fn assess_pace_and_room_to_spare_agree_with_the_typescript_over_the_grid_and_the_scatter() {
    for (i, case) in usage_pace_fixture().pace_cases.iter().enumerate() {
        let inputs = &case.inputs;
        let usage = account(inputs.windows.clone());
        let margin_of = |kind| inputs.margin_of(kind);
        let rate_of = |window: &UsageWindow| inputs.rate_of(window);
        let binding = assess_pace(AssessPaceParams {
            account: Some(&usage),
            now: NOW,
            is_paused: inputs.is_paused,
            margin_of: &margin_of,
            rate_of: &rate_of,
        });
        assert_json_close(
            &binding_json(binding.as_ref()),
            &case.assess_pace,
            &format!("assessPace case {i}"),
        );

        let has_room = has_room_to_spare(HasRoomParams {
            account: Some(&usage),
            now: NOW,
            margin_of: &margin_of,
            rate_of: &rate_of,
        });
        assert_eq!(has_room, case.has_room_to_spare, "hasRoomToSpare case {i}");
    }
}

#[test]
fn pace_forecasts_agree_with_the_typescript() {
    let fixture = usage_pace_fixture();
    for case in &fixture.pace_forecasts {
        let inputs = &fixture.pace_cases[case.index].inputs;
        let usage = account(inputs.windows.clone());
        let forecasts = pace_forecasts(PaceForecastsParams {
            account: Some(&usage),
            now: NOW,
            rate_of: &|window| inputs.rate_of(window),
        });
        assert_json_close(
            &to_json(&forecasts),
            &case.forecasts,
            &format!("paceForecasts case {}", case.index),
        );
    }
}

#[test]
fn forecast_over_pace_agrees_with_the_typescript() {
    for (i, case) in usage_pace_fixture()
        .is_forecast_over_pace
        .iter()
        .enumerate()
    {
        let is_over = is_forecast_over_pace(ForecastOverPaceParams {
            forecast: &case.forecast,
            margin: span(case.margin),
            now: NOW,
        });
        assert_eq!(is_over, case.expected, "isForecastOverPace case {i}");
    }
}

#[test]
fn burn_rate_agrees_with_the_typescript() {
    for (i, case) in usage_pace_fixture().burn_rate.iter().enumerate() {
        let rate = burn_rate(&case.samples).map(BurnRate::get);
        assert_json_close(
            &json!(rate),
            &json!(case.expected),
            &format!("burnRate case {i}"),
        );
    }
}

#[test]
fn even_spend_agrees_with_the_typescript() {
    for (i, case) in usage_pace_fixture()
        .even_spend_reached_at
        .iter()
        .enumerate()
    {
        let reached = even_spend_reached_at(&case.window).map(EpochMilliseconds::get);
        assert_json_close(
            &json!(reached),
            &json!(case.expected),
            &format!("evenSpendReachedAt case {i}"),
        );
    }
}

#[test]
fn working_runway_agrees_with_the_typescript() {
    for (i, case) in usage_pace_fixture().working_runway.iter().enumerate() {
        let runway = working_runway(WorkingRunwayParams {
            window: &case.window,
            rate: BurnRate::new(case.rate),
            ready_at: at(case.ready_at),
            continue_at: at(case.continue_at),
        });
        assert_json_close(
            &json!(runway.get()),
            &json!(case.expected),
            &format!("workingRunway case {i}"),
        );
    }
}

#[test]
fn work_rhythm_agrees_with_the_typescript() {
    for (i, case) in usage_pace_fixture().work_rhythm.iter().enumerate() {
        let rhythm = work_rhythm(WorkRhythmParams {
            runway: span(case.runway),
            time_left: span(case.time_left),
        });
        assert_json_close(
            &to_json(&rhythm),
            &case.expected,
            &format!("workRhythm case {i}"),
        );
    }
}

#[test]
fn slow_rest_and_rest_enough_agree_with_the_typescript() {
    for (i, case) in usage_pace_fixture().slow_rest.iter().enumerate() {
        let rest_until = slow_rest_until(SlowRestParams {
            runway: span(case.runway),
            ready_at: at(case.ready_at),
            continue_at: at(case.continue_at),
            now: at(case.now),
        });
        assert_json_close(
            &json!(rest_until.get()),
            &json!(case.expected_rest_until),
            &format!("slowRestUntil case {i}"),
        );
        let is_enough = is_rest_enough(RestEnoughParams {
            runway: span(case.runway),
            continue_at: at(case.continue_at),
            now: at(case.now),
        });
        assert_eq!(
            is_enough, case.expected_is_rest_enough,
            "isRestEnough case {i}"
        );
    }
}

#[test]
fn pacing_margins_agree_with_the_typescript() {
    for case in &usage_pace_fixture().pacing_margin {
        assert_json_close(
            &json!(pacing_margin(case.kind).get()),
            &json!(case.expected),
            &format!("pacingMargin {:?}", case.kind),
        );
    }
}

#[derive(Deserialize)]
struct CostWeightCase {
    model: String,
    effort: Option<String>,
    ladder: Vec<String>,
    expected: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShedFractionCase {
    utilization: f64,
    time_left: f64,
    rate: Option<f64>,
    expected: Option<f64>,
}

#[derive(Deserialize)]
struct PlanCandidate {
    id: String,
    load: f64,
    saving: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanSheddingCase {
    candidates: Vec<PlanCandidate>,
    total_load: f64,
    fraction: Option<f64>,
    expected: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadSheddingFixture {
    cost_weight: Vec<CostWeightCase>,
    shed_fraction: Vec<ShedFractionCase>,
    plan_shedding: Vec<PlanSheddingCase>,
}

#[test]
fn load_shedding_agrees_with_the_typescript() {
    let fixture: LoadSheddingFixture = fixture(LOAD_SHEDDING);
    for (i, case) in fixture.cost_weight.iter().enumerate() {
        let weight = cost_weight(CostWeightParams {
            choice: &ModelChoice {
                model: Some(case.model.clone()),
                effort: case.effort.clone(),
            },
            ladder: &case.ladder,
        });
        assert_json_close(
            &json!(weight.get()),
            &json!(case.expected),
            &format!("costWeight case {i}"),
        );
    }

    for (i, case) in fixture.shed_fraction.iter().enumerate() {
        let window = session_window(case.utilization, span(case.time_left));
        let shed = shed_fraction(ShedFractionParams {
            window: &window,
            rate: case.rate.map(BurnRate::new),
            free_at: NOW + span(case.time_left),
            now: NOW,
        });
        assert_json_close(
            &json!(shed.map(Fraction::get)),
            &json!(case.expected),
            &format!("shedFraction case {i}"),
        );
    }

    for (i, case) in fixture.plan_shedding.iter().enumerate() {
        let candidates: Vec<ShedCandidate<String>> = case
            .candidates
            .iter()
            .map(|candidate| ShedCandidate {
                id: candidate.id.clone(),
                load: Load::new(candidate.load),
                saving: Fraction::new(candidate.saving),
            })
            .collect();
        let chosen = plan_shedding(PlanSheddingParams {
            candidates: &candidates,
            total_load: Load::new(case.total_load),
            fraction: case.fraction.map(Fraction::new),
        });
        assert_eq!(chosen, case.expected, "planShedding case {i}");
    }
}

#[derive(Deserialize)]
struct HoldCase {
    fraction: f64,
    expected: u32,
}

#[derive(Deserialize)]
struct NextStepCase {
    current: Option<SubagentStep>,
    shed: Option<f64>,
    expected: Value,
}

#[derive(Deserialize)]
struct EasedStepCase {
    current: SubagentStep,
    expected: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubagentStepsFixture {
    hold_for_saving: Vec<HoldCase>,
    next_subagent_step: Vec<NextStepCase>,
    eased_subagent_step: Vec<EasedStepCase>,
}

#[test]
fn subagent_steps_agree_with_the_typescript() {
    let fixture: SubagentStepsFixture = fixture(SUBAGENT_STEPS);
    for case in &fixture.hold_for_saving {
        assert_eq!(
            hold_for_saving(Fraction::new(case.fraction)).get(),
            case.expected,
            "holdForSaving({})",
            case.fraction
        );
    }

    for (i, case) in fixture.next_subagent_step.iter().enumerate() {
        let next = next_subagent_step(NextStepParams {
            current: case.current,
            shed: case.shed.map(Fraction::new),
        });
        assert_json_close(
            &to_json(&next),
            &case.expected,
            &format!("nextSubagentStep case {i}"),
        );
    }

    for (i, case) in fixture.eased_subagent_step.iter().enumerate() {
        assert_json_close(
            &to_json(&eased_subagent_step(case.current)),
            &case.expected,
            &format!("easedSubagentStep case {i}"),
        );
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoundScenario {
    reports: Vec<WindowReport>,
    steps: SubagentSteps,
    stepped_at: HashMap<String, f64>,
    leader: String,
    now: f64,
    expected: Value,
}

#[derive(Deserialize)]
struct AccountRoundFixture {
    scenarios: Vec<RoundScenario>,
}

#[test]
fn plan_round_agrees_with_the_typescript_over_the_scenarios() {
    let fixture: AccountRoundFixture = fixture(ACCOUNT_ROUND);
    for (i, scenario) in fixture.scenarios.iter().enumerate() {
        let reports: Vec<&WindowReport> = scenario.reports.iter().collect();
        let stepped_at: HashMap<String, EpochMilliseconds> = scenario
            .stepped_at
            .iter()
            .map(|(session, time)| (session.clone(), at(*time)))
            .collect();
        let outcome: RoundOutcome = plan_round(PlanRoundParams {
            reports: &reports,
            steps: &scenario.steps,
            stepped_at: &stepped_at,
            leader: &scenario.leader,
            now: at(scenario.now),
        });
        assert_json_close(
            &to_json(&outcome),
            &scenario.expected,
            &format!("planRound scenario {i}"),
        );
    }
}
