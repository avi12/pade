//! The latest report of every window, kept while it is fresh. It replaces the
//! `BroadcastChannel` the windows used to share their reports on
//! (`account_round`): a window that stops reporting simply expires after the
//! same lifetime (`report_lifetime`), and there is no leader to elect - the
//! backend has one engine. Pure: the clock is a parameter, the lock lives in
//! `pacing.rs`.

use std::collections::BTreeMap;

use crate::pacing::account_round::report_lifetime;
use crate::pacing::facts::{ObservedDecision, PacingReport, SessionFacts, WindowLabel};
use crate::pacing::units::{EpochMilliseconds, SpanMilliseconds};

/// How often each window reports - the TypeScript's `POLL_MS`. A report lives
/// a couple of these.
pub const POLL_INTERVAL: SpanMilliseconds = SpanMilliseconds::from_minutes(1.0);

/// How many observed decisions one window may have waiting for a comparison.
const OBSERVED_LIMIT: usize = 256;

#[derive(Debug, Clone)]
struct StoredReport {
    at: EpochMilliseconds,
    sessions: Vec<SessionFacts>,
    observed: Vec<ObservedDecision>,
}

/// A session of a fresh report, with the window and the moment it reported it.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveSession {
    pub window: WindowLabel,
    pub reported_at: EpochMilliseconds,
    pub facts: SessionFacts,
}

#[derive(Debug, Default)]
pub struct Registry {
    windows: BTreeMap<WindowLabel, StoredReport>,
}

impl Registry {
    /// Take a window's report: its sessions replace the last report's, its
    /// observed decisions queue behind the ones not compared yet.
    pub fn submit(&mut self, report: PacingReport) {
        let PacingReport {
            window_label,
            at,
            sessions,
            observed,
        } = report;
        let mut waiting = self
            .windows
            .remove(&window_label)
            .map(|stored| stored.observed)
            .unwrap_or_default();
        waiting.extend(observed);
        let overflow = waiting.len().saturating_sub(OBSERVED_LIMIT);
        waiting.drain(..overflow);
        self.windows.insert(
            window_label,
            StoredReport {
                at,
                sessions,
                observed: waiting,
            },
        );
    }

    /// Every session of every window whose report is still fresh at `now`.
    #[must_use]
    pub fn live_sessions(&self, now: EpochMilliseconds) -> Vec<LiveSession> {
        let lifetime = report_lifetime(POLL_INTERVAL);
        self.windows
            .iter()
            .filter(|(_, stored)| now - stored.at <= lifetime)
            .flat_map(|(label, stored)| {
                stored.sessions.iter().map(|facts| LiveSession {
                    window: label.clone(),
                    reported_at: stored.at,
                    facts: facts.clone(),
                })
            })
            .collect()
    }

    /// Take the observed decisions that happened by `now`, from every window -
    /// a window that closed after its last scan still answers for it - and drop
    /// a window that has expired with nothing left to compare.
    pub fn drain_observed(&mut self, now: EpochMilliseconds) -> Vec<ObservedDecision> {
        let mut due = Vec::new();
        for stored in self.windows.values_mut() {
            let (happened, later): (Vec<_>, Vec<_>) = std::mem::take(&mut stored.observed)
                .into_iter()
                .partition(|decision| decision.at <= now);
            due.extend(happened);
            stored.observed = later;
        }

        let lifetime = report_lifetime(POLL_INTERVAL);
        self.windows
            .retain(|_, stored| now - stored.at <= lifetime || !stored.observed.is_empty());
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacing::facts::{ObservedKind, SessionId};
    use crate::test_support::assert_empty;

    const START: f64 = 1_000_000.0;

    fn at(offset_minutes: f64) -> EpochMilliseconds {
        EpochMilliseconds::new(START) + SpanMilliseconds::from_minutes(offset_minutes)
    }

    fn session(id: &str) -> SessionFacts {
        serde_json::from_value(serde_json::json!({
            "id": id, "conversationId": null, "agentId": "claude", "command": "claude",
            "name": id, "isWorking": true, "backgroundAgentCount": 0, "isPacingOn": true,
            "isHandingOff": false, "hasUsageLimit": false, "isSkipped": false,
            "isContinuedRun": false, "isShielded": false, "isModelSwitchApplying": false,
            "workingSince": null, "workingShare": 1.0, "ownLoad": 1.0, "subagents": [],
            "choice": null, "pace": null
        }))
        .expect("a valid session")
    }

    fn report(label: &str, reported_at: EpochMilliseconds, ids: &[&str]) -> PacingReport {
        PacingReport {
            window_label: WindowLabel::new(label),
            at: reported_at,
            sessions: ids.iter().map(|id| session(id)).collect(),
            observed: Vec::new(),
        }
    }

    fn observed(id: &str, when: EpochMilliseconds, kind: ObservedKind) -> ObservedDecision {
        ObservedDecision {
            session_id: SessionId::new(id),
            at: when,
            kind,
        }
    }

    #[test]
    fn a_report_is_live_for_two_and_a_half_polls() {
        let mut registry = Registry::default();
        registry.submit(report("main", at(0.0), &["a"]));
        assert_eq!(registry.live_sessions(at(0.0)).len(), 1);
        assert_eq!(registry.live_sessions(at(2.5)).len(), 1);
        assert_empty(&registry.live_sessions(at(2.6)));
    }

    #[test]
    fn a_newer_report_replaces_the_windows_sessions() {
        let mut registry = Registry::default();
        registry.submit(report("main", at(0.0), &["a", "b"]));
        registry.submit(report("main", at(1.0), &["c"]));
        let live = registry.live_sessions(at(1.0));
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].facts.id.as_str(), "c");
        assert_eq!(live[0].reported_at, at(1.0));
    }

    #[test]
    fn every_fresh_window_contributes_and_a_stale_one_does_not() {
        let mut registry = Registry::default();
        registry.submit(report("main", at(0.0), &["a"]));
        registry.submit(report("second", at(3.0), &["b"]));
        let live = registry.live_sessions(at(3.0));
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].window.as_str(), "second");
    }

    #[test]
    fn observed_decisions_queue_across_reports_until_drained() {
        let mut registry = Registry::default();
        let mut first = report("main", at(0.0), &["a"]);
        first
            .observed
            .push(observed("a", at(0.0), ObservedKind::Stop));
        registry.submit(first);
        let mut second = report("main", at(1.0), &["a"]);
        second
            .observed
            .push(observed("a", at(1.0), ObservedKind::Resume));
        registry.submit(second);

        let due = registry.drain_observed(at(1.0));
        assert_eq!(due.len(), 2);
        assert_empty(&registry.drain_observed(at(1.0)));
    }

    #[test]
    fn a_decision_after_now_waits_for_a_later_drain() {
        let mut registry = Registry::default();
        let mut early = report("main", at(0.0), &["a"]);
        early
            .observed
            .push(observed("a", at(0.5), ObservedKind::Rest));
        registry.submit(early);
        assert_empty(&registry.drain_observed(at(0.0)));
        assert_eq!(registry.drain_observed(at(1.0)).len(), 1);
    }

    #[test]
    fn an_expired_window_is_forgotten_once_it_has_nothing_to_compare() {
        let mut registry = Registry::default();
        let mut last = report("main", at(0.0), &["a"]);
        last.observed
            .push(observed("a", at(0.0), ObservedKind::Stop));
        registry.submit(last);
        assert_eq!(registry.drain_observed(at(10.0)).len(), 1);
        assert!(registry.windows.is_empty());
    }

    #[test]
    fn the_observed_queue_is_bounded_keeping_the_newest() {
        let mut registry = Registry::default();
        let mut busy = report("main", at(0.0), &["a"]);
        for index in 0..(OBSERVED_LIMIT + 10) {
            let offset = f64::from(u32::try_from(index).expect("small"));
            busy.observed.push(observed(
                "a",
                EpochMilliseconds::new(START + offset),
                ObservedKind::None,
            ));
        }
        registry.submit(busy);
        let due = registry.drain_observed(at(1.0));
        assert_eq!(due.len(), OBSERVED_LIMIT);
        assert_eq!(due[0].at, EpochMilliseconds::new(START + 10.0));
    }
}
