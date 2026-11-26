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
//! account must shed what its binding window doesn't let it keep:
//! `shed = 1 - keep` (`usage_pace`).
//!
//! Every candidate session offers its next step and what that step would save (a
//! fraction of its own load). The steps are taken in order of the share of the
//! account they save, weighted by their session's fan-out (1 + the subagents it
//! runs, so a session fanning work out is slowed before a lone one) - the
//! biggest first - until together they cover the shed. A session takes at most
//! one step per round.

use std::cmp::Ordering;
use std::iter::Sum;
use std::ops::{Add, Mul};

use serde::{Deserialize, Serialize};

use crate::agents::running_model::ModelChoice;
use crate::pacing::model_label::{effort_step, EffortQuery};
use crate::pacing::token_price::price_ratio;
use crate::pacing::units::Fraction;

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
pub fn cost_weight(parameters: CostWeightParams<'_>) -> CostWeight {
    let CostWeightParams { choice, ladder } = parameters;
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

    let has_several_steps = place.steps > 1;
    let height = if has_several_steps {
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
pub fn session_load(parameters: SessionLoadParams) -> Load {
    let SessionLoadParams {
        cost,
        working_share,
        subagents,
    } = parameters;
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

/// The fraction of its spend an account must shed: what its binding window
/// doesn't let it keep (`usage_pace`), at most all of it - `None` when it is
/// already on pace.
#[must_use]
pub fn shed_fraction(keep: Fraction) -> Option<Fraction> {
    let fraction = 1.0 - keep.get();
    (fraction > 0.0).then(|| Fraction::new(fraction.min(1.0)))
}

/// One session that could take a step, and what that step would save. `Id` says
/// whose step it is.
#[derive(Debug, Clone, PartialEq)]
pub struct ShedCandidate<Id> {
    pub id: Id,
    pub load: Load,
    /// The fraction of its own load its next step saves, 0 to 1.
    pub saving: Fraction,
    /// Its session's fan-out: 1 plus the subagents that session runs.
    pub fan_out: u32,
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

/// A candidate as `plan_shedding` ranks it: what its step saves of the account,
/// and that saving weighted by its fan-out.
struct RankedCandidate<'a, Id> {
    id: &'a Id,
    saved: Load,
    priority: f64,
}

/// Which candidates step this round: the biggest share of the account saved,
/// weighted by fan-out, first, until the savings - unweighted - cover
/// `fraction` of `total_load`.
#[must_use]
pub fn plan_shedding<Id: Clone>(parameters: PlanSheddingParams<'_, Id>) -> Vec<Id> {
    let PlanSheddingParams {
        candidates,
        total_load,
        fraction,
    } = parameters;
    let mut ranked: Vec<RankedCandidate<'_, Id>> = candidates
        .iter()
        .filter(|candidate| candidate.saving > Fraction::NONE)
        .map(|candidate| {
            let saved = candidate.load * candidate.saving;
            RankedCandidate {
                id: &candidate.id,
                saved,
                priority: saved.get() * f64::from(candidate.fan_out),
            }
        })
        .collect();
    ranked.sort_by(|first, second| {
        second
            .priority
            .partial_cmp(&first.priority)
            .unwrap_or(Ordering::Equal)
    });
    let Some(fraction) = fraction else {
        return ranked
            .into_iter()
            .take(1)
            .map(|candidate| candidate.id.clone())
            .collect();
    };

    let needed = fraction * total_load;
    let mut chosen = Vec::new();
    let mut saved = Load::ZERO;
    for candidate in ranked {
        let has_saved_enough = saved >= needed;
        if has_saved_enough {
            break;
        }

        chosen.push(candidate.id.clone());
        saved = saved + candidate.saved;
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
    use crate::pacing::load_shedding::*;

    const HEAVY_LOAD: f64 = 3000.0;
    const MEDIUM_LOAD: f64 = 1000.0;
    const LIGHT_LOAD: f64 = 500.0;
    const HIGH_COST: f64 = 5.0;
    const LOWER_COST: f64 = 4.0;
    const QUARTER_SHARE: f64 = 0.25;
    const WIDE_FAN_OUT: u32 = 4;
    const SUBAGENT_SESSION_LOAD: f64 = 12.0;
    const MODEL_SAVING: f64 = 0.4;
    const OPUS_LOAD: f64 = 20.0;
    const SONNET_LOAD: f64 = 3.0;
    const LONE_LOAD: f64 = 9.0;

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
                measured: Load::new(HEAVY_LOAD),
                estimated: Load::new(2.0),
            },
            LoadPair {
                measured: Load::new(MEDIUM_LOAD),
                estimated: Load::new(2.0),
            },
        ]);
        assert!((scale - 1000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn an_estimate_stays_as_it_is_while_nothing_is_measured() {
        assert!((measured_per_estimated(&[]) - 1.0).abs() < f64::EPSILON);
        let none_comparable = measured_per_estimated(&[LoadPair {
            measured: Load::new(LIGHT_LOAD),
            estimated: Load::ZERO,
        }]);
        assert!((none_comparable - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_session_load_grows_with_each_subagent_the_session_runs() {
        let load = |subagents| {
            session_load(SessionLoadParams {
                cost: CostWeight(HIGH_COST),
                working_share: Fraction::WHOLE,
                subagents,
            })
        };
        assert_eq!(load(3), Load::new(load(0).get() * 4.0));
    }

    #[test]
    fn a_session_load_weighs_by_how_much_of_the_time_it_works() {
        let load = session_load(SessionLoadParams {
            cost: CostWeight(LOWER_COST),
            working_share: Fraction::new(QUARTER_SHARE),
            subagents: 0,
        });
        assert_eq!(load, Load::new(1.0));
    }

    #[test]
    fn a_shed_asks_for_what_the_account_may_not_keep() {
        let fraction = shed_fraction(Fraction::new(QUARTER_SHARE)).expect("over pace");
        assert!((fraction.get() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn a_shed_asks_for_at_most_everything_and_nothing_on_pace() {
        assert_eq!(shed_fraction(Fraction::new(-0.5)), Some(Fraction::WHOLE));
        assert_eq!(shed_fraction(Fraction::WHOLE), None);
        assert_eq!(shed_fraction(Fraction::new(1.7)), None);
    }

    fn candidate(id: &'static str, load: f64, saving: f64) -> ShedCandidate<&'static str> {
        ShedCandidate {
            id,
            load: Load::new(load),
            saving: Fraction::new(saving),
            fan_out: 1,
        }
    }

    fn fanned_out() -> ShedCandidate<&'static str> {
        ShedCandidate {
            fan_out: WIDE_FAN_OUT,
            ..candidate(
                "opus-with-three-subagents",
                SUBAGENT_SESSION_LOAD,
                MODEL_SAVING,
            )
        }
    }

    #[test]
    fn slows_a_session_fanning_work_out_before_a_lone_one_spending_as_much() {
        let both = [
            candidate("opus-alone", OPUS_LOAD, MODEL_SAVING),
            fanned_out(),
        ];
        assert_eq!(plan(&both, 32.0, Some(0.1)), ["opus-with-three-subagents"]);
    }

    #[test]
    fn still_slows_the_lone_session_when_the_fanned_out_one_cant_cover_the_shed() {
        let both = [
            candidate("opus-alone", OPUS_LOAD, MODEL_SAVING),
            fanned_out(),
        ];
        assert_eq!(
            plan(&both, 32.0, Some(0.3)),
            ["opus-with-three-subagents", "opus-alone"]
        );
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
            candidate("sonnet-alone", SONNET_LOAD, MODEL_SAVING),
            candidate("opus-with-subagents", OPUS_LOAD, MODEL_SAVING),
            candidate("haiku-resting", 1.0, 1.0),
        ];
        assert_eq!(plan(&all, 24.0, Some(0.3)), ["opus-with-subagents"]);
    }

    #[test]
    fn keeps_stepping_down_the_ranking_while_the_shed_isnt_covered() {
        let all = [
            candidate("sonnet-alone", SONNET_LOAD, MODEL_SAVING),
            candidate("opus-with-subagents", OPUS_LOAD, MODEL_SAVING),
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
            candidate("sonnet-alone", SONNET_LOAD, MODEL_SAVING),
            candidate("opus-with-subagents", OPUS_LOAD, MODEL_SAVING),
        ];
        assert_eq!(plan(&both, 23.0, None), ["opus-with-subagents"]);
    }

    #[test]
    fn never_picks_a_session_with_no_step_left() {
        let none = [candidate("opus-with-subagents", OPUS_LOAD, 0.0)];
        assert_eq!(plan(&none, 20.0, Some(0.5)), Vec::<&str>::new());
    }

    #[test]
    fn restores_the_lightest_session_first() {
        let heavy_and_light = [
            RestoreCandidate {
                id: "heavy",
                load: Load::new(LONE_LOAD),
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
