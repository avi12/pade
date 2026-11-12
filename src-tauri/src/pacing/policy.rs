//! Usage pacing's numeric policy - the port of the pure margins in
//! `lib/usage/pacing/pacing-policy.ts` (its text helpers stay with the UI).
//!
//! The goal: get as much done as possible without either usage limit running out
//! before it resets. While an account is on track to run out early, pacing slows
//! its agents only as much as that needs, mildest first (`PacingStep::ORDER`).

use serde::{Deserialize, Serialize};

use crate::pacing::units::SpanMilliseconds;
use crate::pacing::usage_pace::PacedWindowKind;

/// One thing pacing does to a session, mildest first: turn the effort down,
/// move to a cheaper model, give short rests between stretches of work, and -
/// right at the edge - stop until there's room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PacingStep {
    Effort,
    Model,
    Rest,
    Stop,
}

impl PacingStep {
    /// Every step, in the order pacing climbs them.
    pub const ORDER: [Self; 4] = [Self::Effort, Self::Model, Self::Rest, Self::Stop];
}

/// How far ahead of pace a window may run before pacing acts: a little slack, so
/// a short burst doesn't slow anything, sized to the window - minutes for the
/// 5-hour window, hours for the week. Near the reset it shrinks to a share of the
/// time left (`usage_pace`).
#[must_use]
pub const fn pacing_margin(kind: PacedWindowKind) -> SpanMilliseconds {
    match kind {
        PacedWindowKind::Session => SpanMilliseconds::from_minutes(30.0),
        PacedWindowKind::Weekly => SpanMilliseconds::from_hours(6.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_runs_thirty_minutes_ahead_and_a_week_six_hours() {
        assert_eq!(
            pacing_margin(PacedWindowKind::Session),
            SpanMilliseconds::new(30.0 * 60_000.0)
        );
        assert_eq!(
            pacing_margin(PacedWindowKind::Weekly),
            SpanMilliseconds::new(6.0 * 3_600_000.0)
        );
    }

    #[test]
    fn steps_climb_from_the_mildest_to_a_stop() {
        assert_eq!(PacingStep::ORDER.first(), Some(&PacingStep::Effort));
        assert_eq!(PacingStep::ORDER.last(), Some(&PacingStep::Stop));
    }

    #[test]
    fn steps_travel_under_their_typescript_names() {
        assert_eq!(
            serde_json::to_string(&PacingStep::Effort).unwrap_or_default(),
            "\"effort\""
        );
    }
}
