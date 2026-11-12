//! The steps usage pacing takes on one running subagent - the port of
//! `lib/usage/pacing/engine/subagent-steps.ts`. A subagent works inside its parent's
//! turn, so the parent's between-turn steps never reach it; the agent's own hook
//! does, before every tool call the subagent makes. Its steps, cheapest first:
//!
//! * gate - held a few seconds before each of its tool calls. A subagent spends
//!   in short cycles of one model reply and one tool call, so a hold of h
//!   seconds on a cycle of c cuts its rate by h / (c + h). The first hold is
//!   sized to the share the account must shed, and every further step doubles
//!   it, up to a bound well inside the hook's own timeout.
//! * wrap up - at the longest hold, its next tool call is refused with "finish
//!   now with what you have and report back", so it returns.
//!
//! While the account must shed more than `NO_SPAWN_SHED`, no session on it may
//! start a subagent at all. As room comes back each hold halves, and goes once it
//! is shorter than the shortest; a subagent told to wrap up is finishing and is
//! let be.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::pacing::units::Fraction;

/// A subagent's typical cycle of one reply and one tool call, in seconds.
const TOOL_CYCLE_SECONDS: f64 = 20.0;
/// Each further gate step multiplies the hold by this; easing divides by it.
const HOLD_GROWTH: u32 = 2;
/// Above this share of its spend to shed, an account starts no new subagents.
pub const NO_SPAWN_SHED: Fraction = Fraction::new(0.3);

/// How long a gate holds a subagent before each tool call, in whole seconds -
/// never zero, as the zod schema's `int().positive()` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HoldSeconds(NonZeroU32);

impl HoldSeconds {
    /// A hold known to be positive at compile time.
    const fn fixed(seconds: u32) -> Self {
        match NonZeroU32::new(seconds) {
            Some(positive) => Self(positive),
            None => panic!("a hold is at least a second"),
        }
    }

    /// A hold of `seconds`, or `None` for zero.
    pub fn new(seconds: u32) -> Option<Self> {
        NonZeroU32::new(seconds).map(Self)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }

    fn seconds(self) -> f64 {
        f64::from(self.get())
    }
}

/// The shortest hold worth a step, and the longest before wrapping up.
pub const MINIMUM_HOLD_SECONDS: HoldSeconds = HoldSeconds::fixed(5);
pub const MAXIMUM_HOLD_SECONDS: HoldSeconds = HoldSeconds::fixed(45);

/// One running subagent's pacing step, as the agent's hook reads it: held before
/// each tool call, or told to finish now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "step",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SubagentStep {
    Gate { hold_seconds: HoldSeconds },
    WrapUp,
}

/// A subagent's next step and what it saves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NextSubagentStep {
    pub step: SubagentStep,
    pub saving: Fraction,
}

/// The share of a subagent's unheld rate a hold saves.
#[must_use]
pub fn hold_saving(hold: HoldSeconds) -> Fraction {
    Fraction::new(hold.seconds() / (TOOL_CYCLE_SECONDS + hold.seconds()))
}

/// The hold that saves `fraction` of a subagent's rate, within the bounds.
/// A fraction that is not a number gets the shortest hold (the TypeScript
/// answers NaN there).
pub fn hold_for_saving(fraction: Fraction) -> HoldSeconds {
    let is_beyond_reach = fraction >= Fraction::WHOLE;
    let exact = if is_beyond_reach {
        MAXIMUM_HOLD_SECONDS.seconds()
    } else {
        TOOL_CYCLE_SECONDS * fraction.get() / (1.0 - fraction.get())
    };
    let rounded = exact.round().clamp(
        MINIMUM_HOLD_SECONDS.seconds(),
        MAXIMUM_HOLD_SECONDS.seconds(),
    );
    (MINIMUM_HOLD_SECONDS.get()..=MAXIMUM_HOLD_SECONDS.get())
        .find(|seconds| f64::from(*seconds).total_cmp(&rounded).is_eq())
        .and_then(HoldSeconds::new)
        .unwrap_or(MINIMUM_HOLD_SECONDS)
}

/// Whether an account sheds so much it starts no new subagents - not while its
/// shed is unknown.
#[must_use]
pub fn is_blocking_spawns(shed: Option<Fraction>) -> bool {
    shed.is_some_and(|shed| shed >= NO_SPAWN_SHED)
}

#[derive(Debug, Clone, Copy)]
pub struct NextStepParams {
    pub current: Option<SubagentStep>,
    /// What the account must shed; unknown, the first hold is the shortest.
    pub shed: Option<Fraction>,
}

/// A subagent's next step and what it saves of the load it spends now (already
/// held by its current step) - `None` once it is wrapping up.
#[must_use]
pub fn next_subagent_step(params: NextStepParams) -> Option<NextSubagentStep> {
    let NextStepParams { current, shed } = params;
    let Some(current) = current else {
        let hold = hold_for_saving(shed.unwrap_or(Fraction::NONE));
        return Some(NextSubagentStep {
            step: SubagentStep::Gate { hold_seconds: hold },
            saving: hold_saving(hold),
        });
    };

    let SubagentStep::Gate { hold_seconds: held } = current else {
        return None;
    };

    if held >= MAXIMUM_HOLD_SECONDS {
        return Some(NextSubagentStep {
            step: SubagentStep::WrapUp,
            saving: Fraction::WHOLE,
        });
    }

    let longer =
        HoldSeconds::new(held.get().saturating_mul(HOLD_GROWTH))?.min(MAXIMUM_HOLD_SECONDS);
    Some(NextSubagentStep {
        step: SubagentStep::Gate {
            hold_seconds: longer,
        },
        saving: Fraction::new(
            1.0 - (TOOL_CYCLE_SECONDS + held.seconds()) / (TOOL_CYCLE_SECONDS + longer.seconds()),
        ),
    })
}

/// A subagent's step once its account has room again: a hold halves, and goes
/// once it falls under the shortest (`None`); a wrap-up stays, the subagent
/// finishing.
#[must_use]
pub fn eased_subagent_step(current: SubagentStep) -> Option<SubagentStep> {
    let SubagentStep::Gate { hold_seconds } = current else {
        return Some(current);
    };

    let halved = hold_seconds.get() / HOLD_GROWTH;
    if halved < MINIMUM_HOLD_SECONDS.get() {
        return None;
    }

    HoldSeconds::new(halved).map(|hold| SubagentStep::Gate { hold_seconds: hold })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(seconds: u32) -> SubagentStep {
        SubagentStep::Gate {
            hold_seconds: HoldSeconds::new(seconds).expect("a positive hold"),
        }
    }

    fn close(actual: Fraction, expected: f64, tolerance: f64) -> bool {
        (actual.get() - expected).abs() < tolerance
    }

    #[test]
    fn the_gate_length_saves_the_share_it_was_sized_for() {
        assert!(close(
            hold_saving(hold_for_saving(Fraction::new(0.5))),
            0.5,
            0.05
        ));
        assert!(close(
            hold_saving(hold_for_saving(Fraction::new(0.6))),
            0.6,
            0.05
        ));
    }

    #[test]
    fn the_gate_length_grows_with_the_shed_and_stays_within_its_bounds() {
        assert!(hold_for_saving(Fraction::new(0.2)) < hold_for_saving(Fraction::new(0.5)));
        assert_eq!(hold_for_saving(Fraction::NONE), MINIMUM_HOLD_SECONDS);
        assert_eq!(hold_for_saving(Fraction::new(0.99)), MAXIMUM_HOLD_SECONDS);
        assert_eq!(hold_for_saving(Fraction::WHOLE), MAXIMUM_HOLD_SECONDS);
    }

    #[test]
    fn gates_first_sized_to_the_shed_and_the_shortest_hold_while_the_shed_is_unknown() {
        let sized = next_subagent_step(NextStepParams {
            current: None,
            shed: Some(Fraction::new(0.5)),
        });
        assert_eq!(
            sized.map(|next| next.step),
            Some(SubagentStep::Gate {
                hold_seconds: hold_for_saving(Fraction::new(0.5))
            })
        );
        let unknown = next_subagent_step(NextStepParams {
            current: None,
            shed: None,
        });
        assert_eq!(
            unknown.map(|next| next.step),
            Some(SubagentStep::Gate {
                hold_seconds: MINIMUM_HOLD_SECONDS
            })
        );
    }

    #[test]
    fn doubles_the_hold_up_to_the_longest_saving_what_the_longer_hold_adds() {
        let next = next_subagent_step(NextStepParams {
            current: Some(gate(10)),
            shed: Some(Fraction::new(0.4)),
        })
        .expect("a next step");
        assert_eq!(next.step, gate(20));
        assert!(close(
            next.saving,
            1.0 - (20.0 + 10.0) / (20.0 + 20.0),
            1e-9
        ));
        let capped = next_subagent_step(NextStepParams {
            current: Some(gate(40)),
            shed: Some(Fraction::new(0.4)),
        });
        assert_eq!(
            capped.map(|next| next.step),
            Some(SubagentStep::Gate {
                hold_seconds: MAXIMUM_HOLD_SECONDS
            })
        );
    }

    #[test]
    fn wraps_up_at_the_longest_hold_saving_all_it_spends() {
        let next = next_subagent_step(NextStepParams {
            current: Some(SubagentStep::Gate {
                hold_seconds: MAXIMUM_HOLD_SECONDS,
            }),
            shed: Some(Fraction::new(0.4)),
        });
        assert_eq!(
            next,
            Some(NextSubagentStep {
                step: SubagentStep::WrapUp,
                saving: Fraction::WHOLE
            })
        );
    }

    #[test]
    fn has_nothing_further_once_it_is_wrapping_up() {
        assert_eq!(
            next_subagent_step(NextStepParams {
                current: Some(SubagentStep::WrapUp),
                shed: Some(Fraction::new(0.9)),
            }),
            None
        );
    }

    #[test]
    fn easing_halves_a_hold_and_lifts_it_under_the_shortest() {
        assert_eq!(eased_subagent_step(gate(40)), Some(gate(20)));
        assert_eq!(
            eased_subagent_step(SubagentStep::Gate {
                hold_seconds: MINIMUM_HOLD_SECONDS
            }),
            None
        );
    }

    #[test]
    fn easing_lets_a_subagent_told_to_wrap_up_finish() {
        assert_eq!(
            eased_subagent_step(SubagentStep::WrapUp),
            Some(SubagentStep::WrapUp)
        );
    }

    #[test]
    fn blocks_new_subagents_only_past_the_threshold_never_on_an_unknown_shed() {
        assert!(is_blocking_spawns(Some(NO_SPAWN_SHED)));
        assert!(!is_blocking_spawns(Some(Fraction::new(
            NO_SPAWN_SHED.get() - 0.01
        ))));
        assert!(!is_blocking_spawns(None));
    }

    #[test]
    fn a_step_travels_as_the_zod_schema_shapes_it() {
        let wire = serde_json::to_value(gate(10)).expect("serializes");
        assert_eq!(
            wire,
            serde_json::json!({ "step": "gate", "holdSeconds": 10 })
        );
        let wrap = serde_json::to_value(SubagentStep::WrapUp).expect("serializes");
        assert_eq!(wrap, serde_json::json!({ "step": "wrapUp" }));
        assert!(serde_json::from_value::<SubagentStep>(
            serde_json::json!({ "step": "gate", "holdSeconds": 0 })
        )
        .is_err());
    }
}
