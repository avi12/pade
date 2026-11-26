//! How an over-pace session is slowed by rests: the work-and-rest rhythm that
//! keeps it to its account's keep share (`work_rhythm`), and when one rest ends
//! (`slow_rest_until`).

use serde::{Deserialize, Serialize};

use crate::pacing::units::{EpochMilliseconds, Fraction, SpanMilliseconds};

/// The shortest rest a slowed session takes - a rest of seconds is all
/// interruption and no saving.
const MINIMUM_REST_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(5.0);
/// The shortest stretch a slowed session works between two rests - long enough
/// to get something done, so rests come as short, regular breaks.
const MINIMUM_WORK_STRETCH_MS: SpanMilliseconds = SpanMilliseconds::from_minutes(15.0);
/// The longest a slowed session rests before it works a stretch again, so the
/// pace is measured afresh rather than trusted for days.
const MAXIMUM_REST_MS: SpanMilliseconds = SpanMilliseconds::from_hours(2.0);

/// How long to work, then how long to rest, in a cycle that repeats.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorkRhythm {
    pub stretch: SpanMilliseconds,
    pub rest: SpanMilliseconds,
}

/// The shortest waits that still keep an over-pace account to its keep share.
///
/// A cycle of work `stretch` then rest averages out to the keep share (the
/// `duty`) when `rest = stretch * (1 / duty - 1)` for any stretch - so the waits
/// are shortest with the shortest stretch worth working
/// (`MINIMUM_WORK_STRETCH_MS`). A rest under `MINIMUM_REST_MS` is all
/// interruption and no saving, so with a high duty the stretch grows instead,
/// keeping the rest at its minimum and the duty intact. No rest runs past
/// `MAXIMUM_REST_MS`: under the duty that takes, the next stretch measures the
/// pace again.
#[must_use]
pub fn work_rhythm(duty: Fraction) -> WorkRhythm {
    let bounded_duty = duty.get().clamp(0.0, 1.0);
    let longest_rest_duty =
        MINIMUM_WORK_STRETCH_MS.get() / (MINIMUM_WORK_STRETCH_MS.get() + MAXIMUM_REST_MS.get());
    let is_within_longest_rest = bounded_duty <= longest_rest_duty;
    if is_within_longest_rest {
        return WorkRhythm {
            stretch: MINIMUM_WORK_STRETCH_MS,
            rest: MAXIMUM_REST_MS,
        };
    }

    let is_full_duty = bounded_duty >= 1.0;
    if is_full_duty {
        return WorkRhythm {
            stretch: MINIMUM_WORK_STRETCH_MS,
            rest: MINIMUM_REST_MS,
        };
    }

    let rest_per_work = 1.0 / bounded_duty - 1.0;
    let stretch = MINIMUM_WORK_STRETCH_MS.longer(MINIMUM_REST_MS / rest_per_work);
    WorkRhythm {
        stretch,
        rest: stretch * rest_per_work,
    }
}

#[derive(Clone, Copy)]
pub struct SlowRestParams {
    /// The binding window's keep share (`assess_pace`).
    pub keep: Fraction,
    pub ready_at: EpochMilliseconds,
    pub now: EpochMilliseconds,
}

/// When a slowed session's rest ends: its rhythm's rest from now, never past the
/// moment it would be back on pace anyway.
#[must_use]
pub fn slow_rest_until(params: SlowRestParams) -> EpochMilliseconds {
    let SlowRestParams {
        keep,
        ready_at,
        now,
    } = params;
    ready_at.earlier(now + work_rhythm(keep).rest)
}
