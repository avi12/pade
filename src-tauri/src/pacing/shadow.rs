//! The shadow comparison: what the engine would decide against what the
//! TypeScript decided, session by session and kind by kind, written down so the
//! two can be held to each other before the engine acts. Observation only - it
//! reads decisions and facts and touches nothing but its own log.
//!
//! A decision the engine makes that the TypeScript has not recorded yet is not
//! a disagreement at once: the windows report once a minute and only the round
//! leader's window decides in its own scan, so the others hear of a step after
//! their own report. The engine's decision waits one tick for the same step to
//! show up in the window's next report (agreement with lag) and counts as a
//! disagreement only if it does not. A step the TypeScript took that the engine
//! did not decide is a disagreement at once.
//!
//! The pure part (`ShadowState`) holds the clock as a parameter; the file I/O
//! (`ShadowLog`) is a size-bounded JSONL appender the wiring calls from a
//! blocking task.

use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

use crate::pacing::facts::{
    Decision, ObservedDecision, ObservedKind, PacePhase, SessionFacts, SessionId,
};
use crate::pacing::units::EpochMilliseconds;

/// Where the log lives under the PADE data directory.
pub const SHADOW_LOG_FILE_NAME: &str = "pacing-shadow.jsonl";
/// Where a full log is moved to, replacing the one before it.
const ROTATED_LOG_FILE_NAME: &str = "pacing-shadow.1.jsonl";
/// A log this large is rotated, so the two files together stay under twice it.
const LOG_LIMIT_BYTES: u64 = 1024 * 1024;
/// How many of the latest disagreements the summary keeps.
const LAST_DISAGREEMENT_LIMIT: usize = 20;

/// One comparison: the kind the engine decided and the one the TypeScript took,
/// either of which is absent where the other side did nothing. Equal means the
/// two agree. The summary hands these to the frontend as they are.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub at: EpochMilliseconds,
    pub session_id: SessionId,
    pub observed: Option<ObservedKind>,
    pub decided: Option<ObservedKind>,
}

impl Verdict {
    #[must_use]
    pub fn is_agreement(&self) -> bool {
        self.observed == self.decided
    }

    /// The kind this verdict counts under.
    fn kind(&self) -> Option<ObservedKind> {
        self.observed.or(self.decided)
    }
}

/// How often the engine and the TypeScript agreed on one kind of decision.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct KindCounts {
    pub agree: u64,
    pub disagree: u64,
}

/// What `pacing_shadow_summary` answers.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShadowSummary {
    /// By kind; a kind nobody decided is absent.
    pub counts: BTreeMap<ObservedKind, KindCounts>,
    pub last_disagreements: Vec<Verdict>,
}

/// A decision of the engine still waiting for the window to report it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Waiting {
    session: SessionId,
    kind: ObservedKind,
}

/// What the engine saw of a session when it disagreed, for the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext {
    pub is_working: bool,
    pub pace_phase: Option<PacePhase>,
    pub is_stepped_down: bool,
    pub is_pacing_on: bool,
}

impl SessionContext {
    #[must_use]
    pub fn of(session: &SessionFacts) -> Self {
        Self {
            is_working: session.is_working,
            pace_phase: session.pace.as_ref().map(|pace| pace.phase),
            is_stepped_down: session
                .choice
                .as_ref()
                .is_some_and(|choice| choice.can_step_up),
            is_pacing_on: session.is_pacing_on,
        }
    }
}

/// One comparison's inputs.
#[derive(Debug, Clone, Copy)]
pub struct Comparison<'a> {
    pub now: EpochMilliseconds,
    pub decisions: &'a [Decision],
    /// What the windows reported since the last comparison.
    pub observed: &'a [ObservedDecision],
}

#[derive(Debug, Default)]
pub struct ShadowState {
    waiting: Vec<Waiting>,
    counts: BTreeMap<ObservedKind, KindCounts>,
    last_disagreements: VecDeque<Verdict>,
}

impl ShadowState {
    /// Compare the engine's decisions with the TypeScript's, counting each
    /// verdict. Returns the verdicts reached this tick.
    pub fn compare(&mut self, comparison: Comparison<'_>) -> Vec<Verdict> {
        let Comparison {
            now,
            decisions,
            observed,
        } = comparison;
        let mut unclaimed: Vec<&ObservedDecision> = observed
            .iter()
            .filter(|decision| decision.kind != ObservedKind::None)
            .collect();
        let mut verdicts = Vec::new();

        for waiting in std::mem::take(&mut self.waiting) {
            let matched = claim(&mut unclaimed, &waiting.session, waiting.kind);
            verdicts.push(Verdict {
                at: matched.map_or(now, |decision| decision.at),
                session_id: waiting.session,
                observed: matched.map(|decision| decision.kind),
                decided: Some(waiting.kind),
            });
        }

        for decision in decisions {
            let Some(kind) = decision.observed_kind() else {
                continue;
            };

            let session = decision.session();
            match claim(&mut unclaimed, session, kind) {
                Some(matched) => verdicts.push(Verdict {
                    at: matched.at,
                    session_id: session.clone(),
                    observed: Some(kind),
                    decided: Some(kind),
                }),
                None => self.waiting.push(Waiting {
                    session: session.clone(),
                    kind,
                }),
            }
        }

        verdicts.extend(unclaimed.into_iter().map(|decision| Verdict {
            at: decision.at,
            session_id: decision.session_id.clone(),
            observed: Some(decision.kind),
            decided: None,
        }));
        for verdict in &verdicts {
            self.count(verdict);
        }

        verdicts
    }

    fn count(&mut self, verdict: &Verdict) {
        let Some(kind) = verdict.kind() else {
            return;
        };

        let counts = self.counts.entry(kind).or_default();
        if verdict.is_agreement() {
            counts.agree += 1;
            return;
        }

        counts.disagree += 1;
        self.last_disagreements.push_back(verdict.clone());
        while self.last_disagreements.len() > LAST_DISAGREEMENT_LIMIT {
            self.last_disagreements.pop_front();
        }
    }

    #[must_use]
    pub fn summary(&self) -> ShadowSummary {
        ShadowSummary {
            counts: self.counts.clone(),
            last_disagreements: self.last_disagreements.iter().cloned().collect(),
        }
    }
}

/// Take the first unclaimed observed decision of this session and kind.
fn claim<'a>(
    unclaimed: &mut Vec<&'a ObservedDecision>,
    session: &SessionId,
    kind: ObservedKind,
) -> Option<&'a ObservedDecision> {
    let position = unclaimed
        .iter()
        .position(|decision| decision.session_id == *session && decision.kind == kind)?;
    Some(unclaimed.remove(position))
}

/// One verdict as the log writes it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry<'a> {
    #[serde(flatten)]
    verdict: &'a Verdict,
    is_agreement: bool,
    context: Option<SessionContext>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogLine<'a> {
    at: EpochMilliseconds,
    entries: Vec<LogEntry<'a>>,
}

/// The log line of one comparison, or `None` when there was nothing to compare.
/// A disagreement carries what the engine saw of the session.
#[must_use]
pub fn log_line(
    now: EpochMilliseconds,
    verdicts: &[Verdict],
    contexts: &BTreeMap<SessionId, SessionContext>,
) -> Option<String> {
    if verdicts.is_empty() {
        return None;
    }

    let entries = verdicts
        .iter()
        .map(|verdict| LogEntry {
            verdict,
            is_agreement: verdict.is_agreement(),
            context: contexts.get(&verdict.session_id).copied(),
        })
        .collect();
    serde_json::to_string(&LogLine { at: now, entries }).ok()
}

/// The size-bounded JSONL file the lines go to.
#[derive(Debug, Clone)]
pub struct ShadowLog {
    directory: PathBuf,
}

impl ShadowLog {
    #[must_use]
    pub const fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    /// Append one line, first moving a full log aside (the one before it is
    /// replaced), so the log never outgrows twice its limit.
    pub fn append(&self, line: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.directory)?;
        let path = self.directory.join(SHADOW_LOG_FILE_NAME);
        let size = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
        if size >= LOG_LIMIT_BYTES {
            std::fs::rename(&path, self.directory.join(ROTATED_LOG_FILE_NAME))?;
        }

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(file, "{line}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacing::account_round::ThrottledSession;
    use crate::pacing::units::SpanMilliseconds;
    use crate::test_support::assert_empty;

    const START: f64 = 5_000_000.0;

    fn at(minutes: f64) -> EpochMilliseconds {
        EpochMilliseconds::new(START) + SpanMilliseconds::from_minutes(minutes)
    }

    fn stop(id: &str) -> Decision {
        Decision::Stop {
            session: SessionId::new(id),
        }
    }

    fn observed(id: &str, minutes: f64, kind: ObservedKind) -> ObservedDecision {
        ObservedDecision {
            session_id: SessionId::new(id),
            at: at(minutes),
            kind,
        }
    }

    fn compare(
        state: &mut ShadowState,
        minutes: f64,
        decisions: &[Decision],
        reported: &[ObservedDecision],
    ) -> Vec<Verdict> {
        state.compare(Comparison {
            now: at(minutes),
            decisions,
            observed: reported,
        })
    }

    #[test]
    fn the_same_step_on_both_sides_agrees() {
        let mut state = ShadowState::default();
        let verdicts = compare(
            &mut state,
            1.0,
            &[stop("a")],
            &[observed("a", 0.5, ObservedKind::Stop)],
        );
        assert_eq!(verdicts.len(), 1);
        assert!(verdicts[0].is_agreement());
        assert_eq!(state.summary().counts[&ObservedKind::Stop].agree, 1);
    }

    #[test]
    fn a_step_only_the_typescript_took_disagrees_at_once() {
        let mut state = ShadowState::default();
        let verdicts = compare(
            &mut state,
            1.0,
            &[],
            &[observed("a", 0.5, ObservedKind::Rest)],
        );
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].observed, Some(ObservedKind::Rest));
        assert_eq!(verdicts[0].decided, None);
        let summary = state.summary();
        assert_eq!(summary.counts[&ObservedKind::Rest].disagree, 1);
        assert_eq!(summary.last_disagreements, verdicts);
    }

    #[test]
    fn a_step_only_the_engine_decided_waits_a_tick_for_the_window() {
        let mut state = ShadowState::default();
        let first = compare(&mut state, 1.0, &[stop("a")], &[]);
        assert_empty(&first);
        assert_empty(&state.summary().last_disagreements);

        let second = compare(&mut state, 2.0, &[], &[]);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].decided, Some(ObservedKind::Stop));
        assert_eq!(second[0].observed, None);
        assert_eq!(state.summary().counts[&ObservedKind::Stop].disagree, 1);
    }

    #[test]
    fn a_step_that_shows_up_in_the_next_report_agrees_with_lag() {
        let mut state = ShadowState::default();
        assert_empty(&compare(&mut state, 1.0, &[stop("a")], &[]));
        let second = compare(
            &mut state,
            2.0,
            &[],
            &[observed("a", 1.5, ObservedKind::Stop)],
        );
        assert_eq!(second.len(), 1);
        assert!(second[0].is_agreement());
        assert_eq!(state.summary().counts[&ObservedKind::Stop].disagree, 0);
    }

    #[test]
    fn a_none_from_the_window_never_counts_on_its_own() {
        let mut state = ShadowState::default();
        let verdicts = compare(
            &mut state,
            1.0,
            &[],
            &[observed("a", 0.5, ObservedKind::None)],
        );
        assert_empty(&verdicts);
        assert!(state.summary().counts.is_empty());
    }

    #[test]
    fn the_same_kind_on_another_session_does_not_agree() {
        let mut state = ShadowState::default();
        let verdicts = compare(
            &mut state,
            1.0,
            &[stop("a")],
            &[observed("b", 0.5, ObservedKind::Stop)],
        );
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].session_id.as_str(), "b");
        assert!(!verdicts[0].is_agreement());
    }

    #[test]
    fn a_different_kind_on_the_same_session_is_two_disagreements() {
        let mut state = ShadowState::default();
        let first = compare(
            &mut state,
            1.0,
            &[stop("a")],
            &[observed("a", 0.5, ObservedKind::Rest)],
        );
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].observed, Some(ObservedKind::Rest));
        let second = compare(&mut state, 2.0, &[], &[]);
        assert_eq!(second[0].decided, Some(ObservedKind::Stop));
    }

    #[test]
    fn the_throttle_and_the_restore_are_not_compared() {
        let mut state = ShadowState::default();
        let decisions = [
            Decision::Throttle {
                session: SessionId::new("a"),
                throttle: ThrottledSession {
                    session_id: "a".to_string(),
                    name: "a".to_string(),
                    is_blocking_spawns: true,
                    subagents: Vec::new(),
                },
            },
            Decision::RestoreChoice {
                session: SessionId::new("a"),
            },
        ];
        assert_empty(&compare(&mut state, 1.0, &decisions, &[]));
        assert_empty(&compare(&mut state, 2.0, &[], &[]));
        assert!(state.summary().counts.is_empty());
    }

    #[test]
    fn two_decisions_of_one_kind_claim_one_observation_each() {
        let mut state = ShadowState::default();
        let verdicts = compare(
            &mut state,
            1.0,
            &[stop("a"), stop("a")],
            &[observed("a", 0.5, ObservedKind::Stop)],
        );
        assert_eq!(verdicts.len(), 1);
        let next = compare(&mut state, 2.0, &[], &[]);
        assert_eq!(next.len(), 1);
        assert!(!next[0].is_agreement());
    }

    #[test]
    fn only_the_latest_disagreements_are_kept() {
        let mut state = ShadowState::default();
        let many: Vec<ObservedDecision> = (0..30_u32)
            .map(|index| observed(&format!("s{index}"), 0.5, ObservedKind::Resume))
            .collect();
        let verdicts = compare(&mut state, 1.0, &[], &many);
        assert_eq!(verdicts.len(), 30);
        let summary = state.summary();
        assert_eq!(summary.counts[&ObservedKind::Resume].disagree, 30);
        assert_eq!(summary.last_disagreements.len(), LAST_DISAGREEMENT_LIMIT);
        assert_eq!(summary.last_disagreements[0].session_id.as_str(), "s10");
    }

    #[test]
    fn the_summary_serializes_in_the_shape_the_frontend_reads() {
        let mut state = ShadowState::default();
        compare(
            &mut state,
            1.0,
            &[stop("a")],
            &[observed("a", 0.5, ObservedKind::Stop)],
        );
        compare(
            &mut state,
            1.0,
            &[],
            &[observed("b", 0.5, ObservedKind::Release)],
        );
        let json = serde_json::to_value(state.summary()).expect("serializes");
        assert_eq!(json["counts"]["stop"]["agree"], 1);
        assert_eq!(json["counts"]["release"]["disagree"], 1);
        let last = &json["lastDisagreements"][0];
        assert_eq!(last["sessionId"], "b");
        assert_eq!(last["observed"], "release");
        assert!(last["decided"].is_null());
        assert!(last["at"].is_number());
    }

    #[test]
    fn a_log_line_carries_each_verdict_with_what_the_engine_saw() {
        let verdict = Verdict {
            at: at(1.0),
            session_id: SessionId::new("a"),
            observed: None,
            decided: Some(ObservedKind::Stop),
        };
        let contexts = BTreeMap::from([(
            SessionId::new("a"),
            SessionContext {
                is_working: true,
                pace_phase: None,
                is_stepped_down: false,
                is_pacing_on: true,
            },
        )]);
        let line = log_line(at(1.0), &[verdict], &contexts).expect("a line");
        let json: serde_json::Value = serde_json::from_str(&line).expect("json");
        let entry = &json["entries"][0];
        assert_eq!(entry["isAgreement"], false);
        assert_eq!(entry["decided"], "stop");
        assert_eq!(entry["context"]["isWorking"], true);
        assert!(log_line(at(1.0), &[], &contexts).is_none());
    }

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("pade-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&directory).ok();
        directory
    }

    #[test]
    fn the_log_appends_a_line_per_call() {
        let directory = scratch("shadow-append");
        let log = ShadowLog::new(directory.clone());
        log.append("one").expect("appends");
        log.append("two").expect("appends");
        let written = std::fs::read_to_string(directory.join(SHADOW_LOG_FILE_NAME)).expect("reads");
        std::fs::remove_dir_all(&directory).expect("cleans up");
        assert_eq!(written, "one\ntwo\n");
    }

    #[test]
    fn a_full_log_is_moved_aside_and_started_again() {
        let directory = scratch("shadow-rotate");
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        let full = "x".repeat(usize::try_from(LOG_LIMIT_BYTES).expect("fits"));
        std::fs::write(directory.join(SHADOW_LOG_FILE_NAME), full).expect("fills");

        ShadowLog::new(directory.clone())
            .append("fresh")
            .expect("appends");

        let current = std::fs::read_to_string(directory.join(SHADOW_LOG_FILE_NAME)).expect("reads");
        let rotated = std::fs::metadata(directory.join(ROTATED_LOG_FILE_NAME)).expect("rotated");
        std::fs::remove_dir_all(&directory).expect("cleans up");
        assert_eq!(current, "fresh\n");
        assert_eq!(rotated.len(), LOG_LIMIT_BYTES);
    }
}
