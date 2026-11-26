//! Window durations and epoch seconds as the meter shows them: classify a window by its length, label it, and write a reset as ISO-8601.

use crate::usage::readings::UsageWindowKind;

const LAST_DAY_OF_FOUR_YEARS: i64 = 1_460;
const DAYS_PER_CENTURY: i64 = 36_524;
const LAST_DAY_OF_ERA: i64 = 146_096;
const DAYS_PER_COMMON_YEAR: i64 = 365;
const YEARS_PER_LEAP_CYCLE: i64 = 4;
const YEARS_PER_CENTURY: i64 = 100;
const MONTHS_PER_MARCH_CYCLE: i64 = 5;
const DAYS_PER_MARCH_CYCLE: i64 = 153;
const FIRST_JANUARY_MONTH_POINTER: i64 = 10;
const MARCH_MONTH_NUMBER: i64 = 3;
const JANUARY_POINTER_OFFSET: i64 = 9;
const YEARS_PER_ERA: i64 = 400;

/// The two subscription window durations (Codex's account-wide caps, Muse's
/// allowance), used to classify a window by its length into the same
/// session/weekly kinds the meter and the auto-resume scheduler understand. Any
/// other duration passes through as [`UsageWindowKind::Opaque`].
pub(super) const FIVE_HOUR_WINDOW_SECONDS: i64 = 5 * SECONDS_PER_HOUR;
pub(super) const WEEKLY_WINDOW_SECONDS: i64 = DAYS_PER_WEEK * SECONDS_PER_DAY;

pub(super) const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;
const DAYS_PER_WEEK: i64 = 7;

/// Classify a subscription window (a Codex account-wide cap, a Muse allowance
/// window) by its duration.
pub(super) fn window_kind_for_duration(seconds: i64) -> UsageWindowKind {
    match seconds {
        FIVE_HOUR_WINDOW_SECONDS => UsageWindowKind::Session,
        WEEKLY_WINDOW_SECONDS => UsageWindowKind::Weekly,
        _ => UsageWindowKind::Opaque,
    }
}

/// A human label for a window duration in seconds (`18000` → "5-hour", `604800`
/// → "Weekly", `86400` → "1-day"). Session/weekly windows get product names from
/// the frontend; this is what an opaque window shows.
pub(super) fn humanize_window_seconds(seconds: i64) -> String {
    let is_whole = |unit_seconds: i64| seconds > 0 && seconds % unit_seconds == 0;
    if is_whole(SECONDS_PER_DAY) {
        let days = seconds / SECONDS_PER_DAY;
        let is_whole_week = days == DAYS_PER_WEEK;
        if is_whole_week {
            return "Weekly".to_string();
        }
        return format!("{days}-day");
    }
    if is_whole(SECONDS_PER_HOUR) {
        return format!("{}-hour", seconds / SECONDS_PER_HOUR);
    }
    if is_whole(SECONDS_PER_MINUTE) {
        return format!("{}-minute", seconds / SECONDS_PER_MINUTE);
    }
    format!("{seconds}-second")
}

/// A Unix timestamp (seconds since the epoch) as an ISO-8601 UTC string
/// (`YYYY-MM-DDTHH:MM:SSZ`). Codex reports window resets as epoch seconds; the
/// frontend countdown parses ISO strings (like the Claude windows), so we
/// normalize here. Pure `std` arithmetic - Howard Hinnant's `civil_from_days`,
/// no date crate.
pub(crate) fn unix_seconds_to_iso8601(seconds: i64) -> String {
    // The algorithm counts from 0000-03-01, so leap days fall at a year's end.
    const DAYS_FROM_MARCH_ZERO_TO_UNIX_EPOCH: i64 = 719_468;
    const DAYS_PER_ERA: i64 = 146_097;

    let days = seconds.div_euclid(SECONDS_PER_DAY);
    let seconds_of_day = seconds.rem_euclid(SECONDS_PER_DAY);
    let hour = seconds_of_day / SECONDS_PER_HOUR;
    let minute = (seconds_of_day % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    let second = seconds_of_day % SECONDS_PER_MINUTE;

    let shifted = days + DAYS_FROM_MARCH_ZERO_TO_UNIX_EPOCH;
    let era = shifted.div_euclid(DAYS_PER_ERA);
    let day_of_era = shifted - era * DAYS_PER_ERA;
    let year_of_era = (day_of_era - day_of_era / LAST_DAY_OF_FOUR_YEARS
        + day_of_era / DAYS_PER_CENTURY
        - day_of_era / LAST_DAY_OF_ERA)
        / DAYS_PER_COMMON_YEAR;
    let day_of_year = day_of_era
        - (DAYS_PER_COMMON_YEAR * year_of_era + year_of_era / YEARS_PER_LEAP_CYCLE
            - year_of_era / YEARS_PER_CENTURY);
    let month_pointer = (MONTHS_PER_MARCH_CYCLE * day_of_year + 2) / DAYS_PER_MARCH_CYCLE;
    let day = day_of_year - (DAYS_PER_MARCH_CYCLE * month_pointer + 2) / MONTHS_PER_MARCH_CYCLE + 1;
    let is_march_to_december = month_pointer < FIRST_JANUARY_MONTH_POINTER;
    let month = if is_march_to_december {
        month_pointer + MARCH_MONTH_NUMBER
    } else {
        month_pointer - JANUARY_POINTER_OFFSET
    };
    let year = year_of_era + era * YEARS_PER_ERA + i64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}
