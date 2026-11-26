//! Wall-clock time as epoch durations and milliseconds, and calendar days since the epoch.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

const YEARS_PER_ERA: i64 = 400;
const DAYS_PER_MARCH_CYCLE: i64 = 153;
const MARCH_MONTH_NUMBER: i64 = 3;
const JANUARY_POINTER_OFFSET: i64 = 9;
const MONTHS_PER_MARCH_CYCLE: i64 = 5;
const DAYS_PER_COMMON_YEAR: i64 = 365;
const YEARS_PER_LEAP_CYCLE: i64 = 4;
const YEARS_PER_CENTURY: i64 = 100;
const DAYS_PER_ERA: i64 = 146_097;
const DAYS_FROM_ERA_ORIGIN_TO_UNIX_EPOCH: i64 = 719_468;

/// The wall-clock time since the Unix epoch, or zero for a clock set before it -
/// a timestamp, a nonce or an expiry check never has a better answer to give.
pub(crate) fn since_unix_epoch() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

/// `time` as whole milliseconds since the Unix epoch: zero for a moment before
/// it, saturating at `u64::MAX` - the one millisecond stamp every timestamp,
/// file name and modified-time comparison reads.
pub(crate) fn milliseconds_since_unix_epoch(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |since| {
        u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
    })
}

/// The current wall-clock time in milliseconds since the Unix epoch.
pub(crate) fn now_milliseconds() -> u64 {
    milliseconds_since_unix_epoch(SystemTime::now())
}

/// Days from 1970-01-01 to a calendar date, using Howard Hinnant's
/// `days_from_civil` algorithm (years start in March, so leap days fall last).
pub(crate) fn days_since_epoch(year: i64, month: i64, day: i64) -> i64 {
    let is_january_or_february = month <= 2;
    let shifted_year = if is_january_or_february {
        year - 1
    } else {
        year
    };
    let era = shifted_year.div_euclid(YEARS_PER_ERA);
    let year_of_era = shifted_year - era * YEARS_PER_ERA;
    let day_of_year = (DAYS_PER_MARCH_CYCLE
        * (month + {
            let is_after_february = month > 2;
            if is_after_february {
                -MARCH_MONTH_NUMBER
            } else {
                JANUARY_POINTER_OFFSET
            }
        })
        + 2)
        / MONTHS_PER_MARCH_CYCLE
        + day
        - 1;
    let day_of_era = year_of_era * DAYS_PER_COMMON_YEAR + year_of_era / YEARS_PER_LEAP_CYCLE
        - year_of_era / YEARS_PER_CENTURY
        + day_of_year;
    era * DAYS_PER_ERA + day_of_era - DAYS_FROM_ERA_ORIGIN_TO_UNIX_EPOCH
}
