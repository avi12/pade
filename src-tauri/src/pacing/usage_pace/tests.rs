use super::*;
use crate::pacing::test_support::{account, session_window, weekly_window, NOW};

const MINUTE: SpanMilliseconds = SpanMilliseconds::from_minutes(1.0);
const HOUR: SpanMilliseconds = SpanMilliseconds::from_hours(1.0);
const DAY: SpanMilliseconds = SpanMilliseconds::from_days(1.0);
/// The margin pacing acts on unless a test says otherwise - the default's.
const EARLY: SpanMilliseconds = SpanMilliseconds::from_minutes(30.0);

/// A burn rate in the formula's units, from percent per hour.
fn per_hour(percentage: f64) -> BurnRate {
    Percentage::new(percentage) / HOUR
}

/// One reading a minute, rising `rise` percent each.
fn readings(minutes: u32, rise: f64, resets_at: &str) -> Vec<UsageSample> {
    (0..=minutes)
        .map(|i| UsageSample {
            at: NOW + MINUTE * f64::from(i),
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
    let last_at = old.last().map_or(NOW, |sample| sample.at);
    let next = UsageSample {
        at: last_at + MINUTE,
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
            at: sample.at + HOUR,
            utilization: sample.utilization + Percentage::new(5.0),
            ..sample.clone()
        })
        .collect();
    let last_working_at = after_idle_hour.last().map_or(NOW, |sample| sample.at);
    let next_window = UsageSample {
        at: last_working_at + MINUTE,
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
    early: SpanMilliseconds,
) -> Option<BindingWindow> {
    let account = account(windows);
    assess_pace(AssessPaceParams {
        account: Some(&account),
        now: NOW,
        is_paused,
        margin_of: &|_| Some(early),
        rate_of: &|_| rate,
    })
}

fn assess_default(windows: Vec<UsageWindow>, rate: Option<BurnRate>) -> Option<BindingWindow> {
    assess(windows, rate, false, EARLY)
}

#[test]
fn pauses_a_session_that_would_run_out_hours_before_its_reset() {
    let pace =
        assess_default(vec![session_window(70.0, HOUR * 4.0)], Some(per_hour(30.0))).expect("held");
    // 30% left at 30% an hour lasts one hour; the window runs free in 3h 50m.
    assert_eq!(pace.window.kind, UsageWindowKind::Session);
    assert_eq!(pace.ready_at, NOW + HOUR * 4.0 - RESET_GRACE_MS - HOUR);
    assert_eq!(pace.continue_at, NOW + HOUR * 4.0 - RESET_GRACE_MS);
}

#[test]
fn lets_a_session_run_on_while_it_would_run_out_no_earlier_than_the_chosen_margin() {
    // 30% left at 30% an hour lasts one hour; the window runs free in 1h 20m -
    // 20 minutes early, inside a 30-minute margin but past a 15-minute one.
    let windows = vec![session_window(70.0, MINUTE * 90.0)];
    assert_eq!(assess_default(windows.clone(), Some(per_hour(30.0))), None);
    assert!(assess(windows, Some(per_hour(30.0)), false, MINUTE * 15.0).is_some());
}

#[test]
fn needs_no_window_length_the_same_burn_with_the_reset_an_hour_off_runs_on() {
    assert_eq!(
        assess_default(
            vec![session_window(70.0, MINUTE * 70.0)],
            Some(per_hour(30.0))
        ),
        None
    );
}

#[test]
fn lets_a_session_run_on_at_95_percent_when_the_window_resets_in_ten_minutes() {
    assert_eq!(
        assess_default(
            vec![session_window(95.0, MINUTE * 9.0)],
            Some(per_hour(60.0))
        ),
        None
    );
}

#[test]
fn stops_a_session_about_to_run_dry_half_an_hour_out_until_it_can_finish_the_stretch() {
    let pace = assess(
        vec![session_window(95.0, MINUTE * 30.0)],
        Some(per_hour(30.0)),
        false,
        MINUTE * 5.0,
    )
    .expect("held");
    // 5% at 30% an hour lasts ten minutes - enough once ten minutes stand
    // between it and the grace period.
    assert_eq!(pace.ready_at, NOW + MINUTE * 10.0);
}

#[test]
fn never_pauses_below_the_floor_however_fast_it_burns() {
    assert_eq!(
        assess_default(
            vec![session_window(
                PACE_FLOOR_PERCENTAGE.get() - 1.0,
                HOUR * 4.0
            )],
            Some(per_hour(90.0))
        ),
        None
    );
}

#[test]
fn never_pauses_before_a_burn_rate_is_known() {
    assert_eq!(
        assess_default(vec![session_window(90.0, HOUR * 4.0)], None),
        None
    );
}

#[test]
fn keeps_a_paused_session_paused_until_its_runway_reaches_the_reset() {
    let windows = vec![session_window(70.0, HOUR + RESET_GRACE_MS)];
    // Runway 54 minutes against an hour: inside the margin to start, short to finish.
    let rate = Percentage::new(30.0) / (MINUTE * 54.0);
    assert_eq!(assess_default(windows.clone(), Some(rate)), None);
    assert!(assess(windows, Some(rate), true, EARLY).is_some());
}

#[test]
fn still_pauses_near_a_session_reset_when_the_weekly_window_would_run_dry_first() {
    let pace = assess_default(
        vec![
            session_window(95.0, MINUTE * 5.0),
            weekly_window(90.0, DAY * 3.0),
        ],
        Some(per_hour(2.0)),
    )
    .expect("held");
    assert_eq!(pace.window.kind, UsageWindowKind::Weekly);
}

#[test]
fn still_paces_an_account_whose_usage_credits_would_carry_it_past_the_limit() {
    // Credits are not part of the formula: going over costs real money.
    assert!(assess_default(vec![session_window(90.0, HOUR * 4.0)], Some(per_hour(60.0))).is_some());
}

#[test]
fn never_paces_when_the_account_cant_be_read() {
    assert_eq!(
        assess_pace(AssessPaceParams {
            account: None,
            now: NOW,
            is_paused: false,
            margin_of: &|_| Some(EARLY),
            rate_of: &|_| Some(per_hour(60.0)),
        }),
        None
    );
}

#[test]
fn leaves_a_window_alone_while_its_pacing_is_off() {
    let account = account(vec![weekly_window(90.0, DAY * 3.0)]);
    assert_eq!(
        assess_pace(AssessPaceParams {
            account: Some(&account),
            now: NOW,
            is_paused: false,
            margin_of: &|kind| match kind {
                PacedWindowKind::Weekly => None,
                PacedWindowKind::Session => Some(EARLY),
            },
            rate_of: &|_| Some(per_hour(2.0)),
        }),
        None
    );
}

fn assess_weekly(
    utilization: f64,
    resets_in: SpanMilliseconds,
    is_paused: bool,
) -> Option<BindingWindow> {
    let account = account(vec![weekly_window(utilization, resets_in)]);
    assess_pace(AssessPaceParams {
        account: Some(&account),
        now: NOW,
        is_paused,
        margin_of: &|_| Some(HOUR * 12.0),
        rate_of: &|_| None,
    })
}

fn assess_weekly_at(
    utilization: f64,
    resets_in: SpanMilliseconds,
    rate: BurnRate,
) -> Option<BindingWindow> {
    let account = account(vec![weekly_window(utilization, resets_in)]);
    assess_pace(AssessPaceParams {
        account: Some(&account),
        now: NOW,
        is_paused: false,
        margin_of: &|_| Some(HOUR * 12.0),
        rate_of: &|_| Some(rate),
    })
}

#[test]
fn holds_a_week_at_96_percent_that_would_run_dry_before_its_reset_inside_the_margin() {
    let pace = assess_weekly_at(96.0, HOUR * 8.0, per_hour(2.0)).expect("held");
    // 4% left at 2% an hour lasts 2h, so it may go on 8h - 10m - 2h from now.
    assert_eq!(pace.kind, PacedWindowKind::Weekly);
    assert_eq!(
        pace.ready_at,
        NOW + HOUR * 8.0 - RESET_GRACE_MS - HOUR * 2.0
    );
    assert!(assess_weekly_at(96.0, HOUR * 8.0, per_hour(0.4)).is_none());
}

#[test]
fn a_weekly_window_needs_no_burn_rate_it_is_judged_against_an_even_spend() {
    // Four days in: an even spend reaches 90% in 2.3 more days.
    let pace = assess_weekly(90.0, DAY * 3.0, false).expect("held");
    assert_eq!(pace.kind, PacedWindowKind::Weekly);
    assert_eq!(pace.ready_at, NOW - DAY * 4.0 + WEEKLY_WINDOW_MS * 0.9);
}

#[test]
fn lets_a_week_run_on_within_the_margin_of_an_even_spend_however_fast_it_burned_today() {
    // Five days in, an even spend is at 71%; 75% is about seven hours ahead.
    assert_eq!(assess_weekly(75.0, DAY * 2.0, false), None);
}

#[test]
fn holds_a_paused_session_until_the_even_spend_catches_up() {
    assert!(assess_weekly(75.0, DAY * 2.0, true).is_some());
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
        rate_of: &|window| match window.kind {
            UsageWindowKind::Session => Some(per_hour(30.0)),
            UsageWindowKind::Weekly
            | UsageWindowKind::Model
            | UsageWindowKind::Credits
            | UsageWindowKind::Opaque => None,
        },
    });
    let session = forecasts.session.expect("a session forecast");
    let weekly = forecasts.weekly.expect("a weekly forecast");
    assert_eq!(session.run_out_at, Some(NOW + HOUR));
    // 30% in four days lasts another 9.3 days at that pace - past the reset.
    assert!(weekly.run_out_at.expect("a run-out") > NOW + DAY * 3.0);
    let even = weekly.even_percentage.expect("an even spend").get();
    assert!((even - 400.0 / 7.0).abs() < 1e-9);
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
fn rests_while_no_more_rest_than_work_is_needed_and_stops_past_that() {
    let enough = |runway| {
        is_rest_enough(RestEnoughParams {
            runway,
            continue_at: NOW + HOUR * 2.0,
            now: NOW,
        })
    };
    assert!(enough(HOUR));
    assert!(!enough(MINUTE * 30.0));
}

fn week_window_at_76() -> (UsageWindow, EpochMilliseconds, EpochMilliseconds) {
    let resets_in = HOUR * 54.0;
    let window = weekly_window(76.0, resets_in);
    let continue_at = NOW + resets_in - RESET_GRACE_MS;
    // An even spend reaches 76% about 14 hours from now.
    (window, continue_at, NOW + HOUR * 14.0)
}

#[test]
fn the_working_runway_lasts_what_is_left_at_the_measured_working_burn() {
    let (window, continue_at, ready_at) = week_window_at_76();
    // 24% left at 2% an hour of work lasts 12 hours of work.
    assert_eq!(
        working_runway(WorkingRunwayParams {
            window: &window,
            rate: per_hour(2.0),
            ready_at,
            continue_at,
        }),
        HOUR * 12.0
    );
}

#[test]
fn the_working_runway_falls_back_to_the_time_from_back_on_pace_to_running_free() {
    let (window, continue_at, ready_at) = week_window_at_76();
    assert_eq!(
        working_runway(WorkingRunwayParams {
            window: &window,
            rate: BurnRate::ZERO,
            ready_at,
            continue_at,
        }),
        continue_at - ready_at
    );
}

#[test]
fn sizes_a_weekly_rest_from_how_fast_the_agents_really_spend() {
    let (window, continue_at, ready_at) = week_window_at_76();
    let rest = |rate| {
        slow_rest_until(SlowRestParams {
            runway: working_runway(WorkingRunwayParams {
                window: &window,
                rate,
                ready_at,
                continue_at,
            }),
            ready_at,
            continue_at,
            now: NOW,
        }) - NOW
    };
    // The even spend's 40-hour runway asks for no more than the 5-minute
    // minimum; working at 1% an hour, the 24 hours of work left needs about 6
    // after each 5.
    assert_eq!(rest(BurnRate::ZERO), MINUTE * 5.0);
    assert!(rest(per_hour(1.0)) > MINUTE * 6.0);
}

fn spare(rate: Option<BurnRate>, utilization: f64) -> bool {
    let account = account(vec![session_window(
        utilization,
        HOUR * 2.0 + RESET_GRACE_MS,
    )]);
    has_room_to_spare(HasRoomParams {
        account: Some(&account),
        now: NOW,
        margin_of: &|_| Some(EARLY),
        rate_of: &|_| rate,
    })
}

#[test]
fn vouches_only_for_a_runway_reaching_well_past_the_reset() {
    // 30% left lasts 3h50m at 8% an hour - past 1.5 times the 2h left.
    assert!(spare(Some(per_hour(8.0)), 70.0));
    assert!(!spare(Some(per_hour(12.0)), 70.0));
}

#[test]
fn cant_vouch_before_a_burn_rate_is_measured_unless_the_window_is_under_the_floor() {
    assert!(!spare(None, 70.0));
    assert!(spare(None, PACE_FLOOR_PERCENTAGE.get() - 1.0));
}

#[test]
fn has_no_room_to_spare_without_an_account() {
    assert!(!has_room_to_spare(HasRoomParams {
        account: None,
        now: NOW,
        margin_of: &|_| Some(EARLY),
        rate_of: &|_| None,
    }));
}

fn rhythm(runway: SpanMilliseconds, time_left: SpanMilliseconds) -> WorkRhythm {
    work_rhythm(WorkRhythmParams { runway, time_left })
}

#[test]
fn works_the_shortest_stretch_and_rests_what_keeps_the_duty_the_account_can_afford() {
    // 1h of work affordable in 4h: a duty of a quarter, so 5m of work earns 15m of rest.
    assert_eq!(
        rhythm(HOUR, HOUR * 4.0),
        WorkRhythm {
            stretch: MINUTE * 5.0,
            rest: MINUTE * 15.0
        }
    );
}

#[test]
fn does_the_same_total_work_whatever_the_stretch_work_over_cycle_length_stays_the_duty() {
    let WorkRhythm { stretch, rest } = rhythm(HOUR * 3.0, HOUR * 10.0);
    assert!((stretch / (stretch + rest) - 0.3).abs() < 1e-9);
}

#[test]
fn works_longer_not_rests_less_when_the_duty_is_high() {
    // 9h of work affordable in 10h: a rest under five minutes is all
    // interruption, so the stretch grows to keep the duty at 90%.
    let WorkRhythm { stretch, rest } = rhythm(HOUR * 9.0, HOUR * 10.0);
    assert_eq!(rest, MINUTE * 5.0);
    assert!((stretch - MINUTE * 45.0).abs().get() < 1e-6);
}

#[test]
fn rests_the_minimum_when_the_account_can_afford_all_the_work() {
    assert_eq!(
        rhythm(HOUR * 5.0, HOUR * 2.0),
        WorkRhythm {
            stretch: MINUTE * 5.0,
            rest: MINUTE * 5.0
        }
    );
}

#[test]
fn rests_for_hours_only_when_the_account_can_afford_little_work() {
    // 6 minutes of work affordable in 10h: a duty of 1%.
    assert!(rhythm(MINUTE * 6.0, HOUR * 10.0).rest > HOUR);
}

#[test]
fn ends_the_rest_where_the_rhythm_says_however_long_the_session_had_worked() {
    assert_eq!(
        slow_rest_until(SlowRestParams {
            runway: HOUR,
            ready_at: NOW + HOUR * 12.0,
            continue_at: NOW + HOUR * 4.0,
            now: NOW,
        }),
        NOW + MINUTE * 15.0
    );
}

#[test]
fn never_rests_past_where_a_full_stop_would_end() {
    assert_eq!(
        slow_rest_until(SlowRestParams {
            runway: HOUR,
            ready_at: NOW + MINUTE * 8.0,
            continue_at: NOW + HOUR * 4.0,
            now: NOW,
        }),
        NOW + MINUTE * 8.0
    );
}

#[test]
fn a_forecast_is_over_pace_by_the_rule_the_pacing_loop_acts_on() {
    let account = account(vec![session_window(70.0, HOUR * 4.0)]);
    let forecasts = pace_forecasts(PaceForecastsParams {
        account: Some(&account),
        now: NOW,
        rate_of: &|_| Some(per_hour(30.0)),
    });
    let forecast = forecasts.session.expect("a session forecast");
    let is_over = |margin| {
        is_forecast_over_pace(ForecastOverPaceParams {
            forecast: &forecast,
            margin,
            now: NOW,
        })
    };
    assert!(is_over(EARLY));
    // A margin longer than the time left shrinks to half of it, so even a
    // ten-hour one leaves this window over pace.
    assert!(is_over(HOUR * 10.0));
}
