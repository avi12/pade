//! The usage-pacing formula - the port of `lib/usage/pacing/usage-pace.ts`,
//! pure so every rule is tested on its own and against the TypeScript's own
//! output (`fixtures/usage_pace.json`).
//!
//! Each paced window is judged by one question: if its agents stopped now, when
//! would it be back on pace (`on_pace_at`)? How far ahead of now that lies is
//! how far ahead of pace the window is spending. The two windows answer it their
//! own way, because they are spent differently:
//!
//! * session - worked through in one sitting, so it is judged by its burn rate
//!   while its agents work, with no window length assumed:
//!   `runway = (100% - used) / burn rate`, `on_pace_at = reset - RESET_GRACE_MS - runway`
//! * weekly - spread over days with nights and breaks between, so it is judged
//!   against an even spend from its start to its reset:
//!   `on_pace_at = start + used * WEEKLY_WINDOW_MS`
//!
//! Past `PACE_FLOOR_PERCENTAGE` used, a window more than the user's chosen margin
//! ahead of pace is over pace; a paused session is ready once it is back on pace;
//! a stepped-down one can afford its dearer choice again once it is well behind.
//! Near a reset a window paces no one - it is about to come back anyway.

use serde::{Deserialize, Serialize};

use crate::pacing::reset_time::usage_reset_time;
use crate::pacing::units::{BurnRate, EpochMilliseconds, Fraction, Percentage, SpanMilliseconds};
use crate::usage::{AccountUsage, UsageWindow, UsageWindowKind};

/// Below this much of a window spent, no one is paced however fast it burns.
pub const PACE_FLOOR_PERCENTAGE: Percentage = Percentage::new(60.0);
/// This close to its reset a window paces no one, and a session it paused
/// continues on its own.
pub const RESET_GRACE_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(10.0);
/// A vendor re-stamps a window's reset a little differently on each read
/// (sub-second jitter in the same window); a new window moves it by hours.
pub const RESET_JITTER_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(1.0);
/// A stepped-down session steps back up only once its runway reaches this far
/// past the reset - well clear of where it stepped down, so it doesn't see-saw.
const SPARE_RUNWAY_FRACTION: f64 = 1.5;
/// The chosen margin never asks for more than this share of the time left.
const MAXIMUM_MARGIN_SHARE: f64 = 0.5;
/// Two readings further apart than this span idle time, not burn - skipped.
const MAXIMUM_SAMPLE_GAP_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(5.0);
/// A burn rate needs at least this much working time behind it...
pub const MINIMUM_RATE_SPAN_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(10.0);
/// ...and only this much recent working time counts toward it.
pub const RATE_HISTORY_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(45.0);
/// The shortest rest a slowed session takes - a rest of seconds is all
/// interruption and no saving.
const MINIMUM_REST_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(5.0);
/// The shortest stretch a slowed session works between two rests - as long as
/// the shortest rest, so a rest never follows the last one at once.
const MINIMUM_WORK_STRETCH_MS: SpanMilliseconds = MINIMUM_REST_MS;
/// How long a weekly window runs, start to reset - the length its even spend is
/// drawn over.
pub const WEEKLY_WINDOW_MS: SpanMilliseconds = SpanMilliseconds::from_days(7.0);
/// A session rests only while the time left is at most this many times its runway.
const REST_TO_WORK_LIMIT: f64 = 2.0;

/// The windows pacing watches. A window of another kind (a per-model cap, extra
/// usage) is never paced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PacedWindowKind {
    Session,
    Weekly,
}

impl PacedWindowKind {
    /// Every paced kind, in the order a plan reports them.
    pub const ALL: [Self; 2] = [Self::Session, Self::Weekly];

    /// The paced kind a window kind is, or `None` for one pacing never watches.
    #[must_use]
    pub const fn from_window_kind(kind: UsageWindowKind) -> Option<Self> {
        match kind {
            UsageWindowKind::Session => Some(Self::Session),
            UsageWindowKind::Weekly => Some(Self::Weekly),
            UsageWindowKind::Model | UsageWindowKind::Credits | UsageWindowKind::Opaque => None,
        }
    }
}

/// How far ahead of pace each paced window may run before pacing acts, or
/// `None` for a window whose pacing is off.
pub type MarginOf<'a> = &'a dyn Fn(PacedWindowKind) -> Option<SpanMilliseconds>;

/// A session window's measured burn rate, or `None` while it isn't measured.
pub type RateOf<'a> = &'a dyn Fn(&UsageWindow) -> Option<BurnRate>;

/// A window's spent share, as the formula reads it.
#[must_use]
pub fn utilization_of(window: &UsageWindow) -> Percentage {
    Percentage::new(window.utilization)
}

/// A window's reset, or `None` while the vendor states none (or states one that
/// can't be read).
#[must_use]
pub fn reset_time_of(window: &UsageWindow) -> Option<EpochMilliseconds> {
    usage_reset_time(window.resets_at.as_deref())
}

/// The paced windows an account reports, in `PacedWindowKind::ALL` order - a plan
/// without a session limit (Claude Ultra) reports only its weekly one. Every
/// paced window while there is no reading, since nothing says which the plan has.
#[must_use]
pub fn reported_paced_kinds(account: Option<&AccountUsage>) -> Vec<PacedWindowKind> {
    let Some(account) = account else {
        return PacedWindowKind::ALL.to_vec();
    };

    PacedWindowKind::ALL
        .into_iter()
        .filter(|kind| {
            account
                .windows
                .iter()
                .any(|window| PacedWindowKind::from_window_kind(window.kind) == Some(*kind))
        })
        .collect()
}

/// One reading of a window's usage, taken while one of its agents was working.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSample {
    pub at: EpochMilliseconds,
    pub utilization: Percentage,
    #[serde(default)]
    pub resets_at: Option<String>,
}

/// Whether two readings are of the same window - by its reset, within the
/// jitter, never by the strings, which differ read to read. Readings whose
/// resets can't be read are the same window only when their strings agree.
///
/// The TypeScript tells an absent reset (`undefined`) from a null one; here
/// both are `None`, so a pair of those counts as the same window.
fn is_same_reset(previous: &UsageSample, current: &UsageSample) -> bool {
    let current_reset = usage_reset_time(current.resets_at.as_deref());
    let previous_reset = usage_reset_time(previous.resets_at.as_deref());
    match (current_reset, previous_reset) {
        (Some(current_reset), Some(previous_reset)) => {
            (current_reset - previous_reset).abs() < RESET_JITTER_MS
        }
        _ => current.resets_at == previous.resets_at,
    }
}

/// How fast a window is being spent while its agents work: the rise across
/// consecutive readings of the same window, over the working time between them.
/// `None` until enough working time is behind it.
#[must_use]
pub fn burn_rate(samples: &[UsageSample]) -> Option<BurnRate> {
    let mut spent = Percentage::ZERO;
    let mut span = SpanMilliseconds::ZERO;
    for pair in samples.windows(2) {
        let [previous, current] = pair else {
            continue;
        };

        let gap = current.at - previous.at;
        let is_working_stretch = gap > SpanMilliseconds::ZERO && gap <= MAXIMUM_SAMPLE_GAP_MS;
        let rise = current.utilization - previous.utilization;
        let is_falling = rise < Percentage::ZERO;
        if !is_same_reset(previous, current) || !is_working_stretch || is_falling {
            continue;
        }

        spent += rise;
        span += gap;
    }

    (span >= MINIMUM_RATE_SPAN_MS).then(|| spent / span)
}

/// A paced window being watched now: its kind, when it runs free
/// (`RESET_GRACE_MS` before its reset) and the user's margin for it.
#[derive(Debug, Clone, Copy)]
struct WatchedWindow {
    kind: PacedWindowKind,
    free_at: EpochMilliseconds,
    margin: SpanMilliseconds,
}

#[derive(Clone, Copy)]
struct WatchParams<'a> {
    window: &'a UsageWindow,
    now: EpochMilliseconds,
    margin_of: MarginOf<'a>,
}

/// The watch on a window, or `None` when it isn't paced now - an unpaced or
/// switched-off kind, an unknown or near reset, or usage under the floor.
fn watched_window(params: WatchParams<'_>) -> Option<WatchedWindow> {
    let WatchParams {
        window,
        now,
        margin_of,
    } = params;
    let kind = PacedWindowKind::from_window_kind(window.kind)?;
    let margin = margin_of(kind)?;
    let free_at = reset_time_of(window)? - RESET_GRACE_MS;
    let is_paced_now = free_at > now && utilization_of(window) >= PACE_FLOOR_PERCENTAGE;
    is_paced_now.then_some(WatchedWindow {
        kind,
        free_at,
        margin,
    })
}

/// What is left of a window at a burn rate lasts this long; with nothing being
/// spent, forever.
fn runway_of(window: &UsageWindow, rate: BurnRate) -> SpanMilliseconds {
    if rate.is_positive() {
        return (Percentage::FULL - utilization_of(window)) / rate;
    }

    SpanMilliseconds::new(f64::INFINITY)
}

/// When an even spend over a weekly window starting `WEEKLY_WINDOW_MS` before
/// `reset_at` reaches `utilization`.
fn even_spend_reaches(reset_at: EpochMilliseconds, utilization: Percentage) -> EpochMilliseconds {
    (reset_at - WEEKLY_WINDOW_MS) + utilization.of(WEEKLY_WINDOW_MS)
}

/// When an even spend over a weekly window reaches what it has used - when it is
/// back on pace if its agents stopped now. `None` while its reset is unknown
/// (the TypeScript answers NaN).
#[must_use]
pub fn even_spend_reached_at(window: &UsageWindow) -> Option<EpochMilliseconds> {
    Some(even_spend_reaches(
        reset_time_of(window)?,
        utilization_of(window),
    ))
}

/// Where a weekly window would stand with an even spend from its start to its
/// reset, by `now`.
fn even_spend_percentage(reset_at: EpochMilliseconds, now: EpochMilliseconds) -> Percentage {
    let elapsed = now - (reset_at - WEEKLY_WINDOW_MS);
    Percentage::from_ratio(elapsed / WEEKLY_WINDOW_MS)
}

#[derive(Clone, Copy)]
struct OnPaceParams<'a> {
    kind: PacedWindowKind,
    window: &'a UsageWindow,
    free_at: EpochMilliseconds,
    rate: Option<BurnRate>,
}

/// When a window would be back on pace if its agents stopped now, by its own
/// measure (see the top of this file) - `None` while a session window's burn
/// rate isn't measured yet.
fn on_pace_time(params: OnPaceParams<'_>) -> Option<EpochMilliseconds> {
    let OnPaceParams {
        kind,
        window,
        free_at,
        rate,
    } = params;
    match kind {
        PacedWindowKind::Weekly => even_spend_reached_at(window),
        PacedWindowKind::Session => rate.map(|rate| free_at - runway_of(window, rate)),
    }
}

#[derive(Clone, Copy)]
struct TooFarAheadParams {
    on_pace_at: EpochMilliseconds,
    free_at: EpochMilliseconds,
    margin: SpanMilliseconds,
    now: EpochMilliseconds,
}

/// The trigger: a window more than `margin` ahead of pace. A margin longer than
/// the time left can never be met - "2 h early" with 16 minutes to go would
/// leave the window unpaced right when it runs dry - so it shrinks to at most a
/// share of the time left.
fn is_too_far_ahead(params: TooFarAheadParams) -> bool {
    let TooFarAheadParams {
        on_pace_at,
        free_at,
        margin,
        now,
    } = params;
    let reachable = margin.shorter((free_at - now) * MAXIMUM_MARGIN_SHARE);
    on_pace_at - now > reachable
}

/// How far behind pace a window has to fall before a stepped-down session may
/// climb back: a session window, well clear of where it stepped down so it
/// doesn't see-saw; a weekly window, back on its even spend - the margin it
/// stepped down at is band enough over days.
fn spare_ms(
    kind: PacedWindowKind,
    free_at: EpochMilliseconds,
    now: EpochMilliseconds,
) -> SpanMilliseconds {
    match kind {
        PacedWindowKind::Weekly => SpanMilliseconds::ZERO,
        PacedWindowKind::Session => (free_at - now) * (SPARE_RUNWAY_FRACTION - 1.0),
    }
}

/// One paced window's live line for the Config panel (`pace_forecasts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaceForecast {
    pub window: UsageWindow,
    pub reset_at: EpochMilliseconds,
    /// When it runs out at its pace, or `None` while that isn't measured (or
    /// nothing is being spent).
    pub run_out_at: Option<EpochMilliseconds>,
    /// Whether enough working time is behind the pace to judge it.
    pub is_measured: bool,
    /// When it would be back on pace if its agents stopped now.
    pub on_pace_at: Option<EpochMilliseconds>,
    /// A weekly window's even spend by now - `None` for a session.
    pub even_percentage: Option<Percentage>,
}

#[derive(Clone, Copy)]
pub struct ForecastOverPaceParams<'a> {
    pub forecast: &'a PaceForecast,
    pub margin: SpanMilliseconds,
    pub now: EpochMilliseconds,
}

/// Whether a forecast is over pace now - what the Config panel's live line asks,
/// by the same rule the pacing loop acts on.
#[must_use]
pub fn is_forecast_over_pace(params: ForecastOverPaceParams<'_>) -> bool {
    let ForecastOverPaceParams {
        forecast,
        margin,
        now,
    } = params;
    let free_at = forecast.reset_at - RESET_GRACE_MS;
    let is_paced_now = utilization_of(&forecast.window) >= PACE_FLOOR_PERCENTAGE && free_at > now;
    let Some(on_pace_at) = forecast.on_pace_at else {
        return false;
    };

    is_paced_now
        && is_too_far_ahead(TooFarAheadParams {
            on_pace_at,
            free_at,
            margin,
            now,
        })
}

/// The window that holds a session longest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingWindow {
    pub kind: PacedWindowKind,
    pub window: UsageWindow,
    /// Zero while the window's burn rate isn't measured.
    pub rate: BurnRate,
    /// When the session may go on: once it is back on pace.
    pub ready_at: EpochMilliseconds,
    /// When the reset is close enough for the session to continue on its own.
    pub continue_at: EpochMilliseconds,
}

#[derive(Clone, Copy)]
pub struct AssessPaceParams<'a> {
    pub account: Option<&'a AccountUsage>,
    pub now: EpochMilliseconds,
    /// A paused session is held until it is back on pace, with no margin.
    pub is_paused: bool,
    pub margin_of: MarginOf<'a>,
    pub rate_of: RateOf<'a>,
}

/// Judge an account's pace, each window by its own measure. A window is over
/// pace once it runs more than its margin ahead. The window that holds the
/// session longest is the one reported: when it may go on (`ready_at`, once it
/// is back on pace) and when its reset is close enough to continue on its own
/// (`continue_at`). `None` when no window holds it.
#[must_use]
pub fn assess_pace(params: AssessPaceParams<'_>) -> Option<BindingWindow> {
    let AssessPaceParams {
        account,
        now,
        is_paused,
        margin_of,
        rate_of,
    } = params;
    let mut binding: Option<BindingWindow> = None;
    for window in &account?.windows {
        let Some(watched) = watched_window(WatchParams {
            window,
            now,
            margin_of,
        }) else {
            continue;
        };

        let rate = rate_of(window);
        let Some(on_pace_at) = on_pace_time(OnPaceParams {
            kind: watched.kind,
            window,
            free_at: watched.free_at,
            rate,
        }) else {
            continue;
        };

        let margin = if is_paused {
            SpanMilliseconds::ZERO
        } else {
            watched.margin
        };
        let is_over_pace = is_too_far_ahead(TooFarAheadParams {
            on_pace_at,
            free_at: watched.free_at,
            margin,
            now,
        });
        let ready_at = now.later(on_pace_at);
        let outlasts_binding = binding
            .as_ref()
            .is_none_or(|holding| ready_at > holding.ready_at);
        if is_over_pace && outlasts_binding {
            binding = Some(BindingWindow {
                kind: watched.kind,
                window: window.clone(),
                rate: rate.unwrap_or(BurnRate::ZERO),
                ready_at,
                continue_at: watched.free_at,
            });
        }
    }

    binding
}

/// Every paced window's forecast, by kind.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PaceForecasts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<PaceForecast>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly: Option<PaceForecast>,
}

impl PaceForecasts {
    fn slot(&mut self, kind: PacedWindowKind) -> &mut Option<PaceForecast> {
        match kind {
            PacedWindowKind::Session => &mut self.session,
            PacedWindowKind::Weekly => &mut self.weekly,
        }
    }
}

#[derive(Clone, Copy)]
pub struct PaceForecastsParams<'a> {
    pub account: Option<&'a AccountUsage>,
    pub now: EpochMilliseconds,
    pub rate_of: RateOf<'a>,
}

/// Every paced window's forecast, by kind - for the Config panel's live lines.
/// A session window's comes from its measured burn; a weekly window's from its
/// average spend since it started. The first window of each kind counts.
#[must_use]
pub fn pace_forecasts(params: PaceForecastsParams<'_>) -> PaceForecasts {
    let PaceForecastsParams {
        account,
        now,
        rate_of,
    } = params;
    let mut forecasts = PaceForecasts::default();
    let windows = account.map_or(&[][..], |account| account.windows.as_slice());
    for window in windows {
        let Some(kind) = PacedWindowKind::from_window_kind(window.kind) else {
            continue;
        };
        let Some(reset_at) = reset_time_of(window) else {
            continue;
        };

        let slot = forecasts.slot(kind);
        let is_forecastable = slot.is_none() && reset_at > now;
        if !is_forecastable {
            continue;
        }

        *slot = Some(forecast_of(ForecastParams {
            kind,
            window,
            reset_at,
            now,
            rate_of,
        }));
    }

    forecasts
}

#[derive(Clone, Copy)]
struct ForecastParams<'a> {
    kind: PacedWindowKind,
    window: &'a UsageWindow,
    reset_at: EpochMilliseconds,
    now: EpochMilliseconds,
    rate_of: RateOf<'a>,
}

fn forecast_of(params: ForecastParams<'_>) -> PaceForecast {
    let ForecastParams {
        kind,
        window,
        reset_at,
        now,
        rate_of,
    } = params;
    let free_at = reset_at - RESET_GRACE_MS;
    let rate = match kind {
        PacedWindowKind::Weekly => {
            let elapsed = (now - (reset_at - WEEKLY_WINDOW_MS)).longer(MINIMUM_RATE_SPAN_MS);
            Some(utilization_of(window) / elapsed)
        }
        PacedWindowKind::Session => rate_of(window),
    };
    let even_percentage = match kind {
        PacedWindowKind::Weekly => Some(even_spend_percentage(reset_at, now)),
        PacedWindowKind::Session => None,
    };
    PaceForecast {
        window: window.clone(),
        reset_at,
        run_out_at: rate
            .filter(|rate| rate.is_positive())
            .map(|rate| now + runway_of(window, rate)),
        is_measured: rate.is_some(),
        on_pace_at: on_pace_time(OnPaceParams {
            kind,
            window,
            free_at,
            rate,
        }),
        even_percentage,
    }
}

#[derive(Clone, Copy)]
pub struct WorkingRunwayParams<'a> {
    pub window: &'a UsageWindow,
    pub rate: BurnRate,
    pub ready_at: EpochMilliseconds,
    pub continue_at: EpochMilliseconds,
}

/// How long what is left of a holding window lasts while its agents work: at the
/// measured working burn, the way the agents actually spend it. Until that is
/// measured, the time from when it is back on pace to when it runs free - for a
/// session window the same runway, for a weekly one the even spend's, which
/// assumes round-the-clock work and so sizes rests far too short.
#[must_use]
pub fn working_runway(params: WorkingRunwayParams<'_>) -> SpanMilliseconds {
    let WorkingRunwayParams {
        window,
        rate,
        ready_at,
        continue_at,
    } = params;
    if rate.is_positive() {
        return runway_of(window, rate);
    }

    continue_at - ready_at
}

#[derive(Clone, Copy)]
pub struct RestEnoughParams {
    /// `working_runway`.
    pub runway: SpanMilliseconds,
    pub continue_at: EpochMilliseconds,
    pub now: EpochMilliseconds,
}

/// Whether resting can bring a session back on pace - it needs no more rest than
/// work. Past that, a stop does the job better than rests longer than the work
/// between them.
#[must_use]
pub fn is_rest_enough(params: RestEnoughParams) -> bool {
    let time_left = params.continue_at - params.now;
    time_left <= params.runway * REST_TO_WORK_LIMIT
}

#[derive(Clone, Copy)]
pub struct HasRoomParams<'a> {
    pub account: Option<&'a AccountUsage>,
    pub now: EpochMilliseconds,
    pub margin_of: MarginOf<'a>,
    pub rate_of: RateOf<'a>,
}

/// Whether an account could afford a dearer effort or model again: every watched
/// window is far enough behind pace. A session window with no measured burn yet
/// can't vouch for it.
#[must_use]
pub fn has_room_to_spare(params: HasRoomParams<'_>) -> bool {
    let HasRoomParams {
        account,
        now,
        margin_of,
        rate_of,
    } = params;
    let Some(account) = account else {
        return false;
    };

    for window in &account.windows {
        let Some(watched) = watched_window(WatchParams {
            window,
            now,
            margin_of,
        }) else {
            continue;
        };

        let on_pace_at = on_pace_time(OnPaceParams {
            kind: watched.kind,
            window,
            free_at: watched.free_at,
            rate: rate_of(window),
        });
        let is_short = on_pace_at.is_none_or(|on_pace_at| {
            now - on_pace_at < spare_ms(watched.kind, watched.free_at, now)
        });
        if is_short {
            return false;
        }
    }

    true
}

#[derive(Clone, Copy)]
pub struct WorkRhythmParams {
    /// `working_runway`.
    pub runway: SpanMilliseconds,
    pub time_left: SpanMilliseconds,
}

/// How long to work, then how long to rest, in a cycle that repeats.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorkRhythm {
    pub stretch: SpanMilliseconds,
    pub rest: SpanMilliseconds,
}

/// The shortest waits that still get all the work the account can afford done.
///
/// What is left lasts `runway` of work, and there is `time_left` to cover, so the
/// account can work a share of that time: `duty = runway / time_left`. A cycle of
/// work `stretch` then rest averages out to that duty when
/// `rest = stretch * (1 / duty - 1)` for any stretch - so the total work done is
/// the same, and the waits are shortest with the shortest stretch worth starting
/// (`MINIMUM_WORK_STRETCH_MS`). A rest under `MINIMUM_REST_MS` is all
/// interruption and no saving, so with a high duty the stretch grows instead,
/// keeping the rest at its minimum and the duty intact. The rest is long only
/// when the account can afford little work (a small duty), which no choice of
/// rest length changes.
#[must_use]
pub fn work_rhythm(params: WorkRhythmParams) -> WorkRhythm {
    let WorkRhythmParams { runway, time_left } = params;
    let has_time_and_work = time_left > SpanMilliseconds::ZERO && runway > SpanMilliseconds::ZERO;
    let duty = if has_time_and_work {
        Fraction::new((runway / time_left).min(1.0))
    } else {
        Fraction::NONE
    };
    if duty <= Fraction::NONE {
        return WorkRhythm {
            stretch: MINIMUM_WORK_STRETCH_MS,
            rest: MINIMUM_REST_MS.longer(time_left),
        };
    }

    if duty >= Fraction::WHOLE {
        return WorkRhythm {
            stretch: MINIMUM_WORK_STRETCH_MS,
            rest: MINIMUM_REST_MS,
        };
    }

    let rest_per_work = 1.0 / duty.get() - 1.0;
    let stretch = MINIMUM_WORK_STRETCH_MS.longer(MINIMUM_REST_MS / rest_per_work);
    WorkRhythm {
        stretch,
        rest: stretch * rest_per_work,
    }
}

#[derive(Clone, Copy)]
pub struct SlowRestParams {
    /// `working_runway`.
    pub runway: SpanMilliseconds,
    pub ready_at: EpochMilliseconds,
    pub continue_at: EpochMilliseconds,
    pub now: EpochMilliseconds,
}

/// When a slowed session's rest ends: its rhythm's rest from now, never past the
/// moment a full stop would end.
#[must_use]
pub fn slow_rest_until(params: SlowRestParams) -> EpochMilliseconds {
    let SlowRestParams {
        runway,
        ready_at,
        continue_at,
        now,
    } = params;
    let WorkRhythm { rest, .. } = work_rhythm(WorkRhythmParams {
        runway,
        time_left: continue_at - now,
    });
    ready_at.earlier(now + rest)
}

#[cfg(test)]
mod tests {
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
        let pace = assess_default(vec![session_window(70.0, HOUR * 4.0)], Some(per_hour(30.0)))
            .expect("held");
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
        assert!(
            assess_default(vec![session_window(90.0, HOUR * 4.0)], Some(per_hour(60.0))).is_some()
        );
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
}
