//! One account-wide pacing round over every live session in every PADE window,
//! and every subagent those sessions run - the port of
//! `lib/usage/pacing/account-round.ts`. The wire types match the zod schemas
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

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::pacing::load_shedding::{
    lightest_to_restore, plan_shedding, Load, PlanSheddingParams, RestoreCandidate, ShedCandidate,
};
use crate::pacing::subagent_steps::{
    eased_subagent_step, is_blocking_spawns, next_subagent_step, NextStepParams, SubagentStep,
};
use crate::pacing::units::{EpochMilliseconds, Fraction, SpanMilliseconds};

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
    pub at: EpochMilliseconds,
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
    pub at: EpochMilliseconds,
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
        .filter(|report| now - report.at <= lifetime)
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

/// Whose step a shed candidate is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ShedTarget {
    /// A session, by its id.
    Session(String),
    /// A subagent, by its round key.
    Subagent(String),
}

/// Every running subagent of an account's sessions, under its round key.
fn subagents_of(sessions: &[&SessionReport]) -> Vec<(String, Load)> {
    sessions
        .iter()
        .flat_map(|session| {
            session.subagents.iter().map(|subagent| {
                (
                    subagent_key(SubagentKeyParts {
                        session_id: &session.id,
                        subagent_id: &subagent.id,
                    }),
                    subagent.load,
                )
            })
        })
        .collect()
}

/// What the account must shed: the most any of its over-pace sessions asks for -
/// `None` while none of them has a measured burn.
fn account_shed(sessions: &[&SessionReport]) -> Option<Fraction> {
    let mut shed: Option<Fraction> = None;
    for session in sessions {
        let session_shed = session.over_pace.and_then(|over_pace| over_pace.shed);
        let is_larger_shed = session_shed.is_some_and(|asked| shed.is_none_or(|held| asked > held));
        if is_larger_shed {
            shed = session_shed;
        }
    }

    shed
}

/// What the round decided for one account.
struct AccountPlan {
    climb: Vec<String>,
    restore: Vec<String>,
    steps: SubagentSteps,
    is_blocking_spawns: bool,
    has_stepped: bool,
}

#[derive(Clone, Copy)]
struct AccountInputs<'a> {
    sessions: &'a [&'a SessionReport],
    steps: &'a SubagentSteps,
    /// When a round last stepped each session.
    stepped_at: &'a HashMap<String, EpochMilliseconds>,
}

/// An account on pace: once every session on it has room, each hold eases and the
/// lightest stepped-down session climbs back.
fn ease_account(inputs: AccountInputs<'_>) -> AccountPlan {
    let AccountInputs {
        sessions, steps, ..
    } = inputs;
    let has_room = sessions.iter().all(|session| session.has_room);
    let mut next_steps = SubagentSteps::new();
    for (key, _) in subagents_of(sessions) {
        let held = steps.get(&key).copied();
        let kept = match held {
            Some(current) if has_room => eased_subagent_step(current),
            Some(_) | None => held,
        };
        if let Some(step) = kept {
            next_steps.insert(key, step);
        }
    }

    let restorable: Vec<RestoreCandidate<String>> = sessions
        .iter()
        .filter(|session| session.can_step_up)
        .map(|session| RestoreCandidate {
            id: session.id.clone(),
            load: session.load,
        })
        .collect();
    let restoring = lightest_to_restore(&restorable);
    AccountPlan {
        climb: Vec::new(),
        has_stepped: restoring.is_some(),
        restore: restoring.into_iter().collect(),
        steps: next_steps,
        is_blocking_spawns: false,
    }
}

/// Every step the account could take this round: each over-pace session not
/// stepped since its report, and each subagent with a step left.
fn shed_candidates(
    inputs: AccountInputs<'_>,
    shed: Option<Fraction>,
) -> (
    Vec<ShedCandidate<ShedTarget>>,
    HashMap<String, SubagentStep>,
) {
    let AccountInputs {
        sessions,
        steps,
        stepped_at,
    } = inputs;
    let mut candidates = Vec::new();
    for session in sessions {
        let stepped_time = stepped_at
            .get(&session.id)
            .copied()
            .unwrap_or(EpochMilliseconds::new(0.0));
        let is_stale_after_step = stepped_time > session.reported_at;
        let Some(over_pace) = session.over_pace else {
            continue;
        };

        if !is_stale_after_step {
            candidates.push(ShedCandidate {
                id: ShedTarget::Session(session.id.clone()),
                load: session.load,
                saving: over_pace.saving,
            });
        }
    }

    let mut moves = HashMap::new();
    for (key, load) in subagents_of(sessions) {
        let next = next_subagent_step(NextStepParams {
            current: steps.get(&key).copied(),
            shed,
        });
        if let Some(next) = next {
            moves.insert(key.clone(), next.step);
            candidates.push(ShedCandidate {
                id: ShedTarget::Subagent(key),
                load,
                saving: next.saving,
            });
        }
    }

    (candidates, moves)
}

/// An account over pace: the steps that save the most of it go first, until they
/// cover its shed.
fn shed_account(inputs: AccountInputs<'_>) -> AccountPlan {
    let AccountInputs {
        sessions, steps, ..
    } = inputs;
    let shed = account_shed(sessions);
    let subagents = subagents_of(sessions);
    let total_load: Load = sessions
        .iter()
        .map(|session| session.load)
        .chain(subagents.iter().map(|(_, load)| *load))
        .sum();
    let (candidates, moves) = shed_candidates(inputs, shed);
    let chosen = plan_shedding(PlanSheddingParams {
        candidates: &candidates,
        total_load,
        fraction: shed,
    });
    let mut next_steps = SubagentSteps::new();
    for (key, _) in &subagents {
        let is_chosen = chosen.contains(&ShedTarget::Subagent(key.clone()));
        let kept = if is_chosen {
            moves.get(key).copied()
        } else {
            steps.get(key).copied()
        };
        if let Some(step) = kept {
            next_steps.insert(key.clone(), step);
        }
    }

    let climb = chosen
        .iter()
        .filter_map(|target| match target {
            ShedTarget::Session(id) => Some(id.clone()),
            ShedTarget::Subagent(_) => None,
        })
        .collect();
    AccountPlan {
        climb,
        restore: Vec::new(),
        steps: next_steps,
        is_blocking_spawns: is_blocking_spawns(shed),
        has_stepped: !chosen.is_empty(),
    }
}

/// One account's sessions and the round's say over them.
fn plan_account(inputs: AccountInputs<'_>) -> AccountPlan {
    let is_over_pace = inputs
        .sessions
        .iter()
        .any(|session| session.over_pace.is_some());
    if is_over_pace {
        shed_account(inputs)
    } else {
        ease_account(inputs)
    }
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
        at: now,
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
mod tests {
    use super::*;
    use crate::pacing::subagent_steps::{
        hold_for_saving, HoldSeconds, MAXIMUM_HOLD_SECONDS, NO_SPAWN_SHED,
    };

    const NOW: EpochMilliseconds = EpochMilliseconds::new(1_794_052_800_000.0);
    const ACCOUNT: &str = "avi@example.com";

    fn session(id: &str) -> SessionReport {
        SessionReport {
            id: id.to_string(),
            name: id.to_string(),
            account_key: ACCOUNT.to_string(),
            command: "claude".to_string(),
            conversation_id: Some(format!("{id}-conversation")),
            reported_at: NOW,
            load: Load::new(10.0),
            over_pace: None,
            can_step_up: false,
            has_room: false,
            subagents: Vec::new(),
        }
    }

    fn over(shed: f64, saving: f64) -> OverPace {
        OverPace {
            shed: Some(Fraction::new(shed)),
            saving: Fraction::new(saving),
        }
    }

    fn subagent(id: &str, description: Option<&str>, load: f64) -> SubagentReport {
        SubagentReport {
            id: id.to_string(),
            description: description.map(ToString::to_string),
            load: Load::new(load),
        }
    }

    fn window_report(label: &str, sessions: Vec<SessionReport>) -> WindowReport {
        WindowReport {
            label: label.to_string(),
            at: NOW,
            sessions,
        }
    }

    fn key(session_id: &str, subagent_id: &str) -> String {
        subagent_key(SubagentKeyParts {
            session_id,
            subagent_id,
        })
    }

    fn gate(seconds: u32) -> SubagentStep {
        SubagentStep::Gate {
            hold_seconds: HoldSeconds::new(seconds).expect("a positive hold"),
        }
    }

    fn round_with(
        reports: &[WindowReport],
        steps: &SubagentSteps,
        stepped_at: &HashMap<String, EpochMilliseconds>,
    ) -> RoundOutcome {
        let references: Vec<&WindowReport> = reports.iter().collect();
        plan_round(PlanRoundParams {
            reports: &references,
            steps,
            stepped_at,
            leader: "main",
            now: NOW,
        })
    }

    fn round(reports: &[WindowReport]) -> RoundOutcome {
        round_with(reports, &SubagentSteps::new(), &HashMap::new())
    }

    #[test]
    fn the_leader_is_one_window_the_same_whichever_window_asks() {
        let reports = [
            window_report("window-2", vec![]),
            window_report("main", vec![]),
            window_report("window-1", vec![]),
        ];
        let forward: Vec<&WindowReport> = reports.iter().collect();
        let reversed: Vec<&WindowReport> = reports.iter().rev().collect();
        assert_eq!(round_leader(&forward).as_deref(), Some("main"));
        assert_eq!(round_leader(&reversed).as_deref(), Some("main"));
        assert_eq!(round_leader(&[]), None);
    }

    #[test]
    fn a_window_that_stopped_reporting_is_dropped_so_the_next_one_leads() {
        let stale = WindowReport {
            at: NOW - SpanMilliseconds::from_minutes(10.0),
            ..window_report("main", vec![])
        };
        let reports = [stale, window_report("window-1", vec![])];
        let fresh = fresh_reports(FreshReportsParams {
            reports: &reports,
            now: NOW,
            lifetime: SpanMilliseconds::new(150_000.0),
        });
        assert_eq!(round_leader(&fresh).as_deref(), Some("window-1"));
    }

    #[test]
    fn a_report_lives_two_and_a_half_scans() {
        assert_eq!(
            report_lifetime(SpanMilliseconds::new(60_000.0)),
            SpanMilliseconds::new(150_000.0)
        );
    }

    #[test]
    fn steps_the_biggest_saver_wherever_it_lives_not_the_leaders_own_session_first() {
        let front = SessionReport {
            load: Load::new(10.0),
            over_pace: Some(over(0.1, 0.2)),
            ..session("front")
        };
        let background = SessionReport {
            load: Load::new(40.0),
            over_pace: Some(over(0.1, 0.2)),
            ..session("background")
        };
        let outcome = round(&[
            window_report("main", vec![front]),
            window_report("window-1", vec![background]),
        ]);
        assert_eq!(outcome.plan.climb, ["background"]);
        assert_eq!(outcome.plan.stepped_accounts, [ACCOUNT]);
    }

    #[test]
    fn sheds_the_accounts_fraction_of_every_windows_load_once_not_once_per_window() {
        let sessions: Vec<SessionReport> = ["a", "b", "c", "d"]
            .into_iter()
            .map(|id| SessionReport {
                load: Load::new(25.0),
                over_pace: Some(over(0.2, 0.5)),
                ..session(id)
            })
            .collect();
        let outcome = round(&[
            window_report("main", sessions[..2].to_vec()),
            window_report("window-1", sessions[2..].to_vec()),
        ]);
        // 20% of 100 needs 20 saved; each step saves 12.5, so two - not four.
        assert_eq!(outcome.plan.climb.len(), 2);
    }

    #[test]
    fn ranks_a_background_sessions_running_subagent_against_the_sessions_by_its_own_load() {
        let front = SessionReport {
            load: Load::new(5.0),
            over_pace: Some(over(0.1, 0.2)),
            ..session("front")
        };
        let background = SessionReport {
            load: Load::new(5.0),
            subagents: vec![subagent("a1", Some("Explore the repo"), 60.0)],
            ..session("background")
        };
        let outcome = round(&[
            window_report("main", vec![front]),
            window_report("window-1", vec![background]),
        ]);
        assert_eq!(
            outcome.steps.get(&key("background", "a1")),
            Some(&SubagentStep::Gate {
                hold_seconds: hold_for_saving(Fraction::new(0.1))
            })
        );
        assert_eq!(outcome.plan.climb, Vec::<String>::new());
        let throttle = outcome
            .plan
            .throttles
            .iter()
            .find(|entry| entry.session_id == "background")
            .expect("a throttle for the background session");
        let descriptions: Vec<Option<&str>> = throttle
            .subagents
            .iter()
            .map(|entry| entry.description.as_deref())
            .collect();
        assert_eq!(descriptions, [Some("Explore the repo")]);
        assert!(!throttle.is_blocking_spawns);
    }

    #[test]
    fn wraps_up_a_subagent_already_held_the_longest_and_keeps_the_others_steps() {
        let wrapping = key("s", "a1");
        let kept = key("s", "a2");
        let steps = SubagentSteps::from([
            (
                wrapping.clone(),
                SubagentStep::Gate {
                    hold_seconds: MAXIMUM_HOLD_SECONDS,
                },
            ),
            (kept.clone(), gate(10)),
        ]);
        let sessions = vec![SessionReport {
            load: Load::new(1.0),
            over_pace: Some(over(0.1, 0.0)),
            subagents: vec![subagent("a1", None, 50.0), subagent("a2", None, 1.0)],
            ..session("s")
        }];
        let outcome = round_with(
            &[window_report("window-1", sessions)],
            &steps,
            &HashMap::new(),
        );
        assert_eq!(outcome.steps.get(&wrapping), Some(&SubagentStep::WrapUp));
        assert_eq!(outcome.steps.get(&kept), steps.get(&kept));
    }

    #[test]
    fn blocks_new_subagents_on_every_session_of_the_account_only_past_the_threshold() {
        let shedding = |shed: f64| -> Vec<bool> {
            let asking = SessionReport {
                over_pace: Some(over(shed, 0.2)),
                ..session("a")
            };
            round(&[
                window_report("main", vec![asking]),
                window_report("window-1", vec![session("b")]),
            ])
            .plan
            .throttles
            .iter()
            .map(|throttle| throttle.is_blocking_spawns)
            .collect()
        };
        assert_eq!(shedding(NO_SPAWN_SHED.get()), [true, true]);
        assert_eq!(shedding(NO_SPAWN_SHED.get() / 2.0), [false, false]);
    }

    #[test]
    fn keeps_other_accounts_out_of_the_round() {
        let other = SessionReport {
            account_key: "someone@example.com".to_string(),
            subagents: vec![subagent("a1", None, 90.0)],
            ..session("other")
        };
        let over_pace = SessionReport {
            over_pace: Some(over(0.5, 0.5)),
            ..session("over")
        };
        let outcome = round(&[
            window_report("main", vec![over_pace]),
            window_report("window-1", vec![other]),
        ]);
        assert!(outcome.steps.is_empty());
        let throttle = outcome
            .plan
            .throttles
            .iter()
            .find(|entry| entry.session_id == "other")
            .expect("a throttle for the other account's session");
        assert!(!throttle.is_blocking_spawns);
    }

    #[test]
    fn does_not_step_a_session_again_on_a_report_older_than_its_last_step() {
        let stale = SessionReport {
            reported_at: NOW - SpanMilliseconds::new(30_000.0),
            over_pace: Some(over(0.5, 0.5)),
            ..session("s")
        };
        let stepped_at = HashMap::from([("s".to_string(), NOW - SpanMilliseconds::new(10_000.0))]);
        let outcome = round_with(
            &[window_report("window-1", vec![stale])],
            &SubagentSteps::new(),
            &stepped_at,
        );
        assert_eq!(outcome.plan.climb, Vec::<String>::new());
    }

    #[test]
    fn with_room_again_on_every_session_eases_holds_and_restores_the_lightest_stepped_down_session()
    {
        let steps = SubagentSteps::from([
            (key("a", "a1"), gate(20)),
            (key("a", "finished"), SubagentStep::WrapUp),
        ]);
        let roomy = SessionReport {
            has_room: true,
            subagents: vec![subagent("a1", None, 5.0)],
            ..session("a")
        };
        let light = SessionReport {
            load: Load::new(2.0),
            has_room: true,
            can_step_up: true,
            ..session("light")
        };
        let heavy = SessionReport {
            load: Load::new(20.0),
            has_room: true,
            can_step_up: true,
            ..session("heavy")
        };
        let outcome = round_with(
            &[
                window_report("main", vec![roomy]),
                window_report("window-1", vec![light, heavy]),
            ],
            &steps,
            &HashMap::new(),
        );
        assert_eq!(
            outcome.steps,
            SubagentSteps::from([(key("a", "a1"), gate(10))])
        );
        assert_eq!(outcome.plan.restore, ["light"]);
    }

    #[test]
    fn holds_the_steps_while_some_session_on_the_account_has_no_room_yet() {
        let steps = SubagentSteps::from([(key("a", "a1"), gate(20))]);
        let roomy = SessionReport {
            has_room: true,
            subagents: vec![subagent("a1", None, 5.0)],
            ..session("a")
        };
        let outcome = round_with(
            &[
                window_report("main", vec![roomy]),
                window_report("window-1", vec![session("b")]),
            ],
            &steps,
            &HashMap::new(),
        );
        assert_eq!(outcome.steps, steps);
    }

    #[test]
    fn a_well_formed_plan_crosses_the_wire_and_a_malformed_one_is_refused() {
        let outcome = round(&[window_report("main", vec![session("s")])]);
        let message = PacingMessage::Plan { plan: outcome.plan };
        let wire = serde_json::to_value(&message).expect("serializes");
        assert_eq!(wire["kind"], "plan");
        let parsed: Result<PacingMessage, _> = serde_json::from_value(wire);
        assert_eq!(parsed.expect("parses back"), message);
        let malformed = serde_json::json!({ "kind": "plan", "plan": { "leader": 1 } });
        assert!(serde_json::from_value::<PacingMessage>(malformed).is_err());
    }

    #[test]
    fn writes_a_throttle_as_the_control_file_holds_it() {
        let throttle = ThrottledSession {
            session_id: "s".to_string(),
            name: "s".to_string(),
            is_blocking_spawns: false,
            subagents: vec![ThrottledSubagent {
                id: "a1".to_string(),
                description: None,
                step: SubagentStep::WrapUp,
            }],
        };
        assert!(is_throttling(&throttle));
        let wire = serde_json::to_value(control_throttle(&throttle)).expect("serializes");
        assert_eq!(
            wire,
            serde_json::json!({
                "isBlockingSpawns": false,
                "subagents": { "a1": { "step": "wrapUp" } }
            })
        );
        let idle = ThrottledSession {
            subagents: Vec::new(),
            ..throttle
        };
        assert!(!is_throttling(&idle));
    }
}
