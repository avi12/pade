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
mod tests;
