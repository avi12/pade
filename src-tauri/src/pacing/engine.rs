//! The pacing engine's decision core: the port of the decisions in
//! `lib/stores/usage/usagePacing.svelte.ts`, as one pure function over what the
//! windows reported (`EngineInputs`). No I/O, no clock, no threads, no globals:
//! `now` is an input, every formula is the one in `usage_pace`, `load_shedding`,
//! `subagent_steps` and `account_round`, and the answer is a list of
//! [`Decision`]s plus the memory the next pass starts from. In the shadow phase
//! nothing acts on them (`shadow.rs` compares them with what the TypeScript did).
//!
//! One pass is the TypeScript's `scan` run for every window at once:
//!
//! 1. a stepped-down session whose window is about to reset goes back to the
//!    user's own choice (`adapter.restore`);
//! 2. each session worth a look is stood down (pacing off), left alone (the user
//!    skipped it, a model switch is applying, the user's own run, a handoff
//!    successor still reading), advanced (it is paused) or assessed (it works);
//! 3. the working sessions' assessments go to the account-wide round
//!    (`plan_round`), whose plan says which climb a step, which restore one and
//!    how each session's subagents are held; a climb takes the first step with
//!    somewhere to go - effort, a cheaper model, a rest, a stop;
//! 4. of the paused sessions back on pace, one per account is let go, spaced.

mod climb;
mod scan;

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::pacing::account_round::{RoundPlan, SubagentSteps};
use crate::pacing::facts::{AccountKey, AgentId, ChoiceFacts, Decision, SessionFacts, SessionId};
use crate::pacing::load_shedding::{Load, RestoreCandidate};
use crate::pacing::registry::LiveSession;
use crate::pacing::samples::SampleBook;
use crate::pacing::units::{EpochMilliseconds, Fraction, SpanMilliseconds};
use crate::pacing::usage_pace::{BindingWindow, MINIMUM_RATE_SPAN_MS};
use crate::usage::AccountUsage;

/// How long an account waits after letting one paused session go before the
/// next - long enough for its usage to show what the first one spends.
const RELEASE_SPACING: SpanMilliseconds = MINIMUM_RATE_SPAN_MS;

/// The one engine plays the part of the round's leader.
const ROUND_LEADER: &str = "engine";

/// What the engine has read of an agent's account: whose it is, and its windows.
#[derive(Debug, Clone)]
pub struct AccountReading {
    pub key: AccountKey,
    pub usage: Option<AccountUsage>,
}

/// What a pass remembers for the next: the round's subagent steps, when each
/// session was last stepped and when each account last let a paused session go.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngineMemory {
    pub subagent_steps: SubagentSteps,
    pub stepped_at: HashMap<String, EpochMilliseconds>,
    pub last_release_at: HashMap<AccountKey, EpochMilliseconds>,
}

#[derive(Debug, Clone, Copy)]
pub struct EngineInputs<'a> {
    pub now: EpochMilliseconds,
    /// Every session of every fresh report.
    pub sessions: &'a [LiveSession],
    /// The account reading of each agent the sessions run.
    pub accounts: &'a BTreeMap<AgentId, AccountReading>,
    pub samples: &'a SampleBook,
    pub memory: &'a EngineMemory,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineOutcome {
    pub decisions: Vec<Decision>,
    pub memory: EngineMemory,
    /// The accounts a step went to over: their burn samples start again.
    pub stepped_accounts: Vec<AccountKey>,
}

/// An account as a session sees it: its key, and its windows while read.
struct AccountView<'a> {
    key: AccountKey,
    usage: Option<&'a AccountUsage>,
}

fn account_of<'a>(inputs: &'a EngineInputs<'_>, agent: &AgentId) -> AccountView<'a> {
    match inputs.accounts.get(agent) {
        Some(reading) => AccountView {
            key: reading.key.clone(),
            usage: reading.usage.as_ref(),
        },
        None => AccountView {
            key: AccountKey::new(agent.as_str()),
            usage: None,
        },
    }
}

/// Whether pacing doesn't touch the session this scan: skipped by the user, mid
/// model switch, the user's own run, or a handoff successor still reading.
const fn is_left_alone(session: &SessionFacts) -> bool {
    session.is_skipped
        || session.is_model_switch_applying
        || session.is_continued_run
        || session.is_shielded
}

/// Whether pacing owns nothing of the session: the usage-limit flow or the
/// auto-handoff does.
const fn is_owned_elsewhere(session: &SessionFacts) -> bool {
    session.has_usage_limit || session.is_handing_off
}

/// Whether pacing has the session stepped down to a cheaper choice.
fn is_stepped_down(session: &SessionFacts) -> bool {
    session
        .choice
        .as_ref()
        .is_some_and(|choice| choice.can_step_up)
}

/// The accounts that take a burn reading this pass - one reading per account,
/// and only while one of its sessions works with pacing on: the burn rate
/// measures work, not idle time. The wiring records them before it asks `decide`.
#[must_use]
pub fn accounts_to_sample(
    sessions: &[LiveSession],
    accounts: &BTreeMap<AgentId, AccountReading>,
) -> Vec<AccountKey> {
    let mut sampled: Vec<AccountKey> = Vec::new();
    for live in sessions {
        let session = &live.facts;
        let Some(reading) = accounts.get(&session.agent_id) else {
            continue;
        };

        let is_sampled_already = sampled.contains(&reading.key);
        let should_sample = reading.usage.is_some()
            && session.is_pacing_on
            && session.is_working
            && !is_sampled_already;
        if should_sample {
            sampled.push(reading.key.clone());
        }
    }

    sampled
}

/// A working session judged against its account: held by a window (its
/// binding), or on pace with room to climb back up.
struct Assessment<'a> {
    live: &'a LiveSession,
    account_key: AccountKey,
    binding: Option<BindingWindow>,
    has_room: bool,
    can_step_up: bool,
}

/// What one step of a climb does once it reaches the rest and the stop.
enum PauseStep {
    /// The session rests its rhythm's stretch of work before it rests again.
    BetweenRests,
    Take(Decision),
    /// Not this step's to take - on to the next.
    Pass,
}

struct Pass<'a> {
    inputs: &'a EngineInputs<'a>,
    decisions: Vec<Decision>,
    assessments: Vec<Assessment<'a>>,
    releasable: BTreeMap<AccountKey, Vec<RestoreCandidate<SessionId>>>,
    restored: HashSet<&'a SessionId>,
}

/// Decide what pacing would do now.
#[must_use]
pub fn decide<'a>(inputs: &'a EngineInputs<'a>) -> EngineOutcome {
    let mut pass = Pass {
        inputs,
        decisions: Vec::new(),
        assessments: Vec::new(),
        releasable: BTreeMap::new(),
        restored: HashSet::new(),
    };
    for live in inputs.sessions {
        pass.look_at(live);
    }

    let round = pass.round();
    let mut memory = EngineMemory {
        subagent_steps: round.subagent_steps,
        stepped_at: inputs.memory.stepped_at.clone(),
        last_release_at: inputs.memory.last_release_at.clone(),
    };
    for id in &round.plan.climb {
        memory.stepped_at.insert(id.clone(), inputs.now);
    }

    let live_ids: HashSet<&str> = inputs
        .sessions
        .iter()
        .map(|live| live.facts.id.as_str())
        .collect();
    memory
        .stepped_at
        .retain(|id, _| live_ids.contains(id.as_str()));
    pass.release_paused(&mut memory);
    EngineOutcome {
        decisions: pass.decisions,
        memory,
        stepped_accounts: round
            .plan
            .stepped_accounts
            .iter()
            .map(AccountKey::new)
            .collect(),
    }
}

/// What the round settled: its plan, and the subagent steps it keeps.
struct Round {
    plan: RoundPlan,
    subagent_steps: SubagentSteps,
}

/// Everything a session spends: its own share and its running subagents'.
fn total_load(session: &SessionFacts) -> Load {
    session.own_load
        + session
            .subagents
            .iter()
            .map(|subagent| subagent.load)
            .sum::<Load>()
}

/// What the session's next step would save, as a fraction of its own load -
/// the same first step a climb takes: an effort or model step priced by its
/// cost weight, a rest or stop saving all of it while it lasts.
fn next_step_saving(choice: Option<&ChoiceFacts>) -> Fraction {
    match choice {
        Some(choice) if choice.can_step_down.effort => choice.effort_saving,
        Some(choice) if choice.can_step_down.model => choice.model_saving,
        Some(_) | None => Fraction::WHOLE,
    }
}

#[cfg(test)]
mod tests;
