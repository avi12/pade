use crate::pacing::test_support::{account, session_window, weekly_window, NOW};
use crate::pacing::usage_pace::*;

const BASE_UTILIZATION: f64 = 60.0;
const READING_COUNT: u32 = 12;
const RISE_PER_READING: f64 = 0.5;
const HOURLY_PERCENT: f64 = 30.0;
const SHORT_READING_COUNT: u32 = 10;
const SMALL_PERCENT: f64 = 5.0;
const LOW_PERCENT: f64 = 20.0;
const FOUR_HOURS: f64 = 4.0;
const REMAINING_PERCENT: f64 = 80.0;
const SESSION_PERCENT: f64 = 10.0;
const VERY_HIGH_PERCENT: f64 = 90.0;
const FULL_PERCENT: f64 = 100.0;
const DAYS_PER_WEEK: f64 = 7.0;
const HIGH_PERCENT: f64 = 70.0;
const NEARLY_ON_PACE_SHARE: f64 = 0.97;
const THREE_UNITS: f64 = 3.0;
const ALMOST_SPENT_PERCENT: f64 = 95.0;
const NEARLY_FULL_PERCENT: f64 = 99.0;
const CLOSING_MINUTES: f64 = 9.0;
const PROJECTION_HOURS: f64 = 4.0;
const FAST_HOURLY_PERCENT: f64 = 10.0;
const LONG_HOURS: f64 = 40.0;
const DAILY_HOURLY_PERCENT: f64 = 0.41;
const QUARTER_DUTY: f64 = 0.25;
const QUARTER_REST_MINUTES: f64 = 45.0;
const MOSTLY_ON_DUTY: f64 = 0.9;
const SHORT_REST_MINUTES: f64 = 5.0;
const TINY_DUTY: f64 = 0.02;
const LONGEST_REST_MINUTES: f64 = 120.0;
const NEGATIVE_DUTY_SIZE: f64 = 0.2;

const MINUTE: SpanMilliseconds = SpanMilliseconds::from_minutes(1.0);
const HOUR: SpanMilliseconds = SpanMilliseconds::from_hours(1.0);
const DAY: SpanMilliseconds = SpanMilliseconds::from_days(1.0);
/// How long the allowance of a 5 h session window is paid out over, in hours.
const SESSION_PAYOUT_HOURS: f64 = 8.0 / (7.0 * 24.0) * 5.0;
/// An hour into a 5 h session window, the hours left until it runs free.
const HOURS_UNTIL_FREE: f64 = 230.0 / 60.0;

/// A burn rate in the formula's units, from percent per hour.
fn per_hour(percentage: f64) -> BurnRate {
    Percentage::new(percentage) / HOUR
}

/// One reading a minute, rising `rise` percent each.
fn readings(minutes: u32, rise: f64, resets_at: &str) -> Vec<UsageSample> {
    (0..=minutes)
        .map(|i| UsageSample {
            time: NOW + MINUTE * f64::from(i),
            utilization: Percentage::new(BASE_UTILIZATION + f64::from(i) * rise),
            resets_at: Some(resets_at.to_string()),
        })
        .collect()
}

fn percent_per_hour(samples: &[UsageSample]) -> Option<f64> {
    burn_rate(samples).map(|rate| rate.get() * HOUR.get())
}

fn assert_near(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("a measured rate");
    assert_close(actual, expected);
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{actual} is not {expected}"
    );
}

#[test]
fn burn_rate_measures_how_fast_a_window_rises_while_its_agents_work() {
    assert_near(
        percent_per_hour(&readings(READING_COUNT, RISE_PER_READING, "reset")),
        HOURLY_PERCENT,
    );
}

#[test]
fn burn_rate_waits_for_ten_minutes_of_working_time_before_naming_a_rate() {
    assert_eq!(burn_rate(&readings(9, 0.5, "reset")), None);
}

/// One reading a minute of a window read in whole percents, burning
/// `percent_per_hour` - the flat steps a real window shows.
fn whole_percent_readings(minutes: u32, percent_per_hour: f64, starts_at: f64) -> Vec<UsageSample> {
    let minutes_per_hour = HOUR / MINUTE;
    (0..=minutes)
        .map(|i| UsageSample {
            time: NOW + MINUTE * f64::from(i),
            utilization: Percentage::new(
                (BASE_UTILIZATION + starts_at + percent_per_hour * f64::from(i) / minutes_per_hour)
                    .floor(),
            ),
            resets_at: Some("reset".to_string()),
        })
        .collect()
}

/// A paced burn: 0.7% an hour crosses a whole percent about every 86 minutes.
const PACED_BURN: f64 = 0.7;
const TICK_TO_TICK_TOLERANCE: f64 = 0.05;
const THREE_HOURS_OF_READINGS: u32 = 180;
const TWO_HOURS_OF_READINGS: u32 = 120;
const UNDER_AN_HOUR_OF_READINGS: u32 = 45;
const HALF_A_PERCENT: f64 = 0.5;

#[test]
fn burn_rate_reads_a_slow_burn_tick_to_tick() {
    let rate = percent_per_hour(&whole_percent_readings(
        THREE_HOURS_OF_READINGS,
        PACED_BURN,
        HALF_A_PERCENT,
    ))
    .expect("a rate");
    let is_near = (rate - PACED_BURN).abs() < TICK_TO_TICK_TOLERANCE;
    assert!(is_near, "got {rate}");
}

#[test]
fn burn_rate_names_no_rate_from_a_flat_stretch_too_short_to_show_a_tick() {
    let readings = whole_percent_readings(UNDER_AN_HOUR_OF_READINGS, PACED_BURN, 0.0);
    assert_eq!(burn_rate(&readings), None);
}

#[test]
fn burn_rate_calls_the_burn_nothing_only_after_two_hours_without_a_tick() {
    assert_eq!(
        percent_per_hour(&readings(TWO_HOURS_OF_READINGS, 0.0, "reset")),
        Some(0.0)
    );
}

#[test]
fn a_measured_zero_keeps_the_load_forecast() {
    const RATE_PER_LOAD: f64 = 0.01;
    const LOAD: f64 = 40.0;
    let projected = projected_rate(ProjectedRateParams {
        measured: Some(BurnRate::new(0.0)),
        rate_per_load: Some(per_hour(RATE_PER_LOAD)),
        load: Load::new(LOAD),
    })
    .expect("a projected rate");
    assert_close(projected.get(), per_hour(RATE_PER_LOAD * LOAD).get());
}

#[test]
fn burn_rate_counts_readings_whose_reset_differs_only_by_the_vendors_jitter_as_one_window() {
    let jittered: Vec<UsageSample> = readings(READING_COUNT, RISE_PER_READING, "reset")
        .into_iter()
        .enumerate()
        .map(|(i, sample)| UsageSample {
            resets_at: Some(format!(
                "2026-11-14T00:59:59.{:06}+00:00",
                500_000 + i * 7_919
            )),
            ..sample
        })
        .collect();
    assert_near(percent_per_hour(&jittered), HOURLY_PERCENT);
}

#[test]
fn burn_rate_still_tells_a_new_window_from_the_old_one_by_hours() {
    let old = readings(
        SHORT_READING_COUNT,
        RISE_PER_READING,
        "2026-11-14T00:59:59.533870+00:00",
    );
    let last_at = old.last().map_or(NOW, |sample| sample.time);
    let next = UsageSample {
        time: last_at + MINUTE,
        utilization: Percentage::new(1.0),
        resets_at: Some("2026-11-14T05:59:59.533870+00:00".to_string()),
    };
    let samples = [old, vec![next]].concat();
    assert_near(percent_per_hour(&samples), HOURLY_PERCENT);
}

#[test]
fn burn_rate_leaves_idle_gaps_and_a_new_window_out_of_the_measure() {
    let working = readings(SHORT_READING_COUNT, RISE_PER_READING, "reset");
    let after_idle_hour: Vec<UsageSample> = working
        .iter()
        .map(|sample| UsageSample {
            time: sample.time + HOUR,
            utilization: sample.utilization + Percentage::new(SMALL_PERCENT),
            ..sample.clone()
        })
        .collect();
    let last_working_at = after_idle_hour.last().map_or(NOW, |sample| sample.time);
    let next_window = UsageSample {
        time: last_working_at + MINUTE,
        utilization: Percentage::new(1.0),
        resets_at: Some("next".to_string()),
    };
    let samples = [working, after_idle_hour, vec![next_window]].concat();
    assert_near(percent_per_hour(&samples), HOURLY_PERCENT);
}

fn assess(
    windows: Vec<UsageWindow>,
    rate: Option<BurnRate>,
    is_paused: bool,
) -> Option<BindingWindow> {
    let account = account(windows);
    assess_pace(AssessPaceParams {
        account: Some(&account),
        now: NOW,
        is_paused,
        rate_of: &|_| rate,
    })
}

fn has_room(windows: Vec<UsageWindow>, rate: Option<BurnRate>) -> bool {
    let account = account(windows);
    has_room_to_spare(HasRoomParams {
        account: Some(&account),
        now: NOW,
        rate_of: &|_| rate,
    })
}

/// What a window lets its account spend now, in percent per hour.
fn may_spend_per_hour(window: &UsageWindow) -> f64 {
    let paced = paced_window(window, NOW).expect("a paced window");
    paced.may_spend_at(utilization_of(window), NOW).get() * HOUR.get()
}

#[test]
fn a_window_on_its_even_spend_may_spend_what_lasts_plus_the_whole_allowance_paid_out() {
    // An hour into a 5 h session at 20% used: 80% until it runs free, plus 10%
    // over the payout time.
    let window = session_window(LOW_PERCENT, HOUR * FOUR_HOURS);
    assert_close(
        may_spend_per_hour(&window),
        REMAINING_PERCENT / HOURS_UNTIL_FREE + SESSION_PERCENT / SESSION_PAYOUT_HOURS,
    );
}

#[test]
fn a_window_spent_past_its_allowance_may_spend_only_what_lasts() {
    // 90% used against an even 20%: the allowance is spent, never owed.
    let window = session_window(VERY_HIGH_PERCENT, HOUR * FOUR_HOURS);
    assert_close(
        may_spend_per_hour(&window),
        SESSION_PERCENT / HOURS_UNTIL_FREE,
    );
}

#[test]
fn idle_hours_refill_the_allowance_only_up_to_its_cap() {
    // Nothing used an hour in: the allowance stays 10%, not the 30% the even
    // spend runs ahead.
    let idle = session_window(0.0, HOUR * FOUR_HOURS);
    assert_close(
        may_spend_per_hour(&idle),
        FULL_PERCENT / HOURS_UNTIL_FREE + SESSION_PERCENT / SESSION_PAYOUT_HOURS,
    );
}

#[test]
fn slows_a_fast_burn_from_the_first_percent_there_is_no_floor() {
    let window = weekly_window(SMALL_PERCENT, DAY * DAYS_PER_WEEK - HOUR);
    let pace = assess(vec![window], Some(per_hour(SMALL_PERCENT)), false).expect("held");
    assert_eq!(pace.kind, PacedWindowKind::Weekly);
    assert!(pace.keep < SLOW_DOWN_KEEP);
    assert!(pace.keep > Fraction::NONE);
}

#[test]
fn lets_a_burst_that_fits_in_the_allowance_run_on_with_room_to_spare() {
    let window = session_window(LOW_PERCENT, HOUR * FOUR_HOURS);
    assert_eq!(
        assess(vec![window.clone()], Some(per_hour(30.0)), false),
        None
    );
    assert!(has_room(vec![window], Some(per_hour(30.0))));
}

#[test]
fn reports_the_keep_share_its_pace_leaves() {
    let window = session_window(HIGH_PERCENT, HOUR * FOUR_HOURS);
    let pace = assess(vec![window.clone()], Some(per_hour(HOURLY_PERCENT)), false).expect("held");
    assert_close(
        pace.keep.get(),
        may_spend_per_hour(&window) / HOURLY_PERCENT,
    );
    assert_eq!(pace.continue_at, NOW + HOUR * 4.0 - RESET_GRACE_MS);
}

#[test]
fn holds_a_paused_session_until_its_keep_share_is_back_to_one() {
    let window = session_window(LOW_PERCENT, HOUR * FOUR_HOURS);
    let nearly_on_pace = per_hour(may_spend_per_hour(&window) / NEARLY_ON_PACE_SHARE);
    assert_eq!(
        assess(vec![window.clone()], Some(nearly_on_pace), false),
        None
    );
    let pace = assess(vec![window], Some(nearly_on_pace), true).expect("still held");
    assert!(pace.keep >= SLOW_DOWN_KEEP && pace.keep < Fraction::WHOLE);
}

#[test]
fn a_paused_session_is_ready_once_the_window_lets_it_spend_at_its_pace() {
    let window = session_window(HIGH_PERCENT, HOUR * FOUR_HOURS);
    let rate = per_hour(HOURLY_PERCENT);
    let pace = assess(vec![window.clone()], Some(rate), true).expect("held");
    assert!(pace.ready_at > NOW && pace.ready_at <= pace.continue_at);
    let paced = paced_window(&window, NOW).expect("a paced window");
    let at_ready = paced.may_spend_at(utilization_of(&window), pace.ready_at);
    assert!((at_ready / rate - 1.0).abs() < 1e-6);
}

#[test]
fn reports_the_window_that_keeps_the_least() {
    let session = session_window(HIGH_PERCENT, HOUR * FOUR_HOURS);
    let weekly = weekly_window(HOURLY_PERCENT, DAY * THREE_UNITS);
    let rate = Some(per_hour(HOURLY_PERCENT));
    let session_alone = assess(vec![session.clone()], rate, false).expect("held");
    let weekly_alone = assess(vec![weekly.clone()], rate, false).expect("held");
    assert!(weekly_alone.keep < session_alone.keep);
    let pace = assess(vec![session, weekly], rate, false).expect("held");
    assert_eq!(pace.kind, PacedWindowKind::Weekly);
}

#[test]
fn never_paces_before_a_pace_is_measured() {
    let window = session_window(ALMOST_SPENT_PERCENT, HOUR * FOUR_HOURS);
    assert_eq!(assess(vec![window.clone()], None, false), None);
    assert!(!has_room(vec![window], None));
}

#[test]
fn leaves_a_window_alone_this_close_to_its_reset() {
    let window = session_window(NEARLY_FULL_PERCENT, MINUTE * CLOSING_MINUTES);
    assert_eq!(
        assess(vec![window.clone()], Some(per_hour(90.0)), false),
        None
    );
    assert!(has_room(vec![window], None));
}

#[test]
fn never_paces_when_the_account_cant_be_read() {
    assert_eq!(
        assess_pace(AssessPaceParams {
            account: None,
            now: NOW,
            is_paused: false,
            rate_of: &|_| Some(per_hour(60.0)),
        }),
        None
    );
    assert!(!has_room_to_spare(HasRoomParams {
        account: None,
        now: NOW,
        rate_of: &|_| Some(per_hour(1.0)),
    }));
}

fn project(measured: Option<BurnRate>, rate_per_load: Option<BurnRate>, load: f64) -> Option<f64> {
    projected_rate(ProjectedRateParams {
        measured,
        rate_per_load,
        load: Load::new(load),
    })
    .map(|rate| rate.get() * HOUR.get())
}

#[test]
fn projects_the_pace_onto_the_load_running_now_when_that_is_more() {
    assert_near(
        project(
            Some(per_hour(1.0)),
            Some(per_hour(RISE_PER_READING)),
            PROJECTION_HOURS,
        ),
        2.0,
    );
    assert_near(
        project(
            Some(per_hour(THREE_UNITS)),
            Some(per_hour(RISE_PER_READING)),
            PROJECTION_HOURS,
        ),
        THREE_UNITS,
    );
}

#[test]
fn lifts_the_measured_pace_at_most_threefold() {
    assert_near(
        project(
            Some(per_hour(1.0)),
            Some(per_hour(FAST_HOURLY_PERCENT)),
            LONG_HOURS,
        ),
        THREE_UNITS,
    );
}

#[test]
fn projects_from_either_side_alone_and_from_neither_names_no_pace() {
    assert_near(project(None, Some(per_hour(RISE_PER_READING)), 2.0), 1.0);
    assert_near(project(Some(per_hour(THREE_UNITS)), None, 2.0), THREE_UNITS);
    assert_eq!(project(None, None, 2.0), None);
}

fn session_rate_only(session: Option<BurnRate>) -> impl Fn(&UsageWindow) -> Option<BurnRate> {
    move |window| match window.kind {
        UsageWindowKind::Session => session,
        UsageWindowKind::Weekly
        | UsageWindowKind::Model
        | UsageWindowKind::Credits
        | UsageWindowKind::Opaque => None,
    }
}

#[test]
fn forecasts_each_paced_window_by_its_own_measure() {
    let account = account(vec![
        session_window(70.0, HOUR * 3.0),
        weekly_window(30.0, DAY * 3.0),
    ]);
    let forecasts = pace_forecasts(PaceForecastsParams {
        account: Some(&account),
        now: NOW,
        rate_of: &session_rate_only(Some(per_hour(HOURLY_PERCENT))),
        daily_rate_of: &|_| None,
    });
    let session = forecasts.session.expect("a session forecast");
    let weekly = forecasts.weekly.expect("a weekly forecast");
    assert_eq!(session.run_out_at, Some(NOW + HOUR));
    assert!(session.is_measured);
    assert!(session.keep.is_some());
    // 30% in four days lasts another 9.3 days at that pace - past the reset.
    assert!(weekly.run_out_at.expect("a run-out") > NOW + DAY * 3.0);
    assert!(!weekly.is_measured);
    assert_eq!(weekly.keep, None);
}

#[test]
fn has_no_run_out_time_before_a_sessions_burn_is_measured() {
    let account = account(vec![session_window(70.0, HOUR * 3.0)]);
    let forecasts = pace_forecasts(PaceForecastsParams {
        account: Some(&account),
        now: NOW,
        rate_of: &|_| None,
        daily_rate_of: &|_| None,
    });
    assert_eq!(
        forecasts.session.map(|forecast| forecast.run_out_at),
        Some(None)
    );
}

#[test]
fn forecasts_a_weekly_window_at_its_last_days_spend_once_it_has_one() {
    // 75% after four and a half days averages 0.69% an hour - out in 36 hours -
    // but the last day spent 0.41% an hour, which lasts the 61 hours left.
    let account = account(vec![weekly_window(75.0, HOUR * 61.0)]);
    let forecasts = pace_forecasts(PaceForecastsParams {
        account: Some(&account),
        now: NOW,
        rate_of: &|_| None,
        daily_rate_of: &|_| Some(per_hour(DAILY_HOURLY_PERCENT)),
    });
    let weekly = forecasts.weekly.expect("a weekly forecast");
    assert!(weekly.run_out_at.expect("a run-out") > NOW + HOUR * 60.0);
}

#[test]
fn a_forecast_is_over_pace_by_the_rule_the_pacing_loop_acts_on() {
    let windows = vec![session_window(70.0, HOUR * 4.0)];
    let account = account(windows.clone());
    let forecasts = pace_forecasts(PaceForecastsParams {
        account: Some(&account),
        now: NOW,
        rate_of: &session_rate_only(Some(per_hour(HOURLY_PERCENT))),
        daily_rate_of: &|_| None,
    });
    let forecast = forecasts.session.expect("a session forecast");
    assert!(is_forecast_over_pace(&forecast));
    assert!(assess(windows, Some(per_hour(30.0)), false).is_some());
    let unmeasured = PaceForecast {
        keep: None,
        ..forecast
    };
    assert!(!is_forecast_over_pace(&unmeasured));
}

#[test]
fn reported_kinds_leave_out_a_session_limit_the_plan_doesnt_report() {
    let account = account(vec![weekly_window(30.0, DAY * 3.0)]);
    assert_eq!(
        reported_paced_kinds(Some(&account)),
        [PacedWindowKind::Weekly]
    );
}

#[test]
fn reported_kinds_keep_both_in_their_order_when_both_are_reported() {
    let account = account(vec![
        weekly_window(30.0, DAY * 3.0),
        session_window(70.0, HOUR * 3.0),
    ]);
    assert_eq!(reported_paced_kinds(Some(&account)), PacedWindowKind::ALL);
}

#[test]
fn reported_kinds_offer_every_paced_window_while_there_is_no_reading() {
    assert_eq!(reported_paced_kinds(None), PacedWindowKind::ALL);
}

#[test]
fn works_the_shortest_stretch_and_rests_what_keeps_the_keep_share() {
    let half = work_rhythm(Fraction::new(RISE_PER_READING));
    assert_eq!(half.stretch, MINUTE * 15.0);
    assert_eq!(half.rest, MINUTE * 15.0);
    let quarter = work_rhythm(Fraction::new(QUARTER_DUTY));
    assert_eq!(quarter.stretch, MINUTE * 15.0);
    assert_close(quarter.rest / MINUTE, QUARTER_REST_MINUTES);
}

#[test]
fn works_longer_not_rests_less_when_the_keep_share_is_high() {
    let rhythm = work_rhythm(Fraction::new(MOSTLY_ON_DUTY));
    assert_close(rhythm.rest / MINUTE, SHORT_REST_MINUTES);
    assert_close(rhythm.stretch / MINUTE, QUARTER_REST_MINUTES);
}

#[test]
fn rests_the_minimum_on_pace_and_at_most_two_hours_with_nothing_to_keep() {
    assert_eq!(work_rhythm(Fraction::WHOLE).rest, MINUTE * 5.0);
    assert_eq!(work_rhythm(Fraction::new(1.4)).rest, MINUTE * 5.0);
    assert_close(
        work_rhythm(Fraction::new(TINY_DUTY)).rest / MINUTE,
        LONGEST_REST_MINUTES,
    );
    assert_close(
        work_rhythm(Fraction::NONE).rest / MINUTE,
        LONGEST_REST_MINUTES,
    );
    assert_close(
        work_rhythm(Fraction::new(-NEGATIVE_DUTY_SIZE)).rest / MINUTE,
        LONGEST_REST_MINUTES,
    );
}

#[test]
fn stops_only_with_next_to_nothing_left_in_the_window() {
    let window_at =
        |utilization: f64| weekly_window(utilization, SpanMilliseconds::from_days(THREE_UNITS));
    assert!(!is_nearly_spent(&window_at(66.0)));
    assert!(!is_nearly_spent(&window_at(97.0)));
    assert!(is_nearly_spent(&window_at(98.0)));
}

fn rest_until(keep: f64, ready_at: EpochMilliseconds) -> EpochMilliseconds {
    slow_rest_until(SlowRestParams {
        keep: Fraction::new(keep),
        ready_at,
        now: NOW,
    })
}

#[test]
fn ends_the_rest_where_the_rhythm_says_never_past_ready() {
    assert_eq!(rest_until(0.5, NOW + HOUR * 12.0), NOW + MINUTE * 15.0);
    assert_eq!(rest_until(0.5, NOW + MINUTE * 8.0), NOW + MINUTE * 8.0);
    assert_eq!(rest_until(0.0, NOW + HOUR * 12.0), NOW + HOUR * 2.0);
}
