//! Usage pacing in the backend. Two halves, one concern each:
//!
//! * the pure math, ported from the TypeScript under `src/lib/usage/pacing/` so
//!   the backend can run it - no I/O, no Tauri and no state: every function
//!   takes what it reads and returns what it decides, and each is held to the
//!   TypeScript's own output by the golden fixtures in `pacing/fixtures/`
//!   (regenerate them with `scripts/pacing-fixtures.ts`). Its submodules, one
//!   concern each: `units` (the typed numbers the formula computes in),
//!   `reset_time` (a window's reset string as a point in time), `usage_pace`
//!   (the pace formula), `policy` (its margins), `load_shedding` (sharing an
//!   account's slowdown among its sessions), `subagent_steps` (the holds on a
//!   running subagent), `account_round` (one round over every session on every
//!   account), and the model reads they price by - `model_label`, `model_cost`
//!   and `token_price`;
//! * the engine (`docs/pacing-engine.md`), in its SHADOW phase: the windows
//!   report what only a window can see (`pacing_report`, `facts`, `registry`),
//!   a tick reads each account once (`samples` keeps its burn) and asks the pure
//!   `engine::decide` what pacing would do, and `shadow` holds that against what
//!   the TypeScript actually did (`pacing_shadow_summary`). Nothing here writes
//!   a control file, types into a terminal or emits an event that changes
//!   behaviour: its only outputs are the sample book and the shadow log under
//!   the PADE data directory.
//!
//! This file is the engine's wiring: the lock around each piece of state, the
//! two commands and the tick. A lock is only a lookup - it is released before
//! any usage read or file write.

pub mod account_round;
pub mod engine;
pub mod facts;
#[cfg(test)]
mod golden_fixtures;
pub mod load_shedding;
pub mod model_cost;
pub mod model_label;
pub mod policy;
pub mod registry;
pub mod reset_time;
pub mod samples;
pub mod shadow;
pub mod subagent_steps;
#[cfg(test)]
mod test_support;
pub mod token_price;
pub mod units;
pub mod usage_pace;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tauri::{AppHandle, Manager, State};

use crate::pacing::engine::{
    accounts_to_sample, decide, AccountReading, EngineInputs, EngineMemory,
};
use crate::pacing::facts::{AccountKey, AgentId, ObservedDecision, PacingReport, SessionId};
use crate::pacing::registry::{LiveSession, Registry, POLL_INTERVAL};
use crate::pacing::samples::{RecordParams, SampleBook, SAMPLES_FILE_NAME};
use crate::pacing::shadow::{
    log_line, Comparison, SessionContext, ShadowLog, ShadowState, ShadowSummary,
};
use crate::pacing::units::{EpochMilliseconds, SpanMilliseconds};
use crate::usage::{usage_account_agent, AccountUsage};
use crate::util::{pade_data_directory, since_unix_epoch};

/// Everything the engine keeps, each piece behind its own lock.
#[derive(Debug, Default)]
pub struct PacingEngine {
    registry: Mutex<Registry>,
    samples: Mutex<SampleBook>,
    memory: Mutex<EngineMemory>,
    shadow: Mutex<ShadowState>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a tick takes from the reports before it reads any account.
#[derive(Debug)]
struct Prepared {
    sessions: Vec<LiveSession>,
    observed: Vec<ObservedDecision>,
    /// The agents whose account a session with pacing on spends from.
    agents: BTreeSet<AgentId>,
}

#[derive(Debug)]
struct Settlement<'a> {
    now: EpochMilliseconds,
    prepared: Prepared,
    accounts: &'a BTreeMap<AgentId, AccountReading>,
}

/// What a tick leaves to write: the sample book when it changed, and the
/// shadow log's line when there was anything to compare.
#[derive(Debug, Default, PartialEq)]
struct Writes {
    samples: Option<SampleBook>,
    log_line: Option<String>,
}

impl PacingEngine {
    fn with_samples(samples: SampleBook) -> Self {
        Self {
            samples: Mutex::new(samples),
            ..Self::default()
        }
    }

    fn submit(&self, report: PacingReport) {
        locked(&self.registry).submit(report);
    }

    fn summary(&self) -> ShadowSummary {
        locked(&self.shadow).summary()
    }

    fn prepare(&self, now: EpochMilliseconds) -> Prepared {
        let (sessions, observed) = {
            let mut registry = locked(&self.registry);
            (registry.live_sessions(now), registry.drain_observed(now))
        };
        let agents = sessions
            .iter()
            .filter(|live| live.facts.is_pacing_on)
            .map(|live| live.facts.agent_id.clone())
            .collect();
        Prepared {
            sessions,
            observed,
            agents,
        }
    }

    /// Take a reading of each working account, decide what pacing would do and
    /// compare it with what the windows did. Pure but for the locks.
    fn settle(&self, settlement: Settlement<'_>) -> Writes {
        let Settlement {
            now,
            prepared,
            accounts,
        } = settlement;
        let Prepared {
            sessions, observed, ..
        } = prepared;

        let (decisions, samples) = {
            let mut book = locked(&self.samples);
            let before = book.clone();
            record_samples(&mut book, &sessions, accounts, now);
            let mut memory = locked(&self.memory);
            let outcome = decide(&EngineInputs {
                now,
                sessions: &sessions,
                accounts,
                samples: &book,
                memory: &memory,
            });
            for account_key in &outcome.stepped_accounts {
                book.forget_account(account_key);
            }

            *memory = outcome.memory;
            let samples = (*book != before).then(|| book.clone());
            (outcome.decisions, samples)
        };

        let verdicts = locked(&self.shadow).compare(Comparison {
            now,
            decisions: &decisions,
            observed: &observed,
        });
        let contexts: BTreeMap<SessionId, SessionContext> = sessions
            .iter()
            .map(|live| {
                (
                    live.facts.id.clone(),
                    SessionContext::from_facts(&live.facts),
                )
            })
            .collect();
        Writes {
            samples,
            log_line: log_line(now, &verdicts, &contexts),
        }
    }
}

/// One reading per account that is being worked, of every window it has.
fn record_samples(
    book: &mut SampleBook,
    sessions: &[LiveSession],
    accounts: &BTreeMap<AgentId, AccountReading>,
    now: EpochMilliseconds,
) {
    for account_key in accounts_to_sample(sessions, accounts) {
        let windows = accounts
            .values()
            .find(|reading| reading.key == account_key)
            .and_then(|reading| reading.usage.as_ref())
            .map(|usage| usage.windows.as_slice());
        if let Some(windows) = windows {
            book.record(RecordParams {
                account_key: &account_key,
                windows,
                now,
            });
        }
    }
}

/// One window's report of its scan. The registry update is an in-memory lookup,
/// so the command stays synchronous.
#[tauri::command]
pub fn pacing_report(report: PacingReport, engine: State<'_, PacingEngine>) {
    engine.submit(report);
}

/// How often the engine and the TypeScript agreed, by kind, and the latest
/// disagreements.
#[tauri::command]
#[must_use]
pub fn pacing_shadow_summary(engine: State<'_, PacingEngine>) -> ShadowSummary {
    engine.summary()
}

fn current_time() -> EpochMilliseconds {
    EpochMilliseconds::new(SpanMilliseconds::from(since_unix_epoch()).get())
}

/// The account of each agent, read once: its key (the billing identity, or the
/// agent's id while that is unknown) and its windows.
async fn read_accounts(agents: &BTreeSet<AgentId>) -> BTreeMap<AgentId, AccountReading> {
    let mut accounts = BTreeMap::new();
    for agent in agents {
        let usage = usage_account_agent(agent.as_str().to_string(), false)
            .await
            .into_account();
        let key = usage
            .as_ref()
            .and_then(AccountUsage::billing_identity)
            .map_or_else(|| AccountKey::new(agent.as_str()), AccountKey::new);
        accounts.insert(agent.clone(), AccountReading { key, usage });
    }

    accounts
}

/// Write what a tick left to write. A failure is told on stderr: the shadow
/// phase has no other voice, and a log that stops growing should say why.
async fn persist(writes: Writes) {
    let Some(directory) = pade_data_directory() else {
        eprintln!("[pade] pacing: no data directory, so nothing is persisted");
        return;
    };

    let written = tauri::async_runtime::spawn_blocking(move || {
        let mut failures = Vec::new();
        if let Some(samples) = writes.samples {
            if let Err(error) = samples.save(&directory.join(SAMPLES_FILE_NAME)) {
                failures.push(format!("the burn samples: {error}"));
            }
        }

        if let Some(line) = writes.log_line {
            if let Err(error) = ShadowLog::new(directory).append(&line) {
                failures.push(format!("the shadow log: {error}"));
            }
        }

        failures
    })
    .await;
    match written {
        Ok(failures) => {
            for failure in failures {
                eprintln!("[pade] pacing: could not write {failure}");
            }
        }
        Err(error) => eprintln!("[pade] pacing: the write task failed: {error}"),
    }
}

async fn tick(app: &AppHandle) {
    let engine = app.state::<PacingEngine>();
    let now = current_time();
    let prepared = engine.prepare(now);
    let accounts = read_accounts(&prepared.agents).await;
    let writes = engine.settle(Settlement {
        now,
        prepared,
        accounts: &accounts,
    });
    persist(writes).await;
}

async fn run_ticks(app: AppHandle) {
    let Ok(period) = Duration::try_from(POLL_INTERVAL) else {
        eprintln!("[pade] pacing: the poll interval is not a duration, no ticks");
        return;
    };

    let mut ticks = tokio::time::interval(period);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        tick(&app).await;
    }
}

/// Manage the engine - with the burn samples the last run left - and start its
/// tick. Shadow only.
pub fn init(app: &AppHandle) {
    let samples = pade_data_directory()
        .map(|directory| SampleBook::load(&directory.join(SAMPLES_FILE_NAME)))
        .unwrap_or_default();
    app.manage(PacingEngine::with_samples(samples));
    tauri::async_runtime::spawn(run_ticks(app.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacing::facts::ObservedKind;
    use crate::pacing::test_support::{account, session_window, NOW};
    use crate::test_support::assert_empty;

    const MEASURED_READINGS: u32 = 12;

    fn report(observed: &serde_json::Value) -> PacingReport {
        serde_json::from_value(serde_json::json!({
            "windowLabel": "main",
            "at": NOW.get(),
            "sessions": [{
                "id": "a", "conversationId": null, "agentId": "claude", "command": "claude",
                "name": "a", "isWorking": true, "backgroundAgentCount": 0, "isPacingOn": true,
                "isHandingOff": false, "hasUsageLimit": false, "isSkipped": false,
                "isContinuedRun": false, "isShielded": false, "isModelSwitchApplying": false,
                "workingSince": NOW.get() - 1_800_000.0, "workingShare": 1.0, "ownLoad": 1.0,
                "subagents": [], "choice": null, "pace": null
            }],
            "observed": observed
        }))
        .expect("a valid report")
    }

    fn accounts(used: f64) -> BTreeMap<AgentId, AccountReading> {
        BTreeMap::from([(
            AgentId::new("claude"),
            AccountReading {
                key: AccountKey::new("anthropic"),
                usage: Some(account(vec![session_window(
                    used,
                    SpanMilliseconds::from_hours(3.0),
                )])),
            },
        )])
    }

    /// An engine that has watched the account burn one percent a minute.
    fn engine_that_measured(used: f64) -> PacingEngine {
        let mut book = SampleBook::default();
        for reading in 0..=MEASURED_READINGS {
            let ago = f64::from(MEASURED_READINGS - reading);
            let window = session_window(used - ago, SpanMilliseconds::from_hours(3.0));
            book.record(RecordParams {
                account_key: &AccountKey::new("anthropic"),
                windows: std::slice::from_ref(&window),
                now: NOW - SpanMilliseconds::from_minutes(ago),
            });
        }
        PacingEngine::with_samples(book)
    }

    fn pass(engine: &PacingEngine, now: EpochMilliseconds, used: f64) -> Writes {
        let prepared = engine.prepare(now);
        engine.settle(Settlement {
            now,
            prepared,
            accounts: &accounts(used),
        })
    }

    fn stop_observed(observed_at: EpochMilliseconds) -> serde_json::Value {
        serde_json::json!([{ "sessionId": "a", "at": observed_at.get(), "kind": "stop" }])
    }

    #[test]
    fn a_stop_both_sides_took_is_an_agreement() {
        let engine = engine_that_measured(80.0);
        engine.submit(report(&stop_observed(NOW)));
        let writes = pass(&engine, NOW, 80.0);

        let summary = engine.summary();
        assert_eq!(summary.counts[&ObservedKind::Stop].agree, 1);
        assert_empty(&summary.last_disagreements);
        let line = writes.log_line.expect("something was compared");
        assert!(line.contains("\"isAgreement\":true"));
    }

    #[test]
    fn a_stop_the_window_never_reports_becomes_a_disagreement_a_tick_later() {
        let engine = engine_that_measured(80.0);
        engine.submit(report(&serde_json::json!([])));
        let first = pass(&engine, NOW, 80.0);
        assert_eq!(first.log_line, None);

        let later = NOW + SpanMilliseconds::from_minutes(1.0);
        let mut next = report(&serde_json::json!([]));
        next.time = later;
        engine.submit(next);
        pass(&engine, later, 80.0);
        let summary = engine.summary();
        assert_eq!(summary.counts[&ObservedKind::Stop].disagree, 1);
        assert_eq!(summary.last_disagreements[0].observed, None);
    }

    #[test]
    fn a_report_that_has_expired_decides_nothing() {
        let engine = engine_that_measured(80.0);
        engine.submit(report(&serde_json::json!([])));
        let much_later = NOW + SpanMilliseconds::from_minutes(10.0);
        let writes = pass(&engine, much_later, 80.0);
        assert_eq!(writes, Writes::default());
    }

    #[test]
    fn a_working_account_is_sampled_and_the_book_is_handed_over_to_write() {
        let engine = PacingEngine::default();
        engine.submit(report(&serde_json::json!([])));
        let writes = pass(&engine, NOW, 50.0);
        assert!(writes.samples.is_some());

        let prepared = engine.prepare(NOW);
        assert_eq!(prepared.agents, BTreeSet::from([AgentId::new("claude")]));
    }

    #[test]
    fn an_account_a_step_went_to_starts_its_burn_over() {
        let engine = engine_that_measured(80.0);
        engine.submit(report(&serde_json::json!([])));
        let writes = pass(&engine, NOW, 80.0);
        let saved = writes.samples.expect("the forgotten book is written");
        assert_eq!(saved, SampleBook::default());
    }
}
