//! One account-wide pacing round over every live session in every PADE window,
//! and every subagent those sessions run - the port of
//! `lib/usage/pacing/engine/account-round.ts`. The wire types match the zod schemas
//! there (camelCase), so a report or plan crosses IPC unchanged.
//!
//! An account's burn is one number for all of it, so its slowdown is one
//! decision. Each window owns its own sessions - only it can switch a session's
//! model or stop it - so each window reports what its working sessions spend and
//! what their next step would save, and ONE window, the leader (the open window
//! whose label sorts first among those reporting), plans the round for all of
//! them:
//!
//! * over pace, every candidate - each session by its own share (its running
//!   subagents excluded), each running subagent by its own - offers its next step
//!   and what it saves, and the steps go to the biggest share of the account
//!   saved first until they cover what the account must shed (`plan_shedding`);
//! * while the account sheds past `NO_SPAWN_SHED`, no session on it may start a
//!   subagent;
//! * with room again on every session of the account, the lightest stepped-down
//!   session climbs back and each subagent's hold eases.
//!
//! A session stepped by a round is skipped until its window reports again, so a
//! report older than the step never steps it twice.

mod accounts;

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::pacing::load_shedding::Load;
use crate::pacing::subagent_steps::SubagentStep;
use crate::pacing::units::{EpochMilliseconds, Fraction, SpanMilliseconds};
use accounts::{plan_account, AccountInputs, AccountPlan};

/// The channel the windows share the round on (they share an origin).
pub const PACING_CHANNEL: &str = "pade.usage-pacing";

/// A window's report counts for this many of its scans.
const REPORT_LIFETIME_POLLS: f64 = 2.5;

/// One running subagent a session reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentReport {
    /// The agent's own subagent id, as its hooks report its tool calls.
    pub id: String,
    pub description: Option<String>,
    /// Its measured share of the account, on the sessions' scale.
    pub load: Load,
}

/// Over pace by its own project's settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverPace {
    /// What the account must shed - `None` while unmeasured.
    pub shed: Option<Fraction>,
    /// What its next step saves of its own load.
    pub saving: Fraction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReport {
    pub id: String,
    /// The tab's name, for the usage panel's list.
    pub name: String,
    pub account_key: String,
    pub command: String,
    pub conversation_id: Option<String>,
    /// When its window measured it.
    pub reported_at: EpochMilliseconds,
    /// Its own share of the account, its running subagents excluded.
    pub load: Load,
    pub over_pace: Option<OverPace>,
    /// Stepped down, with room to step back up.
    pub can_step_up: bool,
    /// On pace with room to spare.
    pub has_room: bool,
    pub subagents: Vec<SubagentReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowReport {
    pub label: String,
    #[serde(rename = "at")]
    pub time: EpochMilliseconds,
    pub sessions: Vec<SessionReport>,
}

/// The round's subagent steps by `subagent_key` - its memory, kept where every
/// window reads it, so a new leader takes over where the last one stopped.
pub type SubagentSteps = BTreeMap<String, SubagentStep>;

/// One subagent's throttle, as the usage panel names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThrottledSubagent {
    pub id: String,
    pub description: Option<String>,
    pub step: SubagentStep,
}

/// One session's throttle as the round set it, with what the usage panel names
/// it by.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThrottledSession {
    pub session_id: String,
    pub name: String,
    pub is_blocking_spawns: bool,
    pub subagents: Vec<ThrottledSubagent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundPlan {
    pub leader: String,
    #[serde(rename = "at")]
    pub time: EpochMilliseconds,
    /// Sessions to take their next step, each in its own window.
    pub climb: Vec<String>,
    /// Stepped-down sessions to step back up.
    pub restore: Vec<String>,
    /// Accounts a step went to: every window starts their burn rate over.
    pub stepped_accounts: Vec<String>,
    /// Every reported session, throttled or not - an empty throttle lifts it.
    pub throttles: Vec<ThrottledSession>,
}

/// What one window says to the others on the channel. The `kind` tag is the
/// TypeScript's `PacingMessageKind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PacingMessage {
    Report { report: WindowReport },
    Plan { plan: RoundPlan },
    Gone { label: String },
}

/// A window's report counts this long - a couple of its scans - so a window that
/// closed without a goodbye stops leading and stops being planned for.
#[must_use]
pub fn report_lifetime(poll: SpanMilliseconds) -> SpanMilliseconds {
    poll * REPORT_LIFETIME_POLLS
}

#[derive(Debug, Clone, Copy)]
pub struct FreshReportsParams<'a> {
    pub reports: &'a [WindowReport],
    pub now: EpochMilliseconds,
    pub lifetime: SpanMilliseconds,
}

/// The reports still current at `now`.
#[must_use]
pub fn fresh_reports(params: FreshReportsParams<'_>) -> Vec<&WindowReport> {
    let FreshReportsParams {
        reports,
        now,
        lifetime,
    } = params;
    reports
        .iter()
        .filter(|report| now - report.time <= lifetime)
        .collect()
}

/// The window that plans the round: the first label among the reporting ones, in
/// the UTF-16 order JavaScript sorts strings by.
pub fn round_leader(reports: &[&WindowReport]) -> Option<String> {
    reports
        .iter()
        .map(|report| report.label.as_str())
        .min_by(|first, second| first.encode_utf16().cmp(second.encode_utf16()))
        .map(ToString::to_string)
}

#[derive(Debug, Clone, Copy)]
pub struct SubagentKeyParts<'a> {
    pub session_id: &'a str,
    pub subagent_id: &'a str,
}

/// How the round keys one subagent: its session and its own id.
#[must_use]
pub fn subagent_key(parts: SubagentKeyParts<'_>) -> String {
    format!("{}/{}", parts.session_id, parts.subagent_id)
}

#[derive(Debug, Clone, Copy)]
pub struct PlanRoundParams<'a> {
    pub reports: &'a [&'a WindowReport],
    /// The round's subagent steps so far, by `subagent_key`.
    pub steps: &'a SubagentSteps,
    /// When a round last stepped each session.
    pub stepped_at: &'a HashMap<String, EpochMilliseconds>,
    pub leader: &'a str,
    pub now: EpochMilliseconds,
}

/// The round and the subagent steps it leaves.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoundOutcome {
    pub plan: RoundPlan,
    pub steps: SubagentSteps,
}

/// Every session reported, grouped by account in the order each account first
/// appears.
fn sessions_by_account<'a>(reports: &[&'a WindowReport]) -> Vec<(&'a str, Vec<&'a SessionReport>)> {
    let mut by_account: Vec<(&str, Vec<&SessionReport>)> = Vec::new();
    for session in reports.iter().flat_map(|report| &report.sessions) {
        let key = session.account_key.as_str();
        match by_account.iter_mut().find(|(account, _)| *account == key) {
            Some((_, sessions)) => sessions.push(session),
            None => by_account.push((key, vec![session])),
        }
    }

    by_account
}

fn throttle_of(session: &SessionReport, account: &AccountPlan) -> ThrottledSession {
    ThrottledSession {
        session_id: session.id.clone(),
        name: session.name.clone(),
        is_blocking_spawns: account.is_blocking_spawns,
        subagents: session
            .subagents
            .iter()
            .filter_map(|subagent| {
                let key = subagent_key(SubagentKeyParts {
                    session_id: &session.id,
                    subagent_id: &subagent.id,
                });
                account.steps.get(&key).map(|step| ThrottledSubagent {
                    id: subagent.id.clone(),
                    description: subagent.description.clone(),
                    step: *step,
                })
            })
            .collect(),
    }
}

/// The round over every fresh window's sessions, account by account, and the
/// subagent steps it leaves - a subagent no longer reported loses its step.
#[must_use]
pub fn plan_round(params: PlanRoundParams<'_>) -> RoundOutcome {
    let PlanRoundParams {
        reports,
        steps,
        stepped_at,
        leader,
        now,
    } = params;
    let mut plan = RoundPlan {
        leader: leader.to_string(),
        time: now,
        climb: Vec::new(),
        restore: Vec::new(),
        stepped_accounts: Vec::new(),
        throttles: Vec::new(),
    };
    let mut next_steps = SubagentSteps::new();
    for (account_key, sessions) in sessions_by_account(reports) {
        let account = plan_account(AccountInputs {
            sessions: &sessions,
            steps,
            stepped_at,
        });
        plan.climb.extend(account.climb.iter().cloned());
        plan.restore.extend(account.restore.iter().cloned());
        next_steps.extend(account.steps.iter().map(|(key, step)| (key.clone(), *step)));
        if account.has_stepped {
            plan.stepped_accounts.push(account_key.to_string());
        }

        plan.throttles.extend(
            sessions
                .iter()
                .map(|session| throttle_of(session, &account)),
        );
    }

    RoundOutcome {
        plan,
        steps: next_steps,
    }
}

/// A throttle as the session's control file holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlThrottle {
    pub is_blocking_spawns: bool,
    pub subagents: BTreeMap<String, SubagentStep>,
}

/// Whether a throttle asks anything of its session.
#[must_use]
pub fn is_throttling(throttle: &ThrottledSession) -> bool {
    throttle.is_blocking_spawns || !throttle.subagents.is_empty()
}

/// A throttle as the session's control file holds it.
#[must_use]
pub fn control_throttle(throttle: &ThrottledSession) -> ControlThrottle {
    ControlThrottle {
        is_blocking_spawns: throttle.is_blocking_spawns,
        subagents: throttle
            .subagents
            .iter()
            .map(|subagent| (subagent.id.clone(), subagent.step))
            .collect(),
    }
}

#[cfg(test)]
mod tests;
