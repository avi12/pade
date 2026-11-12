//! The per-session half of one pacing pass: each live session is stood down,
//! left alone, advanced while paused, or assessed while it works.

use super::{
    account_of, is_left_alone, is_owned_elsewhere, is_stepped_down, total_load, Assessment, Pass,
};
use crate::pacing::facts::{Decision, PaceFacts, PacePhase, SessionFacts};
use crate::pacing::load_shedding::RestoreCandidate;
use crate::pacing::policy::pacing_margin;
use crate::pacing::registry::LiveSession;
use crate::pacing::usage_pace::{
    assess_pace, has_room_to_spare, AssessPaceParams, HasRoomParams, PacedWindowKind,
};
use crate::usage::UsageWindow;

impl<'a> Pass<'a> {
    pub(super) fn look_at(&mut self, live: &'a LiveSession) {
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
    pub(super) fn is_choice_due(&self, session: &SessionFacts) -> bool {
        let restore_at = session
            .choice
            .as_ref()
            .filter(|choice| choice.can_step_up)
            .and_then(|choice| choice.restore_at);
        restore_at.is_some_and(|restore_at| self.inputs.now >= restore_at)
    }

    /// Pacing turned off for the session's project: a paused one is let be, a
    /// stepped-down one put back on its own choice.
    pub(super) fn stand_down(&mut self, session: &'a SessionFacts) {
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

    pub(super) fn pace_session(&mut self, live: &'a LiveSession) {
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
    pub(super) fn advance_paced(&mut self, live: &'a LiveSession, pace: &PaceFacts) {
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
    pub(super) fn continue_at_reset(&mut self, session: &SessionFacts, pace: &PaceFacts) {
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
    pub(super) fn assess_working(&mut self, live: &'a LiveSession) {
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
}
