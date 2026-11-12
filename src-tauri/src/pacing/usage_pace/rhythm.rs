//! How a window that holds a session is worked through: how long the rest of it
//! lasts while agents work (`working_runway`), whether a rest is enough, whether
//! a stepped-down session has room to climb back, and the work-and-rest rhythm
//! that spreads a slowdown (`work_rhythm`, `slow_rest_until`).

use serde::{Deserialize, Serialize};

use super::{
    on_pace_time, runway_of, spare_ms, watched_window, MarginOf, OnPaceParams, RateOf, WatchParams,
};
use crate::pacing::units::{BurnRate, EpochMilliseconds, Fraction, SpanMilliseconds};
use crate::usage::{AccountUsage, UsageWindow};

/// The shortest rest a slowed session takes - a rest of seconds is all
/// interruption and no saving.
const MINIMUM_REST_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(5.0);
/// The shortest stretch a slowed session works between two rests - as long as
/// the shortest rest, so a rest never follows the last one at once.
const MINIMUM_WORK_STRETCH_MS: SpanMilliseconds = MINIMUM_REST_MS;

/// A session rests only while the time left is at most this many times its runway.
const REST_TO_WORK_LIMIT: f64 = 2.0;

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
