use super::*;
use crate::pacing::test_support::{account, session_window, weekly_window, NOW};

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
            utilization: Percentage::new(60.0 + f64::from(i) * rise),
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
    assert_near(percent_per_hour(&readings(12, 0.5, "reset")), 30.0);
}

#[test]
fn burn_rate_waits_for_ten_minutes_of_working_time_before_naming_a_rate() {
    assert_eq!(burn_rate(&readings(9, 0.5, "reset")), None);
}

#[test]
fn burn_rate_counts_readings_whose_reset_differs_only_by_the_vendors_jitter_as_one_window() {
    let jittered: Vec<UsageSample> = readings(12, 0.5, "reset")
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
    assert_near(percent_per_hour(&jittered), 30.0);
}

#[test]
fn burn_rate_still_tells_a_new_window_from_the_old_one_by_hours() {
    let old = readings(10, 0.5, "2026-11-14T00:59:59.533870+00:00");
    let last_at = old.last().map_or(NOW, |sample| sample.time);
    let next = UsageSample {
        time: last_at + MINUTE,
        utilization: Percentage::new(1.0),
        resets_at: Some("2026-11-14T05:59:59.533870+00:00".to_string()),
    };
    let samples = [old, vec![next]].concat();
    assert_near(percent_per_hour(&samples), 30.0);
}

#[test]
fn burn_rate_leaves_idle_gaps_and_a_new_window_out_of_the_measure() {
    let working = readings(10, 0.5, "reset");
    let after_idle_hour: Vec<UsageSample> = working
        .iter()
        .map(|sample| UsageSample {
            time: sample.time + HOUR,
            utilization: sample.utilization + Percentage::new(5.0),
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
    assert_near(percent_per_hour(&samples), 30.0);
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
    let window = session_window(20.0, HOUR * 4.0);
    assert_close(
        may_spend_per_hour(&window),
        80.0 / HOURS_UNTIL_FREE + 10.0 / SESSION_PAYOUT_HOURS,
    );
}

#[test]
fn a_window_spent_past_its_allowance_may_spend_only_what_lasts() {
    // 90% used against an even 20%: the allowance is spent, never owed.
    let window = session_window(90.0, HOUR * 4.0);
    assert_close(may_spend_per_hour(&window), 10.0 / HOURS_UNTIL_FREE);
}

#[test]
fn idle_hours_refill_the_allowance_only_up_to_its_cap() {
    // Nothing used an hour in: the allowance stays 10%, not the 30% the even
    // spend runs ahead.
    let idle = session_window(0.0, HOUR * 4.0);
    assert_close(
        may_spend_per_hour(&idle),
        100.0 / HOURS_UNTIL_FREE + 10.0 / SESSION_PAYOUT_HOURS,
    );
}

#[test]
fn slows_a_fast_burn_from_the_first_percent_there_is_no_floor() {
    let window = weekly_window(5.0, DAY * 7.0 - HOUR);
    let pace = assess(vec![window], Some(per_hour(5.0)), false).expect("held");
    assert_eq!(pace.kind, PacedWindowKind::Weekly);
    assert!(pace.keep < SLOW_DOWN_KEEP);
    assert!(pace.keep > Fraction::NONE);
}

#[test]
fn lets_a_burst_that_fits_in_the_allowance_run_on_with_room_to_spare() {
    let window = session_window(20.0, HOUR * 4.0);
    assert_eq!(
        assess(vec![window.clone()], Some(per_hour(30.0)), false),
        None
    );
    assert!(has_room(vec![window], Some(per_hour(30.0))));
}

#[test]
fn reports_the_keep_share_its_pace_leaves() {
    let window = session_window(70.0, HOUR * 4.0);
    let pace = assess(vec![window.clone()], Some(per_hour(30.0)), false).expect("held");
    assert_close(pace.keep.get(), may_spend_per_hour(&window) / 30.0);
    assert_eq!(pace.continue_at, NOW + HOUR * 4.0 - RESET_GRACE_MS);
}

#[test]
fn holds_a_paused_session_until_its_keep_share_is_back_to_one() {
    let window = session_window(20.0, HOUR * 4.0);
    let nearly_on_pace = per_hour(may_spend_per_hour(&window) / 0.97);
    assert_eq!(
        assess(vec![window.clone()], Some(nearly_on_pace), false),
        None
    );
    let pace = assess(vec![window], Some(nearly_on_pace), true).expect("still held");
    assert!(pace.keep >= SLOW_DOWN_KEEP && pace.keep < Fraction::WHOLE);
}

#[test]
fn a_paused_session_is_ready_once_the_window_lets_it_spend_at_its_pace() {
    let window = session_window(70.0, HOUR * 4.0);
    let rate = per_hour(30.0);
    let pace = assess(vec![window.clone()], Some(rate), true).expect("held");
    assert!(pace.ready_at > NOW && pace.ready_at <= pace.continue_at);
    let paced = paced_window(&window, NOW).expect("a paced window");
    let at_ready = paced.may_spend_at(utilization_of(&window), pace.ready_at);
    assert!((at_ready / rate - 1.0).abs() < 1e-6);
}

#[test]
fn reports_the_window_that_keeps_the_least() {
    let session = session_window(70.0, HOUR * 4.0);
    let weekly = weekly_window(30.0, DAY * 3.0);
    let rate = Some(per_hour(30.0));
    let session_alone = assess(vec![session.clone()], rate, false).expect("held");
    let weekly_alone = assess(vec![weekly.clone()], rate, false).expect("held");
    assert!(weekly_alone.keep < session_alone.keep);
    let pace = assess(vec![session, weekly], rate, false).expect("held");
    assert_eq!(pace.kind, PacedWindowKind::Weekly);
}

#[test]
fn never_paces_before_a_pace_is_measured() {
    let window = session_window(95.0, HOUR * 4.0);
    assert_eq!(assess(vec![window.clone()], None, false), None);
    assert!(!has_room(vec![window], None));
}

#[test]
fn leaves_a_window_alone_this_close_to_its_reset() {
    let window = session_window(99.0, MINUTE * 9.0);
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
    assert_near(project(Some(per_hour(1.0)), Some(per_hour(0.5)), 4.0), 2.0);
    assert_near(project(Some(per_hour(3.0)), Some(per_hour(0.5)), 4.0), 3.0);
}

#[test]
fn projects_from_either_side_alone_and_from_neither_names_no_pace() {
    assert_near(project(None, Some(per_hour(0.5)), 2.0), 1.0);
    assert_near(project(Some(per_hour(3.0)), None, 2.0), 3.0);
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
        rate_of: &session_rate_only(Some(per_hour(30.0))),
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
    });
    assert_eq!(
        forecasts.session.map(|forecast| forecast.run_out_at),
        Some(None)
    );
}

#[test]
fn a_forecast_is_over_pace_by_the_rule_the_pacing_loop_acts_on() {
    let windows = vec![session_window(70.0, HOUR * 4.0)];
    let account = account(windows.clone());
    let forecasts = pace_forecasts(PaceForecastsParams {
        account: Some(&account),
        now: NOW,
        rate_of: &session_rate_only(Some(per_hour(30.0))),
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
    let half = work_rhythm(Fraction::new(0.5));
    assert_eq!(half.stretch, MINUTE * 15.0);
    assert_eq!(half.rest, MINUTE * 15.0);
    let tenth = work_rhythm(Fraction::new(0.1));
    assert_eq!(tenth.stretch, MINUTE * 15.0);
    assert_close(tenth.rest / MINUTE, 135.0);
}

#[test]
fn works_longer_not_rests_less_when_the_keep_share_is_high() {
    let rhythm = work_rhythm(Fraction::new(0.9));
    assert_close(rhythm.rest / MINUTE, 5.0);
    assert_close(rhythm.stretch / MINUTE, 45.0);
}

#[test]
fn rests_the_minimum_on_pace_and_without_end_with_nothing_to_keep() {
    assert_eq!(work_rhythm(Fraction::WHOLE).rest, MINUTE * 5.0);
    assert_eq!(work_rhythm(Fraction::new(1.4)).rest, MINUTE * 5.0);
    assert!(work_rhythm(Fraction::NONE).rest.get().is_infinite());
    assert!(work_rhythm(Fraction::new(-0.2)).rest.get().is_infinite());
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
    assert_eq!(rest_until(0.0, NOW + HOUR * 12.0), NOW + HOUR * 12.0);
}
