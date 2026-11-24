//! The usage-pacing formula - the port of `lib/usage/pacing/engine/usage-pace.ts`,
//! pure so every rule is tested on its own and against the TypeScript's own
//! output (`fixtures/usage_pace.json`).
//!
//! Pacing spreads what is left of a window over the time left until it resets,
//! and slows an account only as much as that takes, from the first percent on -
//! there is no usage level below which it looks away. Each paced window answers
//! one question: what share of its current spend can the account keep?
//!
//! * `affordable = (100% - used) / time left`
//! * `allowance = min(ALLOWANCE, max(0, ALLOWANCE - (used - even spend so far)))`
//! * `may spend = affordable + allowance / payout time`
//! * `keep = may spend / pace`
//!
//! The allowance is the room for bursts. An even spend climbs through nights and
//! breaks while nothing is spent, so idle hours refill it (up to `ALLOWANCE`),
//! and every rise in usage drains it. One session's burst fits in it and slows
//! nothing; several sessions bursting at once drain it within hours, and the
//! keep share slides down toward what lasts to the reset.
//!
//! Below `SLOW_DOWN_KEEP` a window is over pace; a paused session is ready once
//! its keep share reaches 1; a stepped-down one can afford its dearer choice
//! again once the keep share is well above it (`STEP_UP_KEEP`). Near a reset a
//! window paces no one - it is about to come back anyway.

mod forecast;
mod rhythm;

pub use forecast::{
    is_forecast_over_pace, pace_forecasts, PaceForecast, PaceForecasts, PaceForecastsParams,
};
pub use rhythm::{slow_rest_until, work_rhythm, SlowRestParams, WorkRhythm};

use serde::{Deserialize, Serialize};

use crate::pacing::load_shedding::Load;
use crate::pacing::reset_time::usage_reset_time;
use crate::pacing::units::{BurnRate, EpochMilliseconds, Fraction, Percentage, SpanMilliseconds};
use crate::usage::{AccountUsage, UsageWindow, UsageWindowKind};

/// This close to its reset a window paces no one, and a session it paused
/// continues on its own.
pub const RESET_GRACE_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(10.0);
/// A vendor re-stamps a window's reset a little differently on each read
/// (sub-second jitter in the same window); a new window moves it by hours.
pub const RESET_JITTER_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(1.0);
/// Two readings further apart than this span idle time, not burn - skipped.
const MAXIMUM_SAMPLE_GAP_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(5.0);
/// A burn rate needs at least this much working time behind it...
pub const MINIMUM_RATE_SPAN_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(10.0);
/// ...and only this much recent working time counts toward it.
pub const RATE_HISTORY_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(45.0);
/// How long a weekly window runs, start to reset.
pub const WEEKLY_WINDOW_MS: SpanMilliseconds = SpanMilliseconds::from_days(7.0);
/// How long a session window runs, start to reset.
const SESSION_WINDOW_MS: SpanMilliseconds = SpanMilliseconds::from_hours(5.0);
/// The burst allowance: how far ahead of an even spend a window may run, in
/// percent of the window, before the overspend has to be paid back.
const ALLOWANCE_PERCENTAGE: Percentage = Percentage::new(10.0);
/// The allowance is paid out over this share of the window - eight hours of a
/// week, a quarter of an hour of a session window.
const PAYOUT_SHARE: f64 = 8.0 / (7.0 * 24.0);
/// Under this keep share a window is over pace - a little slack, so the jitter
/// of whole-percent readings doesn't slow anything.
pub const SLOW_DOWN_KEEP: Fraction = Fraction::new(0.95);
/// A stepped-down session steps back up only once its account could keep this
/// much more than it spends - clear of what one step saves, so it doesn't
/// see-saw.
const STEP_UP_KEEP: Fraction = Fraction::new(1.3);
/// At or under this keep share there is next to nothing to spend: a rest would
/// be all rest, so the session stops until there is room.
pub const STOP_KEEP: Fraction = Fraction::new(0.05);
/// Halvings that find when a paused session is back on pace - a week down to
/// well under a second.
const READY_SEARCH_STEPS: u32 = 40;

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

    /// How long a window of this kind runs, start to reset - the length its even
    /// spend and its allowance's payout are drawn over.
    #[must_use]
    pub const fn window_length(self) -> SpanMilliseconds {
        match self {
            Self::Session => SESSION_WINDOW_MS,
            Self::Weekly => WEEKLY_WINDOW_MS,
        }
    }
}

/// The pace each window's account is measured at, or `None` while it isn't
/// measured yet.
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
    #[serde(rename = "at")]
    pub time: EpochMilliseconds,
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

        let gap = current.time - previous.time;
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

#[derive(Debug, Clone, Copy)]
pub struct ProjectedRateParams {
    pub measured: Option<BurnRate>,
    /// The account's burn per unit of load, measured last.
    pub rate_per_load: Option<BurnRate>,
    /// The account's working load now (`load_shedding`).
    pub load: Load,
}

/// How far the load forecast may lift a measured burn - load and burn are only
/// loosely tied, and an unbounded forecast once stopped sessions until the
/// weekly reset.
const MAXIMUM_PROJECTED_RISE: f64 = 3.0;

/// An account's pace as its agents run now: the measured burn, raised to what
/// the sessions working now would spend at the burn per unit of load measured
/// last, at most `MAXIMUM_PROJECTED_RISE` times the measured burn - so a
/// session starting on, or switching to, a dear model counts at once. Either
/// alone when the other is unknown; `None` when both are.
#[must_use]
pub fn projected_rate(params: ProjectedRateParams) -> Option<BurnRate> {
    let ProjectedRateParams {
        measured,
        rate_per_load,
        load,
    } = params;
    let forecast = rate_per_load.map(|rate| BurnRate::new(rate.get() * load.get()));
    match (measured, forecast) {
        (Some(measured), Some(forecast)) => Some(BurnRate::new(
            measured
                .get()
                .max(forecast.get())
                .min(measured.get() * MAXIMUM_PROJECTED_RISE),
        )),
        (measured, forecast) => measured.or(forecast),
    }
}

/// A paced window while it still paces: its kind, its reset and when it runs
/// free (`RESET_GRACE_MS` before it).
#[derive(Debug, Clone, Copy)]
struct PacedWindow {
    kind: PacedWindowKind,
    reset_at: EpochMilliseconds,
    free_at: EpochMilliseconds,
}

/// The paced window, or `None` for an unpaced kind or an unknown or near reset.
fn paced_window(window: &UsageWindow, now: EpochMilliseconds) -> Option<PacedWindow> {
    let kind = PacedWindowKind::from_window_kind(window.kind)?;
    let reset_at = reset_time_of(window)?;
    let free_at = reset_at - RESET_GRACE_MS;
    (free_at > now).then_some(PacedWindow {
        kind,
        reset_at,
        free_at,
    })
}

impl PacedWindow {
    /// What the window lets its account spend at `moment`, with `used` spent by then
    /// (see the top of this file).
    fn may_spend_at(self, used: Percentage, moment: EpochMilliseconds) -> BurnRate {
        let length = self.kind.window_length();
        let elapsed = (moment - (self.reset_at - length))
            .longer(SpanMilliseconds::ZERO)
            .shorter(length);
        let even = Percentage::from_ratio(elapsed / length);
        let allowance = ALLOWANCE_PERCENTAGE
            .get()
            .min((ALLOWANCE_PERCENTAGE - (used - even)).get().max(0.0));
        let affordable = (Percentage::FULL - used) / (self.free_at - moment);
        let payout = Percentage::new(allowance) / (length * PAYOUT_SHARE);
        BurnRate::new((affordable.get() + payout.get()).max(0.0))
    }
}

/// A paced window weighed against the pace its account is measured at, now.
#[derive(Clone, Copy)]
struct PaceCheck<'a> {
    paced: PacedWindow,
    window: &'a UsageWindow,
    rate: BurnRate,
    now: EpochMilliseconds,
}

impl PaceCheck<'_> {
    /// The share of its spend the window lets its account keep now: 1 is exactly
    /// on pace, under it the account must shed the rest, over it there is room
    /// to spare. Infinite while nothing is spent.
    fn keep(self) -> Fraction {
        if !self.rate.is_positive() {
            return Fraction::new(f64::INFINITY);
        }

        let may_spend = self
            .paced
            .may_spend_at(utilization_of(self.window), self.now);
        Fraction::new(may_spend / self.rate)
    }

    /// When the window would let its account spend at its pace again if its
    /// agents stopped now - its even spend climbing back toward what is used.
    /// Its free time when that never comes first.
    fn ready_time(self) -> EpochMilliseconds {
        let used = utilization_of(self.window);
        let is_ready_at = |moment| self.paced.may_spend_at(used, moment) >= self.rate;
        if is_ready_at(self.now) {
            return self.now;
        }

        let mut early = self.now;
        let mut late = self.paced.free_at;
        for _ in 0..READY_SEARCH_STEPS {
            let middle = EpochMilliseconds::new(early.get().midpoint(late.get()));
            if is_ready_at(middle) {
                late = middle;
            } else {
                early = middle;
            }
        }

        late
    }
}

/// The window that keeps an account's spend the least.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingWindow {
    pub kind: PacedWindowKind,
    pub window: UsageWindow,
    /// The pace the account is measured at.
    pub rate: BurnRate,
    /// The share of that pace the window lets the account keep.
    pub keep: Fraction,
    /// When the session may go on: once the window lets it spend at its pace.
    pub ready_at: EpochMilliseconds,
    /// When the reset is close enough for the session to continue on its own.
    pub continue_at: EpochMilliseconds,
}

#[derive(Clone, Copy)]
pub struct AssessPaceParams<'a> {
    pub account: Option<&'a AccountUsage>,
    pub now: EpochMilliseconds,
    /// A paused session is held until its keep share is back to 1.
    pub is_paused: bool,
    pub rate_of: RateOf<'a>,
}

/// Judge an account's pace, each paced window by its keep share at its pace
/// (`rate_of`). A window is over pace under `SLOW_DOWN_KEEP` - or, for a paused
/// session, until it is back to 1. The window that keeps the least is the one
/// reported: its keep share, when the session may go on (`ready_at`) and when
/// the reset is close enough to continue on its own (`continue_at`). `None` when
/// no window holds it - or none is measured yet.
#[must_use]
pub fn assess_pace(params: AssessPaceParams<'_>) -> Option<BindingWindow> {
    let AssessPaceParams {
        account,
        now,
        is_paused,
        rate_of,
    } = params;
    let threshold = if is_paused {
        Fraction::WHOLE
    } else {
        SLOW_DOWN_KEEP
    };
    let mut binding: Option<BindingWindow> = None;
    for window in &account?.windows {
        let (Some(paced), Some(rate)) = (paced_window(window, now), rate_of(window)) else {
            continue;
        };

        let check = PaceCheck {
            paced,
            window,
            rate,
            now,
        };
        let keep = check.keep();
        let keeps_least = binding.as_ref().is_none_or(|holding| keep < holding.keep);
        if keep < threshold && keeps_least {
            binding = Some(BindingWindow {
                kind: paced.kind,
                window: window.clone(),
                rate,
                keep,
                ready_at: check.ready_time(),
                continue_at: paced.free_at,
            });
        }
    }

    binding
}

#[derive(Clone, Copy)]
pub struct HasRoomParams<'a> {
    pub account: Option<&'a AccountUsage>,
    pub now: EpochMilliseconds,
    pub rate_of: RateOf<'a>,
}

/// Whether an account could afford a dearer effort or model again: every paced
/// window keeps well above its spend (`STEP_UP_KEEP`). A window with no measured
/// pace yet can't vouch for it.
#[must_use]
pub fn has_room_to_spare(params: HasRoomParams<'_>) -> bool {
    let HasRoomParams {
        account,
        now,
        rate_of,
    } = params;
    let Some(account) = account else {
        return false;
    };

    account.windows.iter().all(|window| {
        let Some(paced) = paced_window(window, now) else {
            return true;
        };

        rate_of(window).is_some_and(|rate| {
            let check = PaceCheck {
                paced,
                window,
                rate,
                now,
            };
            check.keep() >= STEP_UP_KEEP
        })
    })
}

#[cfg(test)]
mod tests;
