//! The per-account half of a round: what each account does this round - ease the
//! holds and let the lightest stepped-down session climb back while every session
//! has room, or shed load by stepping the sessions and subagents over pace.

use std::collections::HashMap;

use super::{subagent_key, SessionReport, SubagentKeyParts, SubagentSteps};
use crate::pacing::load_shedding::{
    lightest_to_restore, plan_shedding, Load, PlanSheddingParams, RestoreCandidate, ShedCandidate,
};
use crate::pacing::subagent_steps::{
    eased_subagent_step, is_blocking_spawns, next_subagent_step, NextStepParams, SubagentStep,
};
use crate::pacing::units::{EpochMilliseconds, Fraction};

/// Whose step a shed candidate is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ShedTarget {
    /// A session, by its id.
    Session(String),
    /// A subagent, by its round key.
    Subagent(String),
}

/// Every running subagent of an account's sessions, under its round key.
fn subagents_of(sessions: &[&SessionReport]) -> Vec<(String, Load)> {
    sessions
        .iter()
        .flat_map(|session| {
            session.subagents.iter().map(|subagent| {
                (
                    subagent_key(SubagentKeyParts {
                        session_id: &session.id,
                        subagent_id: &subagent.id,
                    }),
                    subagent.load,
                )
            })
        })
        .collect()
}

/// What the account must shed: the most any of its over-pace sessions asks for -
/// `None` while none of them has a measured burn.
fn account_shed(sessions: &[&SessionReport]) -> Option<Fraction> {
    let mut shed: Option<Fraction> = None;
    for session in sessions {
        let session_shed = session.over_pace.and_then(|over_pace| over_pace.shed);
        let is_larger_shed = session_shed.is_some_and(|asked| shed.is_none_or(|held| asked > held));
        if is_larger_shed {
            shed = session_shed;
        }
    }

    shed
}

/// What the round decided for one account.
pub(super) struct AccountPlan {
    pub(super) climb: Vec<String>,
    pub(super) restore: Vec<String>,
    pub(super) steps: SubagentSteps,
    pub(super) is_blocking_spawns: bool,
    pub(super) has_stepped: bool,
}

#[derive(Clone, Copy)]
pub(super) struct AccountInputs<'a> {
    pub(super) sessions: &'a [&'a SessionReport],
    pub(super) steps: &'a SubagentSteps,
    /// When a round last stepped each session.
    pub(super) stepped_at: &'a HashMap<String, EpochMilliseconds>,
}

/// An account on pace: once every session on it has room, each hold eases and the
/// lightest stepped-down session climbs back.
fn ease_account(inputs: AccountInputs<'_>) -> AccountPlan {
    let AccountInputs {
        sessions, steps, ..
    } = inputs;
    let has_room = sessions.iter().all(|session| session.has_room);
    let mut next_steps = SubagentSteps::new();
    for (key, _) in subagents_of(sessions) {
        let held = steps.get(&key).copied();
        let kept = match held {
            Some(current) if has_room => eased_subagent_step(current),
            Some(_) | None => held,
        };
        if let Some(step) = kept {
            next_steps.insert(key, step);
        }
    }

    let restorable: Vec<RestoreCandidate<String>> = sessions
        .iter()
        .filter(|session| session.can_step_up)
        .map(|session| RestoreCandidate {
            id: session.id.clone(),
            load: session.load,
        })
        .collect();
    let restoring = lightest_to_restore(&restorable);
    AccountPlan {
        climb: Vec::new(),
        has_stepped: restoring.is_some(),
        restore: restoring.into_iter().collect(),
        steps: next_steps,
        is_blocking_spawns: false,
    }
}

/// Every step the account could take this round: each over-pace session not
/// stepped since its report, and each subagent with a step left.
fn shed_candidates(
    inputs: AccountInputs<'_>,
    shed: Option<Fraction>,
) -> (
    Vec<ShedCandidate<ShedTarget>>,
    HashMap<String, SubagentStep>,
) {
    let AccountInputs {
        sessions,
        steps,
        stepped_at,
    } = inputs;
    let mut candidates = Vec::new();
    for session in sessions {
        let stepped_time = stepped_at
            .get(&session.id)
            .copied()
            .unwrap_or(EpochMilliseconds::new(0.0));
        let is_stale_after_step = stepped_time > session.reported_at;
        let Some(over_pace) = session.over_pace else {
            continue;
        };

        if !is_stale_after_step {
            candidates.push(ShedCandidate {
                id: ShedTarget::Session(session.id.clone()),
                load: session.load,
                saving: over_pace.saving,
            });
        }
    }

    let mut moves = HashMap::new();
    for (key, load) in subagents_of(sessions) {
        let next = next_subagent_step(NextStepParams {
            current: steps.get(&key).copied(),
            shed,
        });
        if let Some(next) = next {
            moves.insert(key.clone(), next.step);
            candidates.push(ShedCandidate {
                id: ShedTarget::Subagent(key),
                load,
                saving: next.saving,
            });
        }
    }

    (candidates, moves)
}

/// An account over pace: the steps that save the most of it go first, until they
/// cover its shed.
fn shed_account(inputs: AccountInputs<'_>) -> AccountPlan {
    let AccountInputs {
        sessions, steps, ..
    } = inputs;
    let shed = account_shed(sessions);
    let subagents = subagents_of(sessions);
    let total_load: Load = sessions
        .iter()
        .map(|session| session.load)
        .chain(subagents.iter().map(|(_, load)| *load))
        .sum();
    let (candidates, moves) = shed_candidates(inputs, shed);
    let chosen = plan_shedding(PlanSheddingParams {
        candidates: &candidates,
        total_load,
        fraction: shed,
    });
    let mut next_steps = SubagentSteps::new();
    for (key, _) in &subagents {
        let is_chosen = chosen.contains(&ShedTarget::Subagent(key.clone()));
        let kept = if is_chosen {
            moves.get(key).copied()
        } else {
            steps.get(key).copied()
        };
        if let Some(step) = kept {
            next_steps.insert(key.clone(), step);
        }
    }

    let climb = chosen
        .iter()
        .filter_map(|target| match target {
            ShedTarget::Session(id) => Some(id.clone()),
            ShedTarget::Subagent(_) => None,
        })
        .collect();
    AccountPlan {
        climb,
        restore: Vec::new(),
        steps: next_steps,
        is_blocking_spawns: is_blocking_spawns(shed),
        has_stepped: !chosen.is_empty(),
    }
}

/// One account's sessions and the round's say over them.
pub(super) fn plan_account(inputs: AccountInputs<'_>) -> AccountPlan {
    let is_over_pace = inputs
        .sessions
        .iter()
        .any(|session| session.over_pace.is_some());
    if is_over_pace {
        shed_account(inputs)
    } else {
        ease_account(inputs)
    }
}
