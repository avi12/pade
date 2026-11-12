//! A window's reset time as a point in time - the port of the TypeScript
//! `usageResetTime`. The usage endpoint stamps resets with microseconds
//! (`2026-11-14T00:59:59.533870+00:00`); the digits past the millisecond are
//! trimmed first so every parser reads the timestamp the same way. Hand-rolled
//! on `util::days_since_epoch` to keep a date crate out.
//!
//! Where `new Date(text)` would fall back to a legacy format, or read a
//! timestamp with no zone as local time, this answers `None`: the endpoint
//! always states a zone, and an unknown reset is already what `None` means.

use crate::pacing::units::EpochMilliseconds;
use crate::util::days_since_epoch;

const MILLISECOND_DIGITS: usize = 3;
const MILLISECONDS_PER_SECOND: u32 = 1_000;
const MILLISECONDS_PER_MINUTE: u32 = 60_000;
const MILLISECONDS_PER_HOUR: u32 = 3_600_000;
const MILLISECONDS_PER_DAY: f64 = 86_400_000.0;
const MINUTES_PER_HOUR: i32 = 60;
const MAXIMUM_HOUR: u32 = 23;
const MAXIMUM_MINUTE_OR_SECOND: u32 = 59;
const MONTHS: u32 = 12;
const DATE_PARTS: usize = 3;

/// The time a reset string stands for, or `None` when it is absent, empty or
/// not a timestamp pacing can read.
#[must_use]
pub fn usage_reset_time(iso: Option<&str>) -> Option<EpochMilliseconds> {
    let trimmed = trim_sub_millisecond_digits(iso?);
    let (date, rest) = trimmed.split_once(['T', ' '])?;
    let (year, month, day) = parse_date(date)?;
    let (clock, offset_minutes) = split_zone(rest)?;
    let milliseconds_of_day = parse_clock(clock)?;

    let days = i32::try_from(days_since_epoch(
        i64::from(year),
        i64::from(month),
        i64::from(day),
    ))
    .ok()?;
    let local = f64::from(days) * MILLISECONDS_PER_DAY + f64::from(milliseconds_of_day);
    let offset_milliseconds = f64::from(offset_minutes) * f64::from(MILLISECONDS_PER_MINUTE);
    Some(EpochMilliseconds::new(local - offset_milliseconds))
}

/// `.533870` becomes `.533`: the first fraction of four or more digits is cut
/// to three, as the TypeScript regex `(\.\d{3})\d+` does.
fn trim_sub_millisecond_digits(iso: &str) -> String {
    for (point, _) in iso.match_indices('.') {
        let digits = iso[point + 1..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        if digits > MILLISECOND_DIGITS {
            let kept_end = point + 1 + MILLISECOND_DIGITS;
            let dropped_end = point + 1 + digits;
            return format!("{}{}", &iso[..kept_end], &iso[dropped_end..]);
        }
    }

    iso.to_string()
}

fn parse_date(date: &str) -> Option<(i32, u32, u32)> {
    let parts: Vec<&str> = date.split('-').collect();
    let [year, month, day] = <[&str; DATE_PARTS]>::try_from(parts).ok()?;
    let year = fixed_digits(year, 4).and_then(|digits| i32::try_from(digits).ok())?;
    let month = fixed_digits(month, 2)?;
    let day = fixed_digits(day, 2)?;
    let is_real_day =
        (1..=MONTHS).contains(&month) && day >= 1 && day <= days_in_month(year, month);
    is_real_day.then_some((year, month, day))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let is_leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    match month {
        2 if is_leap_year => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The clock part and the zone's offset from UTC in minutes. A timestamp with
/// no zone has none.
fn split_zone(rest: &str) -> Option<(&str, i32)> {
    if let Some(clock) = rest.strip_suffix('Z') {
        return Some((clock, 0));
    }

    let sign_at = rest.rfind(['+', '-'])?;
    let (clock, zone) = rest.split_at(sign_at);
    let sign = if zone.starts_with('-') { -1 } else { 1 };
    let digits = zone[1..].replace(':', "");
    let hours = fixed_digits(digits.get(..2)?, 2)?;
    let minutes = fixed_digits(digits.get(2..)?, 2)?;
    let is_real_zone =
        digits.len() == 4 && hours <= MAXIMUM_HOUR && minutes <= MAXIMUM_MINUTE_OR_SECOND;
    let offset = i32::try_from(hours).ok()? * MINUTES_PER_HOUR + i32::try_from(minutes).ok()?;
    is_real_zone.then_some((clock, sign * offset))
}

/// `HH:MM`, `HH:MM:SS` or `HH:MM:SS.f` (one to three fraction digits), as
/// milliseconds into the day.
fn parse_clock(clock: &str) -> Option<u32> {
    let (whole, fraction) = clock.split_once('.').unwrap_or((clock, ""));
    let mut parts = whole.split(':');
    let hour = fixed_digits(parts.next()?, 2)?;
    let minute = fixed_digits(parts.next()?, 2)?;
    let second = parts.next().map_or(Some(0), |text| fixed_digits(text, 2))?;
    let is_real_clock = parts.next().is_none()
        && hour <= MAXIMUM_HOUR
        && minute <= MAXIMUM_MINUTE_OR_SECOND
        && second <= MAXIMUM_MINUTE_OR_SECOND;
    if !is_real_clock {
        return None;
    }

    let milliseconds = parse_fraction(fraction)?;
    Some(
        hour * MILLISECONDS_PER_HOUR
            + minute * MILLISECONDS_PER_MINUTE
            + second * MILLISECONDS_PER_SECOND
            + milliseconds,
    )
}

/// A fraction of a second of up to three digits, in milliseconds (`.5` is 500).
fn parse_fraction(fraction: &str) -> Option<u32> {
    if fraction.is_empty() {
        return Some(0);
    }

    let is_plain =
        fraction.len() <= MILLISECOND_DIGITS && fraction.bytes().all(|b| b.is_ascii_digit());
    if !is_plain {
        return None;
    }

    format!("{fraction:0<3}").parse().ok()
}

/// A number written with exactly `width` digits.
fn fixed_digits(text: &str, width: usize) -> Option<u32> {
    let is_exact = text.len() == width && text.bytes().all(|b| b.is_ascii_digit());
    if !is_exact {
        return None;
    }

    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset(iso: &str) -> Option<f64> {
        usage_reset_time(Some(iso)).map(EpochMilliseconds::get)
    }

    #[test]
    fn reads_the_vendors_microsecond_stamp_to_the_millisecond() {
        assert_eq!(
            reset("2026-11-14T00:59:59.533870+00:00"),
            Some(1_794_617_999_533.0)
        );
        assert_eq!(reset("2026-11-14T00:59:59.533Z"), Some(1_794_617_999_533.0));
    }

    #[test]
    fn applies_the_zone_offset() {
        assert_eq!(
            reset("2026-11-14T02:59:59+02:00"),
            reset("2026-11-14T00:59:59Z")
        );
        assert_eq!(
            reset("2026-11-13T19:59:59-0500"),
            reset("2026-11-14T00:59:59Z")
        );
    }

    #[test]
    fn a_short_fraction_is_milliseconds_not_digits() {
        assert_eq!(reset("1970-01-01T00:00:01.5Z"), Some(1_500.0));
    }

    #[test]
    fn is_unknown_for_anything_that_is_not_a_timestamp() {
        assert_eq!(usage_reset_time(None), None);
        assert_eq!(reset(""), None);
        assert_eq!(reset("reset"), None);
        assert_eq!(reset("2026-02-30T00:00:00Z"), None);
        assert_eq!(reset("2026-11-14T00:59:59"), None);
    }
}
