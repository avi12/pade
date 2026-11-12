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

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::pacing::account_round::{
    is_throttling, plan_round, OverPace, PlanRoundParams, RoundPlan, SessionReport, SubagentSteps,
    WindowReport,
};
use crate::pacing::facts::{
    AccountKey, AgentId, ChoiceFacts, Decision, PaceFacts, PacePhase, SessionFacts, SessionId,
};
use crate::pacing::load_shedding::{
    lightest_to_restore, shed_fraction, Load, RestoreCandidate, ShedFractionParams,
};
use crate::pacing::policy::{pacing_margin, PacingStep};
use crate::pacing::registry::LiveSession;
use crate::pacing::samples::SampleBook;
use crate::pacing::units::{EpochMilliseconds, Fraction, SpanMilliseconds};
use crate::pacing::usage_pace::{
    assess_pace, has_room_to_spare, is_rest_enough, slow_rest_until, work_rhythm, working_runway,
    AssessPaceParams, BindingWindow, HasRoomParams, PacedWindowKind, RestEnoughParams,
    SlowRestParams, WorkRhythmParams, WorkingRunwayParams, MINIMUM_RATE_SPAN_MS,
};
use crate::usage::{AccountUsage, UsageWindow};

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

impl<'a> Pass<'a> {
    fn look_at(&mut self, live: &'a LiveSession) {
        let session = &live.facts;
        if self.is_choice_due(session) {
            self.decisions.push(Decision::RestoreChoice {
                session: session.id.clone(),
            });
            self.restored.insert(&session.id);
        }

        let is_stepped = is_stepped_down(session) && !self.restored.contains(&session.id);
        let is_worth_a_look = session.pace.is_some() || session.is_working || is_stepped;
        if !is_worth_a_look {
            return;
        }

        if !session.is_pacing_on {
            self.stand_down(session);
            return;
        }

        self.pace_session(live);
    }

    /// A stepped-down session whose window is about to reset goes back to the
    /// user's own choice.
    fn is_choice_due(&self, session: &SessionFacts) -> bool {
        let restore_at = session
            .choice
            .as_ref()
            .filter(|choice| choice.can_step_up)
            .and_then(|choice| choice.restore_at);
        restore_at.is_some_and(|restore_at| self.inputs.now >= restore_at)
    }

    /// Pacing turned off for the session's project: a paused one is let be, a
    /// stepped-down one put back on its own choice.
    fn stand_down(&mut self, session: &'a SessionFacts) {
        if session.pace.is_some() {
            self.decisions.push(Decision::Resume {
                session: session.id.clone(),
            });
        }

        if is_stepped_down(session) && !self.restored.contains(&session.id) {
            self.decisions.push(Decision::RestoreChoice {
                session: session.id.clone(),
            });
        }
    }

    fn pace_session(&mut self, live: &'a LiveSession) {
        let session = &live.facts;
        if is_owned_elsewhere(session) {
            if session.pace.is_some() {
                self.decisions.push(Decision::Release {
                    session: session.id.clone(),
                });
            }

            return;
        }

        if is_left_alone(session) {
            return;
        }

        match &session.pace {
            Some(pace) => self.advance_paced(live, pace),
            None if session.is_working => self.assess_working(live),
            None => {}
        }
    }

    /// A paused session: continue it at the end of its rest or its window, or
    /// leave it held while it is still over pace. An unreadable account leaves
    /// it be.
    fn advance_paced(&mut self, live: &'a LiveSession, pace: &PaceFacts) {
        let session = &live.facts;
        let now = self.inputs.now;
        let is_rest_pending = pace.phase == PacePhase::Pausing && pace.rest_until.is_some();
        let is_rest_over = !is_rest_pending && pace.rest_until.is_some_and(|until| now >= until);
        let is_still_stopping = pace.phase == PacePhase::Pausing && session.is_working;
        let is_due = now >= pace.continue_at || (is_rest_over && !is_still_stopping);
        if is_due {
            self.continue_at_reset(session, pace);
            return;
        }

        let account = account_of(self.inputs, &session.agent_id);
        let Some(usage) = account.usage else {
            return;
        };

        let margin_of = |kind: PacedWindowKind| Some(pacing_margin(kind));
        let rate_of = |window: &UsageWindow| {
            pace.rates
                .get(&window.key)
                .copied()
                .or_else(|| self.inputs.samples.rate_of(&account.key, window))
        };
        let binding = assess_pace(AssessPaceParams {
            account: Some(usage),
            now,
            is_paused: true,
            margin_of: &margin_of,
            rate_of: &rate_of,
        });
        let is_still_over_pace = binding.is_some();
        if is_still_over_pace || is_still_stopping {
            return;
        }

        if pace.phase == PacePhase::Overridden {
            self.decisions.push(Decision::Release {
                session: session.id.clone(),
            });
            return;
        }

        self.releasable
            .entry(account.key)
            .or_default()
            .push(RestoreCandidate {
                id: session.id.clone(),
                load: total_load(session),
            });
    }

    /// As its window is about to reset a paused session continues on its own -
    /// unless the user took it over, which ends the pause.
    fn continue_at_reset(&mut self, session: &SessionFacts, pace: &PaceFacts) {
        let decision = if pace.phase == PacePhase::Overridden {
            Decision::Release {
                session: session.id.clone(),
            }
        } else {
            Decision::Resume {
                session: session.id.clone(),
            }
        };
        self.decisions.push(decision);
    }

    /// A working session not yet paused, judged against its account.
    fn assess_working(&mut self, live: &'a LiveSession) {
        let session = &live.facts;
        let now = self.inputs.now;
        let account = account_of(self.inputs, &session.agent_id);
        let margin_of = |kind: PacedWindowKind| Some(pacing_margin(kind));
        let rate_of = |window: &UsageWindow| self.inputs.samples.rate_of(&account.key, window);
        let binding = assess_pace(AssessPaceParams {
            account: account.usage,
            now,
            is_paused: false,
            margin_of: &margin_of,
            rate_of: &rate_of,
        });
        let has_room = binding.is_none()
            && has_room_to_spare(HasRoomParams {
                account: account.usage,
                now,
                margin_of: &margin_of,
                rate_of: &rate_of,
            });
        let can_step_up =
            has_room && is_stepped_down(session) && !self.restored.contains(&session.id);
        self.assessments.push(Assessment {
            live,
            account_key: account.key,
            binding,
            has_room,
            can_step_up,
        });
    }

    /// Hand every assessment to the account-wide round and take what it says:
    /// the sessions it steps take their next step, the ones it restores climb
    /// back, and every session's subagents are held as it plans.
    fn round(&mut self) -> Round {
        let now = self.inputs.now;
        let report = WindowReport {
            label: ROUND_LEADER.to_string(),
            at: now,
            sessions: self
                .assessments
                .iter()
                .map(|each| self.report_of(each))
                .collect(),
        };
        let outcome = plan_round(PlanRoundParams {
            reports: &[&report],
            steps: &self.inputs.memory.subagent_steps,
            stepped_at: &self.inputs.memory.stepped_at,
            leader: ROUND_LEADER,
            now,
        });
        for id in &outcome.plan.climb {
            self.climb(id);
        }

        for id in &outcome.plan.restore {
            self.restore(id);
        }

        for throttle in outcome
            .plan
            .throttles
            .iter()
            .filter(|each| is_throttling(each))
        {
            self.decisions.push(Decision::Throttle {
                session: SessionId::new(&throttle.session_id),
                throttle: throttle.clone(),
            });
        }

        Round {
            plan: outcome.plan,
            subagent_steps: outcome.steps,
        }
    }

    /// A working session as the round sees it: its own share of the account (its
    /// running subagents apart, each with its own), and - over pace - what the
    /// account must shed and what its next step would save.
    fn report_of(&self, assessment: &Assessment<'_>) -> SessionReport {
        let live = assessment.live;
        let session = &live.facts;
        let over_pace = assessment.binding.as_ref().map(|binding| OverPace {
            shed: shed_fraction(ShedFractionParams {
                window: &binding.window,
                rate: Some(binding.rate),
                free_at: binding.continue_at,
                now: self.inputs.now,
            }),
            saving: next_step_saving(session.choice.as_ref()),
        });
        SessionReport {
            id: session.id.as_str().to_string(),
            name: session.name.clone(),
            account_key: assessment.account_key.as_str().to_string(),
            command: session.command.clone(),
            conversation_id: session.conversation_id.clone(),
            reported_at: live.reported_at,
            load: session.own_load,
            over_pace,
            can_step_up: assessment.can_step_up,
            has_room: assessment.has_room,
            subagents: session.subagents.clone(),
        }
    }

    fn assessment_of(&self, id: &str) -> Option<&Assessment<'a>> {
        self.assessments
            .iter()
            .find(|assessment| assessment.live.facts.id.as_str() == id)
    }

    /// Take a session's next allowed step: effort and model step down a notch
    /// at a time; resting gives way to a stop once it would need more rest than
    /// work - or stays, when a stop isn't allowed.
    fn climb(&mut self, id: &str) {
        if let Some(decision) = self.climb_decision(id) {
            self.decisions.push(decision);
        }
    }

    fn climb_decision(&self, id: &str) -> Option<Decision> {
        let assessment = self.assessment_of(id)?;
        let binding = assessment.binding.as_ref()?;
        let session = &assessment.live.facts;
        let can_step_down = session.choice.as_ref().map(|choice| choice.can_step_down);
        for step in PacingStep::ORDER {
            let taken =
                match step {
                    PacingStep::Effort => can_step_down.is_some_and(|can| can.effort).then(|| {
                        Decision::StepEffortDown {
                            session: session.id.clone(),
                        }
                    }),
                    PacingStep::Model => can_step_down.is_some_and(|can| can.model).then(|| {
                        Decision::StepModelDown {
                            session: session.id.clone(),
                        }
                    }),
                    PacingStep::Rest | PacingStep::Stop => match self.pause_step(PauseStepParams {
                        step,
                        session,
                        binding,
                    }) {
                        PauseStep::BetweenRests => return None,
                        PauseStep::Take(decision) => Some(decision),
                        PauseStep::Pass => None,
                    },
                };
            if taken.is_some() {
                return taken;
            }
        }

        None
    }

    /// The rest or the stop of a climb. A rest right after the last one ended
    /// would buy a "continue" only to cut off the first thing it started - so
    /// the session works its rhythm's stretch before it rests again.
    fn pause_step(&self, params: PauseStepParams<'_>) -> PauseStep {
        let PauseStepParams {
            step,
            session,
            binding,
        } = params;
        let now = self.inputs.now;
        let runway = working_runway(WorkingRunwayParams {
            window: &binding.window,
            rate: binding.rate,
            ready_at: binding.ready_at,
            continue_at: binding.continue_at,
        });
        let is_weekly_window = binding.kind == PacedWindowKind::Weekly;
        let is_rest_still_enough = is_rest_enough(RestEnoughParams {
            runway,
            continue_at: binding.continue_at,
            now,
        });
        let is_resting = step == PacingStep::Rest && (is_weekly_window || is_rest_still_enough);
        let worked_for = now - session.working_since.unwrap_or(now);
        let stretch = work_rhythm(WorkRhythmParams {
            runway,
            time_left: binding.continue_at - now,
        })
        .stretch;
        let is_between_rests = is_resting && worked_for < stretch;
        if is_between_rests {
            return PauseStep::BetweenRests;
        }

        if is_resting {
            return PauseStep::Take(Decision::Rest {
                session: session.id.clone(),
                until: slow_rest_until(SlowRestParams {
                    runway,
                    ready_at: binding.ready_at,
                    continue_at: binding.continue_at,
                    now,
                }),
            });
        }

        match step {
            PacingStep::Stop => PauseStep::Take(Decision::Stop {
                session: session.id.clone(),
            }),
            PacingStep::Effort | PacingStep::Model | PacingStep::Rest => PauseStep::Pass,
        }
    }

    /// A session the round restores climbs one notch back up.
    fn restore(&mut self, id: &str) {
        let stepped = self
            .assessment_of(id)
            .map(|assessment| &assessment.live.facts)
            .filter(|session| is_stepped_down(session))
            .map(|session| session.id.clone());
        if let Some(session) = stepped {
            self.decisions.push(Decision::StepUp { session });
        }
    }

    /// The paused sessions back on pace continue one per account at a time, the
    /// lightest first, each `RELEASE_SPACING` after the last - so the account's
    /// usage shows what that one spends before the next is let go.
    fn release_paused(&mut self, memory: &mut EngineMemory) {
        let now = self.inputs.now;
        for (account_key, candidates) in std::mem::take(&mut self.releasable) {
            let last_release = memory
                .last_release_at
                .get(&account_key)
                .copied()
                .unwrap_or(EpochMilliseconds::new(0.0));
            let is_spaced = now - last_release >= RELEASE_SPACING;
            let lightest = is_spaced
                .then(|| lightest_to_restore(&candidates))
                .flatten();
            if let Some(session) = lightest {
                memory.last_release_at.insert(account_key, now);
                self.decisions.push(Decision::Resume { session });
            }
        }
    }
}

#[derive(Clone, Copy)]
struct PauseStepParams<'a> {
    step: PacingStep,
    session: &'a SessionFacts,
    binding: &'a BindingWindow,
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
