//! Wall-clock time as epoch durations and milliseconds, and calendar days since the epoch.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
