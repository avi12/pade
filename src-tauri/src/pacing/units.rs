//! The units the pacing formula computes in, each its own type so a point in
//! time, a span, a percentage and a burn rate can never be mixed up: only the
//! combinations the formula uses compile (a time minus a time is a span, a
//! percentage over a span is a rate, a percentage over a rate is a span).
//!
//! Every unit wraps an `f64` because the TypeScript formula computes in JS
//! numbers: an on-pace time is `start + used / 100 * week`, which lands between
//! two milliseconds, and a span is negative once a time has passed. Each is
//! `#[serde(transparent)]`, so on the wire it is the bare number the zod schemas
//! carry. `std::time::Duration` is unsigned and whole-nanosecond, so it cannot
//! be the span itself; [`SpanMilliseconds`] converts to and from it at the edge.

use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, Mul, Sub};
use std::time::Duration;

const MILLISECONDS_PER_SECOND: f64 = 1_000.0;
const MILLISECONDS_PER_MINUTE: f64 = 60_000.0;
const MINUTES_PER_HOUR: f64 = 60.0;
const HOURS_PER_DAY: f64 = 24.0;
const PERCENT_PER_WHOLE: f64 = 100.0;

/// A point in time, in milliseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EpochMilliseconds(f64);

impl EpochMilliseconds {
    #[must_use]
    pub const fn new(milliseconds: f64) -> Self {
        Self(milliseconds)
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// The later of two points in time.
    #[must_use]
    pub fn later(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    /// The earlier of two points in time.
    #[must_use]
    pub fn earlier(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }
}

impl Sub for EpochMilliseconds {
    type Output = SpanMilliseconds;

    fn sub(self, earlier: Self) -> SpanMilliseconds {
        SpanMilliseconds(self.0 - earlier.0)
    }
}

impl Add<SpanMilliseconds> for EpochMilliseconds {
    type Output = Self;

    fn add(self, span: SpanMilliseconds) -> Self {
        Self(self.0 + span.0)
    }
}

impl Sub<SpanMilliseconds> for EpochMilliseconds {
    type Output = Self;

    fn sub(self, span: SpanMilliseconds) -> Self {
        Self(self.0 - span.0)
    }
}

/// A signed length of time, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpanMilliseconds(f64);

impl SpanMilliseconds {
    pub const ZERO: Self = Self(0.0);

    #[must_use]
    pub const fn new(milliseconds: f64) -> Self {
        Self(milliseconds)
    }

    #[must_use]
    pub const fn from_minutes(minutes: f64) -> Self {
        Self(minutes * MILLISECONDS_PER_MINUTE)
    }

    #[must_use]
    pub const fn from_hours(hours: f64) -> Self {
        Self::from_minutes(hours * MINUTES_PER_HOUR)
    }

    #[must_use]
    pub const fn from_days(days: f64) -> Self {
        Self::from_hours(days * HOURS_PER_DAY)
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    #[must_use]
    pub fn shorter(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    #[must_use]
    pub fn longer(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    #[must_use]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

impl Add for SpanMilliseconds {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl AddAssign for SpanMilliseconds {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl Sub for SpanMilliseconds {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl Mul<f64> for SpanMilliseconds {
    type Output = Self;

    fn mul(self, factor: f64) -> Self {
        Self(self.0 * factor)
    }
}

impl Div<f64> for SpanMilliseconds {
    type Output = Self;

    fn div(self, divisor: f64) -> Self {
        Self(self.0 / divisor)
    }
}

/// How many times one span holds another.
impl Div for SpanMilliseconds {
    type Output = f64;

    fn div(self, other: Self) -> f64 {
        self.0 / other.0
    }
}

impl From<Duration> for SpanMilliseconds {
    fn from(duration: Duration) -> Self {
        Self(duration.as_secs_f64() * MILLISECONDS_PER_SECOND)
    }
}

/// A span has no [`Duration`] when it is negative or not a finite number.
impl TryFrom<SpanMilliseconds> for Duration {
    type Error = std::time::TryFromFloatSecsError;

    fn try_from(span: SpanMilliseconds) -> Result<Self, Self::Error> {
        Self::try_from_secs_f64(span.0 / MILLISECONDS_PER_SECOND)
    }
}

/// How much of a usage window is spent, in percent. Not clamped: the formula
/// reads whatever the vendor reports, and an even spend can pass 100.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Percentage(f64);

impl Percentage {
    pub const ZERO: Self = Self(0.0);
    /// A window with nothing left.
    pub const FULL: Self = Self(PERCENT_PER_WHOLE);

    #[must_use]
    pub const fn new(percent: f64) -> Self {
        Self(percent)
    }

    /// The percentage a ratio stands for (`0.5` is 50%).
    #[must_use]
    pub fn from_ratio(ratio: f64) -> Self {
        Self(ratio * PERCENT_PER_WHOLE)
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// This share of `span`.
    #[must_use]
    pub fn of(self, span: SpanMilliseconds) -> SpanMilliseconds {
        span * (self.0 / PERCENT_PER_WHOLE)
    }
}

impl Add for Percentage {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl AddAssign for Percentage {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl Sub for Percentage {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

/// A percentage spent over a span is a burn rate.
impl Div<SpanMilliseconds> for Percentage {
    type Output = BurnRate;

    fn div(self, span: SpanMilliseconds) -> BurnRate {
        BurnRate(self.0 / span.0)
    }
}

/// A percentage left at a burn rate lasts a span.
impl Div<BurnRate> for Percentage {
    type Output = SpanMilliseconds;

    fn div(self, rate: BurnRate) -> SpanMilliseconds {
        SpanMilliseconds(self.0 / rate.0)
    }
}

/// How fast a window is spent: percent per millisecond.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BurnRate(f64);

impl BurnRate {
    /// No burn measured: the sentinel the formula's `rate > 0` checks read.
    pub const ZERO: Self = Self(0.0);

    #[must_use]
    pub const fn new(percent_per_millisecond: f64) -> Self {
        Self(percent_per_millisecond)
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    #[must_use]
    pub fn is_positive(self) -> bool {
        self.0 > 0.0
    }
}

/// How many times one rate holds another.
impl Div for BurnRate {
    type Output = f64;

    fn div(self, other: Self) -> f64 {
        self.0 / other.0
    }
}

/// A share of a whole, nominally 0 to 1: what an account must shed, what a step
/// saves, how much of the time a session works.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fraction(f64);

impl Fraction {
    pub const NONE: Self = Self(0.0);
    pub const WHOLE: Self = Self(1.0);

    #[must_use]
    pub const fn new(share: f64) -> Self {
        Self(share)
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_minus_a_time_is_a_span_and_a_span_moves_a_time() {
        let start = EpochMilliseconds::new(1_000.0);
        let end = start + SpanMilliseconds::from_minutes(1.0);
        assert_eq!(end - start, SpanMilliseconds::new(60_000.0));
        assert_eq!(end - SpanMilliseconds::from_minutes(1.0), start);
    }

    #[test]
    fn a_percentage_over_a_span_is_a_rate_and_over_a_rate_is_a_span() {
        let span = SpanMilliseconds::from_hours(2.0);
        let rate = Percentage::new(30.0) / span;
        assert_eq!(Percentage::new(30.0) / rate, span);
    }

    #[test]
    fn a_share_of_a_span_scales_it() {
        assert_eq!(
            Percentage::new(50.0).of(SpanMilliseconds::from_days(1.0)),
            SpanMilliseconds::from_hours(12.0)
        );
    }

    #[test]
    fn a_span_converts_to_a_duration_only_while_it_is_not_negative() {
        let span = SpanMilliseconds::from_minutes(1.0);
        assert_eq!(Duration::try_from(span), Ok(Duration::from_secs(60)));
        assert!(Duration::try_from(SpanMilliseconds::new(-1.0)).is_err());
        assert_eq!(
            SpanMilliseconds::from(Duration::from_secs(60)),
            SpanMilliseconds::from_minutes(1.0)
        );
    }

    #[test]
    fn units_travel_as_bare_numbers() {
        let json = serde_json::to_string(&EpochMilliseconds::new(5.0)).unwrap_or_default();
        assert_eq!(json, "5.0");
    }
}
