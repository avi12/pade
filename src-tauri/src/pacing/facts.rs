//! The wire types of the pacing engine's shadow phase (`docs/pacing-engine.md`):
//! what each window reports about its sessions (`PacingReport`), what the
//! TypeScript actually decided during the scan (`ObservedDecision`), and what the
//! engine would decide (`Decision`). Types only - no rule lives here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::pacing::account_round::{SubagentReport, ThrottledSession};
use crate::pacing::load_shedding::Load;
use crate::pacing::units::{BurnRate, EpochMilliseconds, Fraction};
use crate::pacing::usage_pace::PacedWindowKind;

/// A string identity of its own, so a session id, an agent id and a window label
/// can never be handed to one another's parameter.
macro_rules! identifier {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn new(identity: impl Into<String>) -> Self {
                Self(identity.into())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

identifier!(
    /// A PADE session's id.
    SessionId
);
identifier!(
    /// An agent's registry id (`claude`, `codex`): the key of its usage account.
    AgentId
);
identifier!(
    /// The label of the window that sent a report.
    WindowLabel
);
identifier!(
    /// The billing account a session spends from: the usage reading's account
    /// identity, or the agent id while that is unknown (the TypeScript's own key).
    AccountKey
);

/// What a session runs, or ran before pacing stepped it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChoiceFacts {
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// Which of the two choice steps still has somewhere to go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanStepDown {
    pub effort: bool,
    pub model: bool,
}

/// A session's model and effort, and what stepping them would do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceFacts {
    pub model: Option<String>,
    pub effort: Option<String>,
    /// What the user chose, while pacing has the session stepped down.
    pub original: Option<ModelChoiceFacts>,
    pub can_step_down: CanStepDown,
    /// Pacing has stepped the session down: there is a step to undo.
    pub can_step_up: bool,
    /// When the stepped-down choice goes back to the user's own.
    pub restore_at: Option<EpochMilliseconds>,
    /// The share of its own load the next effort step saves.
    pub effort_saving: Fraction,
    /// The share of its own load the next model step saves.
    pub model_saving: Fraction,
}

/// Where a paced session is: asked to stop and still finishing its step,
/// stopped and waiting for room, or restarted by the user while over pace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PacePhase {
    Pausing,
    Paused,
    Overridden,
}

/// The pause the TypeScript holds on a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaceFacts {
    pub phase: PacePhase,
    pub window_kind: PacedWindowKind,
    pub ready_at: EpochMilliseconds,
    pub continue_at: EpochMilliseconds,
    pub rest_until: Option<EpochMilliseconds>,
    pub has_stopped_work: bool,
    pub is_held: bool,
    pub paused_at: EpochMilliseconds,
    /// Each holding window's burn rate when it last held the session, by window
    /// key - a paused session burns nothing, so it is judged by the pace it
    /// was working at.
    #[serde(default)]
    pub rates: BTreeMap<String, BurnRate>,
}

/// One session, as its window sees it this scan. The switches are the window's
/// own judgement and arrive as facts: only a window can see its screens.
// A mirror of the TypeScript `SessionFacts`: each flag is one fact the window
// reports, and folding them into a bit set would hide them from the wire.
#[expect(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFacts {
    pub id: SessionId,
    pub conversation_id: Option<String>,
    pub agent_id: AgentId,
    pub command: String,
    pub name: String,
    pub is_working: bool,
    pub background_agent_count: u32,
    pub is_pacing_on: bool,
    pub is_handing_off: bool,
    pub has_usage_limit: bool,
    pub is_skipped: bool,
    pub is_continued_run: bool,
    pub is_shielded: bool,
    pub is_model_switch_applying: bool,
    pub working_since: Option<EpochMilliseconds>,
    pub working_share: Fraction,
    pub own_load: Load,
    pub subagents: Vec<SubagentReport>,
    pub choice: Option<ChoiceFacts>,
    pub pace: Option<PaceFacts>,
}

/// What the TypeScript decided about a session, by the kind of decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObservedKind {
    StepEffortDown,
    StepModelDown,
    StepUp,
    Rest,
    Stop,
    Resume,
    Release,
    /// Looked at, nothing decided.
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedDecision {
    pub session_id: SessionId,
    pub at: EpochMilliseconds,
    pub kind: ObservedKind,
}

/// One window's report of one scan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PacingReport {
    pub window_label: WindowLabel,
    pub at: EpochMilliseconds,
    pub sessions: Vec<SessionFacts>,
    pub observed: Vec<ObservedDecision>,
}

/// What the engine would do. Shadow phase: nothing acts on it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Decision {
    StepEffortDown {
        session: SessionId,
    },
    StepModelDown {
        session: SessionId,
    },
    StepUp {
        session: SessionId,
    },
    Rest {
        session: SessionId,
        until: EpochMilliseconds,
    },
    Stop {
        session: SessionId,
    },
    Resume {
        session: SessionId,
    },
    Release {
        session: SessionId,
    },
    /// The round's throttle for a session: its subagents' holds and the spawn block.
    Throttle {
        session: SessionId,
        throttle: ThrottledSession,
    },
    RestoreChoice {
        session: SessionId,
    },
}

impl Decision {
    #[must_use]
    pub const fn session(&self) -> &SessionId {
        match self {
            Self::StepEffortDown { session }
            | Self::StepModelDown { session }
            | Self::StepUp { session }
            | Self::Rest { session, .. }
            | Self::Stop { session }
            | Self::Resume { session }
            | Self::Release { session }
            | Self::Throttle { session, .. }
            | Self::RestoreChoice { session } => session,
        }
    }

    /// The kind the TypeScript would have recorded for this decision, or `None`
    /// for one it records nowhere: the round's throttle is written, not stepped,
    /// and a choice restored is not recorded either - neither is compared.
    #[must_use]
    pub const fn observed_kind(&self) -> Option<ObservedKind> {
        match self {
            Self::StepEffortDown { .. } => Some(ObservedKind::StepEffortDown),
            Self::StepModelDown { .. } => Some(ObservedKind::StepModelDown),
            Self::StepUp { .. } => Some(ObservedKind::StepUp),
            Self::Rest { .. } => Some(ObservedKind::Rest),
            Self::Stop { .. } => Some(ObservedKind::Stop),
            Self::Resume { .. } => Some(ObservedKind::Resume),
            Self::Release { .. } => Some(ObservedKind::Release),
            Self::Throttle { .. } | Self::RestoreChoice { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT_JSON: &str = r#"{
        "windowLabel": "main",
        "at": 1000,
        "sessions": [{
            "id": "s1", "conversationId": null, "agentId": "claude", "command": "claude",
            "name": "work", "isWorking": true, "backgroundAgentCount": 0,
            "isPacingOn": true, "isHandingOff": false, "hasUsageLimit": false,
            "isSkipped": false, "isContinuedRun": false, "isShielded": false,
            "isModelSwitchApplying": false, "workingSince": 500, "workingShare": 0.5,
            "ownLoad": 2.0, "subagents": [{"id": "a", "description": null, "load": 1.0}],
            "choice": {
                "model": "opus", "effort": "high",
                "original": {"model": "opus", "effort": "xhigh"},
                "canStepDown": {"effort": true, "model": false}, "canStepUp": true,
                "restoreAt": 9000, "effortSaving": 0.25, "modelSaving": 0.0
            },
            "pace": {
                "phase": "paused", "windowKind": "session", "readyAt": 2000,
                "continueAt": 3000, "restUntil": null, "hasStoppedWork": true,
                "isHeld": false, "pausedAt": 900, "rates": {"five_hour": 0.00001}
            }
        }],
        "observed": [{"sessionId": "s1", "at": 990, "kind": "stepEffortDown"}]
    }"#;

    #[test]
    fn a_report_in_the_contract_shape_parses() {
        let report: PacingReport = serde_json::from_str(REPORT_JSON).expect("a valid report");
        assert_eq!(report.window_label.as_str(), "main");
        let session = &report.sessions[0];
        assert_eq!(session.id.as_str(), "s1");
        assert_eq!(session.subagents.len(), 1);
        let pace = session.pace.as_ref().expect("a pace");
        assert_eq!(pace.phase, PacePhase::Paused);
        assert_eq!(pace.window_kind, PacedWindowKind::Session);
        assert_eq!(pace.rates.len(), 1);
        assert_eq!(report.observed[0].kind, ObservedKind::StepEffortDown);
    }

    #[test]
    fn a_report_survives_a_round_trip() {
        let report: PacingReport = serde_json::from_str(REPORT_JSON).expect("a valid report");
        let written = serde_json::to_string(&report).expect("serializes");
        let again: PacingReport = serde_json::from_str(&written).expect("parses again");
        assert_eq!(report, again);
    }

    #[test]
    fn a_pause_without_rates_parses_with_none() {
        let without = REPORT_JSON.replace(r#", "rates": {"five_hour": 0.00001}"#, "");
        let report: PacingReport = serde_json::from_str(&without).expect("a valid report");
        let pace = report.sessions[0].pace.as_ref().expect("a pace");
        assert!(pace.rates.is_empty());
    }

    #[test]
    fn observed_kinds_travel_under_their_contract_names() {
        let names: Vec<String> = [
            ObservedKind::StepEffortDown,
            ObservedKind::StepModelDown,
            ObservedKind::StepUp,
            ObservedKind::Rest,
            ObservedKind::Stop,
            ObservedKind::Resume,
            ObservedKind::Release,
            ObservedKind::None,
        ]
        .iter()
        .map(|kind| serde_json::to_string(kind).expect("serializes"))
        .collect();
        assert_eq!(
            names,
            [
                "\"stepEffortDown\"",
                "\"stepModelDown\"",
                "\"stepUp\"",
                "\"rest\"",
                "\"stop\"",
                "\"resume\"",
                "\"release\"",
                "\"none\""
            ]
        );
    }

    #[test]
    fn every_decision_but_the_throttle_and_the_restore_has_an_observed_kind() {
        let session = SessionId::new("s");
        let until = EpochMilliseconds::new(1.0);
        let steppable = [
            Decision::StepEffortDown {
                session: session.clone(),
            },
            Decision::StepModelDown {
                session: session.clone(),
            },
            Decision::StepUp {
                session: session.clone(),
            },
            Decision::Rest {
                session: session.clone(),
                until,
            },
            Decision::Stop {
                session: session.clone(),
            },
            Decision::Resume {
                session: session.clone(),
            },
            Decision::Release {
                session: session.clone(),
            },
        ];
        assert!(steppable
            .iter()
            .all(|decision| decision.observed_kind().is_some()));
        let throttle = Decision::Throttle {
            session: session.clone(),
            throttle: ThrottledSession {
                session_id: "s".to_string(),
                name: "n".to_string(),
                is_blocking_spawns: true,
                subagents: Vec::new(),
            },
        };
        assert_eq!(throttle.observed_kind(), None);
        let restore = Decision::RestoreChoice {
            session: session.clone(),
        };
        assert_eq!(restore.observed_kind(), None);
        assert_eq!(throttle.session(), &session);
    }
}
