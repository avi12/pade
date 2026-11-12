//! The account-wide half of one pacing pass: hand every assessment to the round,
//! take the steps it plans (effort, model, rest, stop), restore what it frees
//! and let paused sessions go, spaced.

use super::{
    is_stepped_down, next_step_saving, Assessment, EngineMemory, Pass, PauseStep, Round,
    RELEASE_SPACING, ROUND_LEADER,
};
use crate::pacing::account_round::{
    is_throttling, plan_round, OverPace, PlanRoundParams, SessionReport, WindowReport,
};
use crate::pacing::facts::{Decision, SessionFacts, SessionId};
use crate::pacing::load_shedding::{lightest_to_restore, shed_fraction, ShedFractionParams};
use crate::pacing::policy::PacingStep;
use crate::pacing::units::EpochMilliseconds;
use crate::pacing::usage_pace::{
    is_rest_enough, slow_rest_until, work_rhythm, working_runway, BindingWindow, PacedWindowKind,
    RestEnoughParams, SlowRestParams, WorkRhythmParams, WorkingRunwayParams,
};

#[derive(Clone, Copy)]
struct PauseStepParams<'a> {
    step: PacingStep,
    session: &'a SessionFacts,
    binding: &'a BindingWindow,
}

impl<'a> Pass<'a> {
    /// Hand every assessment to the account-wide round and take what it says:
    /// the sessions it steps take their next step, the ones it restores climb
    /// back, and every session's subagents are held as it plans.
    pub(super) fn round(&mut self) -> Round {
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
    pub(super) fn report_of(&self, assessment: &Assessment<'_>) -> SessionReport {
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

    pub(super) fn assessment_of(&self, id: &str) -> Option<&Assessment<'a>> {
        self.assessments
            .iter()
            .find(|assessment| assessment.live.facts.id.as_str() == id)
    }

    /// Take a session's next allowed step: effort and model step down a notch
    /// at a time; resting gives way to a stop once it would need more rest than
    /// work - or stays, when a stop isn't allowed.
    pub(super) fn climb(&mut self, id: &str) {
        if let Some(decision) = self.climb_decision(id) {
            self.decisions.push(decision);
        }
    }

    pub(super) fn climb_decision(&self, id: &str) -> Option<Decision> {
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
    pub(super) fn restore(&mut self, id: &str) {
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
    pub(super) fn release_paused(&mut self, memory: &mut EngineMemory) {
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
