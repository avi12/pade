//! The usage-pacing formula - the port of `lib/usage/pacing/engine/usage-pace.ts`,
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

mod forecast;
mod rhythm;

pub use forecast::{
    is_forecast_over_pace, pace_forecasts, ForecastOverPaceParams, PaceForecast, PaceForecasts,
    PaceForecastsParams,
};
pub use rhythm::{
    has_room_to_spare, is_rest_enough, slow_rest_until, work_rhythm, working_runway, HasRoomParams,
    RestEnoughParams, SlowRestParams, WorkRhythm, WorkRhythmParams, WorkingRunwayParams,
};

use serde::{Deserialize, Serialize};

use crate::pacing::reset_time::usage_reset_time;
use crate::pacing::units::{BurnRate, EpochMilliseconds, Percentage, SpanMilliseconds};
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
/// How long a weekly window runs, start to reset - the length its even spend is
/// drawn over.
pub const WEEKLY_WINDOW_MS: SpanMilliseconds = SpanMilliseconds::from_days(7.0);

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

#[cfg(test)]
mod tests;
