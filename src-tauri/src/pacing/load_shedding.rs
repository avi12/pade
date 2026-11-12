//! How usage pacing shares one account's slowdown among the sessions spending it
//! - the port of `lib/usage/pacing/engine/load-shedding.ts`.
//!
//! An account's burn rate is one number for everything on it: every session, and
//! every subagent those sessions run. So when it runs over pace, the question is
//! not "should this session slow down" but "how much of the account's spend must
//! go, and from whom". Each session's share is estimated from what drives its
//! spend, or measured where the agent's own transcripts say what it spent:
//!
//! `load = model price * effort weight * working share * (1 + subagents)`
//!
//! The estimate is rescaled onto the measured sessions' scale
//! (`measured_per_estimated`) so both kinds of load share one account. The
//! account must shed the fraction its burn runs over the pace that lasts to the
//! reset: `shed = 1 - target rate / measured rate`, `target = left / time to reset`.
//!
//! Every candidate session offers its next step and what that step would save (a
//! fraction of its own load). The steps are taken in order of the share of the
//! account they save - the biggest saving first - until together they cover the
//! shed. A session takes at most one step per round.

use std::cmp::Ordering;
use std::iter::Sum;
use std::ops::{Add, Mul};

use serde::{Deserialize, Serialize};

use crate::agents::running_model::ModelChoice;
use crate::pacing::model_label::{effort_step, EffortQuery};
use crate::pacing::token_price::price_ratio;
use crate::pacing::units::{BurnRate, EpochMilliseconds, Fraction, Percentage};
use crate::pacing::usage_pace::utilization_of;
use crate::usage::UsageWindow;

/// An effort ladder spans these weights, lowest level to highest - more effort
/// means more thinking tokens on the same model.
const LOWEST_EFFORT_WEIGHT: f64 = 0.5;
const HIGHEST_EFFORT_WEIGHT: f64 = 1.5;
/// A model with no effort, or a level no ladder knows, sits at the middle.
const MIDDLE_EFFORT_WEIGHT: f64 = 1.0;

/// What a choice costs per working minute, relative to the others.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CostWeight(f64);

impl CostWeight {
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// How many times one cost holds another.
impl std::ops::Div for CostWeight {
    type Output = f64;

    fn div(self, other: Self) -> f64 {
        self.0 / other.0
    }
}

/// How much of an account's spend a session (or one of its subagents) drives, on
/// one scale for the whole account.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Load(f64);

impl Load {
    pub const ZERO: Self = Self(0.0);

    #[must_use]
    pub const fn new(load: f64) -> Self {
        Self(load)
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl Add for Load {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sum for Load {
    fn sum<Loads: Iterator<Item = Self>>(loads: Loads) -> Self {
        loads.fold(Self::ZERO, Add::add)
    }
}

/// A share of a load.
impl Mul<Fraction> for Load {
    type Output = Self;

    fn mul(self, share: Fraction) -> Self {
        Self(self.0 * share.get())
    }
}

/// A share of a load, from the share's side.
impl Mul<Load> for Fraction {
    type Output = Load;

    fn mul(self, load: Load) -> Load {
        Load(self.get() * load.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CostWeightParams<'a> {
    pub choice: &'a ModelChoice,
    /// The model's own effort levels from its agent's catalog, when known.
    pub ladder: &'a [String],
}

/// What a choice costs per working minute, relative to the others: the model's
/// price tier, scaled by where its effort sits on its ladder.
#[must_use]
pub fn cost_weight(params: CostWeightParams<'_>) -> CostWeight {
    let CostWeightParams { choice, ladder } = params;
    let model = choice.model.as_deref().unwrap_or("");
    let tier_weight = price_ratio(model);
    let place = choice.effort.as_deref().and_then(|effort| {
        effort_step(EffortQuery {
            model,
            effort,
            ladder,
        })
    });
    let Some(place) = place else {
        return CostWeight(tier_weight * MIDDLE_EFFORT_WEIGHT);
    };

    let height = if place.steps > 1 {
        f64::from(place.step - 1) / f64::from(place.steps - 1)
    } else {
        1.0
    };
    CostWeight(
        tier_weight
            * (LOWEST_EFFORT_WEIGHT + height * (HIGHEST_EFFORT_WEIGHT - LOWEST_EFFORT_WEIGHT)),
    )
}

#[derive(Debug, Clone, Copy)]
pub struct SessionLoadParams {
    pub cost: CostWeight,
    /// How much of the recent time it spent working, 0 to 1.
    pub working_share: Fraction,
    /// The subagents it runs right now, each spending on the same account.
    pub subagents: u32,
}

/// A session's estimated share-driver of its account's spend.
#[must_use]
pub fn session_load(params: SessionLoadParams) -> Load {
    let SessionLoadParams {
        cost,
        working_share,
        subagents,
    } = params;
    Load(cost.get() * working_share.get() * (1.0 + f64::from(subagents)))
}

/// A session's load both ways: what its transcripts measured and what the
/// estimate says.
#[derive(Debug, Clone, Copy)]
pub struct LoadPair {
    pub measured: Load,
    pub estimated: Load,
}

/// How much measured load one unit of estimated load stands for, over the
/// sessions both measured and estimated - so a session nothing measured yet joins
/// the measured ones on their scale. 1 while nothing is measured.
#[must_use]
pub fn measured_per_estimated(sessions: &[LoadPair]) -> f64 {
    let measured: Load = sessions.iter().map(|session| session.measured).sum();
    let estimated: Load = sessions.iter().map(|session| session.estimated).sum();
    let is_comparable = measured > Load::ZERO && estimated > Load::ZERO;
    if is_comparable {
        return measured.get() / estimated.get();
    }

    1.0
}

#[derive(Clone, Copy)]
pub struct ShedFractionParams<'a> {
    pub window: &'a UsageWindow,
    pub rate: Option<BurnRate>,
    pub free_at: EpochMilliseconds,
    pub now: EpochMilliseconds,
}

/// The fraction of its spend an account must shed to last until `free_at` -
/// `None` while its burn isn't measured, or when it is already on pace.
#[must_use]
pub fn shed_fraction(params: ShedFractionParams<'_>) -> Option<Fraction> {
    let ShedFractionParams {
        window,
        rate,
        free_at,
        now,
    } = params;
    let time_left = free_at - now;
    let rate = rate.filter(|rate| rate.is_positive())?;
    if time_left.get() <= 0.0 {
        return None;
    }

    let target = (Percentage::FULL - utilization_of(window)) / time_left;
    let fraction = 1.0 - target / rate;
    (fraction > 0.0).then_some(Fraction::new(fraction))
}

/// One session that could take a step, and what that step would save. `Id` says
/// whose step it is.
#[derive(Debug, Clone, PartialEq)]
pub struct ShedCandidate<Id> {
    pub id: Id,
    pub load: Load,
    /// The fraction of its own load its next step saves, 0 to 1.
    pub saving: Fraction,
}

#[derive(Debug, Clone, Copy)]
pub struct PlanSheddingParams<'a, Id> {
    pub candidates: &'a [ShedCandidate<Id>],
    /// Every working session's load on the account, the candidates' included.
    pub total_load: Load,
    /// What the account must shed; unknown (no burn measured yet), only the one
    /// biggest saving steps, the gentlest move that still acts.
    pub fraction: Option<Fraction>,
}

/// Which candidates step this round: biggest share of the account saved first,
/// until the savings cover `fraction` of `total_load`.
#[must_use]
pub fn plan_shedding<Id: Clone>(params: PlanSheddingParams<'_, Id>) -> Vec<Id> {
    let PlanSheddingParams {
        candidates,
        total_load,
        fraction,
    } = params;
    let mut ranked: Vec<(&Id, Load)> = candidates
        .iter()
        .filter(|candidate| candidate.saving > Fraction::NONE)
        .map(|candidate| (&candidate.id, candidate.load * candidate.saving))
        .collect();
    ranked.sort_by(|(_, first), (_, second)| second.partial_cmp(first).unwrap_or(Ordering::Equal));
    let Some(fraction) = fraction else {
        return ranked
            .into_iter()
            .take(1)
            .map(|(id, _)| id.clone())
            .collect();
    };

    let needed = fraction * total_load;
    let mut chosen = Vec::new();
    let mut saved = Load::ZERO;
    for (id, saving) in ranked {
        if saved >= needed {
            break;
        }

        chosen.push(id.clone());
        saved = saved + saving;
    }

    chosen
}

/// A stepped-down session that could step back up, and the load that would add.
#[derive(Debug, Clone, PartialEq)]
pub struct RestoreCandidate<Id> {
    pub id: Id,
    pub load: Load,
}

/// The stepped-down session to step back up first: the one adding the least load
/// back, so the account climbs back as gently as it came down. The first of
/// equals wins.
pub fn lightest_to_restore<Id: Clone>(candidates: &[RestoreCandidate<Id>]) -> Option<Id> {
    let mut lightest: Option<&RestoreCandidate<Id>> = None;
    for candidate in candidates {
        if lightest.is_none_or(|current| candidate.load < current.load) {
            lightest = Some(candidate);
        }
    }

    lightest.map(|candidate| candidate.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacing::test_support::session_window;
    use crate::pacing::units::SpanMilliseconds;

    const MINUTE: SpanMilliseconds = SpanMilliseconds::from_minutes(1.0);
    const NOW: EpochMilliseconds = EpochMilliseconds::new(1_793_361_600_000.0);

    fn weight(model: &str, effort: Option<&str>) -> CostWeight {
        cost_weight(CostWeightParams {
            choice: &ModelChoice {
                model: Some(model.to_string()),
                effort: effort.map(ToString::to_string),
            },
            ladder: &[],
        })
    }

    #[test]
    fn a_dearer_tier_costs_more_than_a_cheaper_one_at_the_same_effort() {
        let opus = weight("claude-opus-5-5", Some("high"));
        let sonnet = weight("claude-sonnet-5-5", Some("high"));
        let haiku = weight("claude-haiku-5-5", Some("high"));
        assert!(opus > sonnet);
        assert!(sonnet > haiku);
    }

    #[test]
    fn the_tiers_price_by_their_long_context_list_price_ratio() {
        let haiku = weight("claude-haiku-5-5", None);
        assert!((weight("claude-sonnet-5-5", None) / haiku - 4.0).abs() < f64::EPSILON);
        assert!((weight("claude-opus-5-5", None) / haiku - 8.0).abs() < f64::EPSILON);
        assert!((weight("claude-fable-5-1", None) / haiku - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn more_effort_costs_more_than_less_on_the_same_model() {
        assert!(weight("claude-opus-5-5", Some("max")) > weight("claude-opus-5-5", Some("low")));
    }

    #[test]
    fn an_unknown_model_and_effort_sit_at_the_middle() {
        assert_eq!(
            weight("mystery-model", None),
            weight("claude-sonnet-5-5", None)
        );
    }

    #[test]
    fn an_estimate_scales_onto_what_the_measured_sessions_burn() {
        let scale = measured_per_estimated(&[
            LoadPair {
                measured: Load::new(3000.0),
                estimated: Load::new(2.0),
            },
            LoadPair {
                measured: Load::new(1000.0),
                estimated: Load::new(2.0),
            },
        ]);
        assert!((scale - 1000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn an_estimate_stays_as_it_is_while_nothing_is_measured() {
        assert!((measured_per_estimated(&[]) - 1.0).abs() < f64::EPSILON);
        let none_comparable = measured_per_estimated(&[LoadPair {
            measured: Load::new(500.0),
            estimated: Load::ZERO,
        }]);
        assert!((none_comparable - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_session_load_grows_with_each_subagent_the_session_runs() {
        let load = |subagents| {
            session_load(SessionLoadParams {
                cost: CostWeight(5.0),
                working_share: Fraction::WHOLE,
                subagents,
            })
        };
        assert_eq!(load(3), Load::new(load(0).get() * 4.0));
    }

    #[test]
    fn a_session_load_weighs_by_how_much_of_the_time_it_works() {
        let load = session_load(SessionLoadParams {
            cost: CostWeight(4.0),
            working_share: Fraction::new(0.25),
            subagents: 0,
        });
        assert_eq!(load, Load::new(1.0));
    }

    fn shed(rate: Option<BurnRate>) -> Option<Fraction> {
        let window = session_window(80.0, SpanMilliseconds::ZERO);
        shed_fraction(ShedFractionParams {
            window: &window,
            rate,
            free_at: NOW + MINUTE * 100.0,
            now: NOW,
        })
    }

    #[test]
    fn a_shed_asks_for_the_share_the_burn_runs_over_the_pace_that_lasts() {
        // 20% left over 100 minutes lasts at 0.2%/min; burning 0.8%/min is 4x over.
        let fraction = shed(Some(BurnRate::new(0.8 / MINUTE.get()))).expect("over pace");
        assert!((fraction.get() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn a_shed_asks_for_nothing_on_pace_or_while_the_burn_is_unmeasured() {
        assert_eq!(shed(Some(BurnRate::new(0.1 / MINUTE.get()))), None);
        assert_eq!(shed(None), None);
    }

    fn candidate(id: &'static str, load: f64, saving: f64) -> ShedCandidate<&'static str> {
        ShedCandidate {
            id,
            load: Load::new(load),
            saving: Fraction::new(saving),
        }
    }

    fn plan(
        candidates: &[ShedCandidate<&'static str>],
        total_load: f64,
        fraction: Option<f64>,
    ) -> Vec<&'static str> {
        plan_shedding(PlanSheddingParams {
            candidates,
            total_load: Load::new(total_load),
            fraction: fraction.map(Fraction::new),
        })
    }

    #[test]
    fn steps_the_biggest_saving_first_and_stops_once_the_shed_is_covered() {
        let all = [
            candidate("sonnet-alone", 3.0, 0.4),
            candidate("opus-with-subagents", 20.0, 0.4),
            candidate("haiku-resting", 1.0, 1.0),
        ];
        assert_eq!(plan(&all, 24.0, Some(0.3)), ["opus-with-subagents"]);
    }

    #[test]
    fn keeps_stepping_down_the_ranking_while_the_shed_isnt_covered() {
        let all = [
            candidate("sonnet-alone", 3.0, 0.4),
            candidate("opus-with-subagents", 20.0, 0.4),
            candidate("haiku-resting", 1.0, 1.0),
        ];
        assert_eq!(
            plan(&all, 24.0, Some(0.35)),
            ["opus-with-subagents", "sonnet-alone"]
        );
    }

    #[test]
    fn steps_only_the_biggest_saving_while_the_shed_is_unknown() {
        let both = [
            candidate("sonnet-alone", 3.0, 0.4),
            candidate("opus-with-subagents", 20.0, 0.4),
        ];
        assert_eq!(plan(&both, 23.0, None), ["opus-with-subagents"]);
    }

    #[test]
    fn never_picks_a_session_with_no_step_left() {
        let none = [candidate("opus-with-subagents", 20.0, 0.0)];
        assert_eq!(plan(&none, 20.0, Some(0.5)), Vec::<&str>::new());
    }

    #[test]
    fn restores_the_lightest_session_first() {
        let heavy_and_light = [
            RestoreCandidate {
                id: "heavy",
                load: Load::new(9.0),
            },
            RestoreCandidate {
                id: "light",
                load: Load::new(2.0),
            },
        ];
        assert_eq!(lightest_to_restore(&heavy_and_light), Some("light"));
    }

    #[test]
    fn has_nothing_to_restore_when_no_session_is_stepped_down() {
        assert_eq!(lightest_to_restore::<&str>(&[]), None);
    }
}
