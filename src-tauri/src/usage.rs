//! Agent usage / quota meter.
//!
//! Surfaces how much of the active agent's quota is left, sourced WITHOUT
//! spending any message quota - we never invoke the agent CLI. Two sources,
//! best-effort per agent:
//!  - The vendor's OAuth usage endpoint (the same numbers claude.ai shows),
//!    called with the locally-stored access token and cached for ~3 minutes -
//!    a real network request, but one that consumes no quota.
//!  - Data the agent already persisted locally (e.g. the subscription tier
//!    label) when the network / token isn't available, leaving
//!    `used_percentage`/`resets_at` `None` if the precise numbers aren't on disk.
//!
//! When nothing reliable is found the account reading says why
//! ([`NoReadingReason`]: no usage source, signed out, sign-in expired, or the
//! vendor unreachable) and the UI shows that honestly - we never fabricate
//! numbers. We never refresh a vendor token either: refresh tokens are
//! single-use, and rotating one under a running CLI would sign it out.
//!
//! `limit_reset` adds the meter's one action: offering and claiming Claude's
//! free usage-limit reset over the same OAuth endpoint.

mod antigravity;
mod claude;
mod codex;
mod copilot;
mod cursor;
pub(crate) mod limit_reset;
mod muse;
mod readings;
mod sign_in;
#[cfg(test)]
mod tests;
mod transport;
pub(crate) mod window_time;

use self::antigravity::fetch_antigravity_account_usage;
use self::claude::{claude_tier_label_usage, fetch_claude_account_usage};
use self::codex::{codex_tier_label_usage, fetch_codex_account_usage};
use self::copilot::fetch_copilot_account_usage;
use self::cursor::fetch_cursor_account_usage;
use self::muse::fetch_muse_account_usage;
pub(crate) use self::readings::{
    AccountReading, AccountUsage, NoReading, NoReadingReason, UsageWindow, UsageWindowKind,
};
use crate::agents::{
    ID_ANTIGRAVITY, ID_CLAUDE, ID_CODEX, ID_COPILOT, ID_CURSOR, ID_MUSE, ID_OPENCODE,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;
use std::time::Instant;

/// The agents we can source usage for, and the one authoritative mapping from
/// the registry id string the frontend sends (`session.agent.id`). An agent we
/// have no usage adapter for isn't in this set, so its usage stays an honest "-".
///
/// Two adapter shapes live here. A *live* adapter (Claude, Codex, Copilot) reads
/// real rate-limit windows from the vendor's own usage endpoint with the locally
/// stored token. A *tier-label* adapter (Antigravity, Cursor) has no quota-free
/// usage endpoint we can rely on, so it confirms the local login exists and
/// surfaces just the plan label with no numbers. Agents that authenticate with
/// the user's *own* pay-as-you-go provider API key rather than a metered
/// subscription - Grok (an xAI key) and aider (the user's OpenAI/Anthropic key) -
/// have no plan quota at all, so they are deliberately absent and stay "-".
/// A *CLI* adapter (Muse) asks the agent's own CLI to print its allowance, so
/// PADE never touches its credentials at all.
/// Qwen Code has no usage source: its Qwen OAuth free tier closed on 2026-04-15
/// (<https://github.com/QwenLM/qwen-code/issues/3203>), it calls no quota
/// endpoint and keeps no counter file, and its `/stats` shows local token
/// counts - what was used, never what is left - so it stays "no usage source".
/// An agent id and an adapter aren't 1:1: opencode signs into the same
/// `ChatGPT` subscription Codex does, so its id resolves to the Codex adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum UsageAgent {
    Claude,
    Codex,
    Copilot,
    Antigravity,
    Cursor,
    Muse,
}

impl UsageAgent {
    fn from_id(id: &str) -> Option<Self> {
        match id {
            ID_CLAUDE => Some(Self::Claude),
            // opencode authenticates against the same ChatGPT subscription Codex
            // does, so it reads the Codex adapter - one account, one source of
            // truth.
            ID_CODEX | ID_OPENCODE => Some(Self::Codex),
            ID_COPILOT => Some(Self::Copilot),
            ID_ANTIGRAVITY => Some(Self::Antigravity),
            ID_CURSOR => Some(Self::Cursor),
            ID_MUSE => Some(Self::Muse),
            _ => None,
        }
    }

    /// Which window heads this adapter's meter - see [`HeadlineWindow`].
    fn headline_window(self) -> HeadlineWindow {
        match self {
            Self::Claude => HeadlineWindow::WeeklyOnly,
            Self::Codex | Self::Copilot | Self::Antigravity | Self::Cursor | Self::Muse => {
                HeadlineWindow::WeeklyOrMostConsumed
            }
        }
    }

    /// The registry id of the agent whose sign-in this adapter reads - the CLI a
    /// signed-out or expired reading tells the user to run. opencode reads
    /// Codex's login, so its hint names Codex.
    fn sign_in_agent(self) -> &'static str {
        match self {
            Self::Claude => ID_CLAUDE,
            Self::Codex => ID_CODEX,
            Self::Copilot => ID_COPILOT,
            Self::Antigravity => ID_ANTIGRAVITY,
            Self::Cursor => ID_CURSOR,
            Self::Muse => ID_MUSE,
        }
    }

    /// Read this adapter's account afresh, bypassing the cache.
    fn fetch_account_usage(self) -> Result<AccountUsage, NoReadingReason> {
        match self {
            Self::Claude => fetch_claude_account_usage(),
            Self::Codex => fetch_codex_account_usage(),
            Self::Copilot => fetch_copilot_account_usage(),
            Self::Antigravity => fetch_antigravity_account_usage(),
            Self::Cursor => fetch_cursor_account_usage(),
            Self::Muse => fetch_muse_account_usage(),
        }
    }

    /// The honest plan label the meter falls back to when the live account can't
    /// be read (offline, or the token expired). The other adapters already fall
    /// back to their local login inside the account read itself.
    fn offline_usage(self) -> Option<Usage> {
        match self {
            Self::Claude => claude_tier_label_usage(),
            Self::Codex => codex_tier_label_usage(),
            Self::Copilot | Self::Antigravity | Self::Cursor | Self::Muse => None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Usage {
    /// Percent of quota consumed, 0..100, when a precise number is known.
    #[serde(rename = "usedPct")]
    pub used_percentage: Option<f64>,
    /// Short human label for the meter (e.g. a plan/tier name or a summary).
    pub label: String,
    /// When the quota window resets, as an ISO-8601 string, when known.
    pub resets_at: Option<String>,
    /// Where the figure came from - so the UI can be honest about it.
    pub source: String,
}

/// Remaining usage for `agent` - live account windows when the OAuth endpoint
/// is reachable, local tier label otherwise. `None` when we have no reliable
/// signal for that agent (the UI then shows "usage -").
// `async` + `spawn_blocking`: a synchronous command runs on the MAIN thread, and
// a cache miss here is a real network request (curl, up to 12s) - long enough to
// stop the window pumping messages ("Not Responding"). The blocking pool keeps
// even several concurrent per-agent fetches off the async workers too.
#[tauri::command]
pub(crate) async fn usage_get(agent: String) -> Option<Usage> {
    tauri::async_runtime::spawn_blocking(move || usage_for(&agent))
        .await
        .ok()
        .flatten()
}

/// The synchronous adapter dispatch behind [`usage_get`]: the live account
/// windows when they can be read, else the adapter's offline plan label.
fn usage_for(agent: &str) -> Option<Usage> {
    let agent = UsageAgent::from_id(agent)?;
    let Some(account) = account_usage_for(AccountUsageRequest::cached(agent)).account else {
        return agent.offline_usage();
    };
    Some(usage_from_account(account, agent.headline_window()))
}

/// Which window becomes the meter's headline when the account carries no weekly
/// cap. Claude's endpoint always returns a weekly window, so its meter shows the
/// weekly figure or nothing; every other adapter falls back to the most-consumed
/// window so a session-only or per-feature account still surfaces a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HeadlineWindow {
    WeeklyOnly,
    WeeklyOrMostConsumed,
}

/// The headline figure for an account's meter, mapping whatever windows the
/// endpoint returned into the flat [`Usage`] the handoff logic reads: the weekly
/// all-models cap when present, else - per `headline` - the most-consumed window,
/// else no number at all (a tier-label account with an empty window list). The one
/// home for the windows → [`Usage`] mapping every live and tier-label adapter
/// reads through (DRY).
pub(super) fn usage_from_account(account: AccountUsage, headline: HeadlineWindow) -> Usage {
    let chosen = account
        .windows
        .iter()
        .find(|window| window.kind == UsageWindowKind::Weekly)
        .or_else(|| match headline {
            HeadlineWindow::WeeklyOnly => None,
            HeadlineWindow::WeeklyOrMostConsumed => account
                .windows
                .iter()
                .max_by(|first, second| first.utilization.total_cmp(&second.utilization)),
        });
    let (used_percentage, resets_at) = chosen.map_or((None, None), |window| {
        (Some(window.utilization), window.resets_at.clone())
    });
    Usage {
        used_percentage,
        label: account.plan,
        resets_at,
        source: account.source,
    }
}

/// The `source` naming a figure read from a local file under the home directory.
/// Derived from the same relative path the file is read from, so path and label
/// can never drift.
pub(super) fn local_source(relative_path: &str) -> String {
    format!("local:{relative_path}")
}

/// A plan label: the vendor's product, then its tier when the account names one.
pub(super) fn plan_label(product: &str, tier: Option<&str>) -> String {
    tier.map_or_else(|| product.to_string(), |tier| format!("{product} {tier}"))
}

pub(super) const CLAUDE_PLAN: &str = "Claude";
pub(super) const CODEX_PLAN: &str = "Codex";
pub(super) const COPILOT_PLAN: &str = "Copilot";
pub(super) const ANTIGRAVITY_PLAN: &str = "Antigravity";
pub(super) const CURSOR_PLAN: &str = "Cursor";

/// How long one reading of a usage endpoint is reused. They are rate-limited per
/// token and safe at ~180s intervals, so every adapter (and the limit-reset
/// offer, read off the same endpoint) caches its answer this long.
pub(crate) const USAGE_CACHE_TTL: Duration = Duration::from_secs(170);
/// How long one usage read may take before curl gives up.
pub(super) const USAGE_REQUEST_TIMEOUT_SECONDS: u32 = 12;

#[derive(Debug, Clone, Copy)]
enum UsageCachePolicy {
    UseCache,
    Refresh,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct AccountUsageRequest {
    agent: UsageAgent,
    cache_policy: UsageCachePolicy,
}

impl AccountUsageRequest {
    const fn cached(agent: UsageAgent) -> Self {
        Self {
            agent,
            cache_policy: UsageCachePolicy::UseCache,
        }
    }

    const fn refresh(agent: UsageAgent) -> Self {
        Self {
            agent,
            cache_policy: UsageCachePolicy::Refresh,
        }
    }
}

/// Live account usage windows for a specific agent (`claude`, `codex`) - what the
/// per-agent meter renders and the auto-resume scheduler reads - and, when the
/// latest ask brought none, why: no adapter for this agent, signed out, a
/// rejected or expired sign-in (until the agent's next run refreshes it), or the
/// vendor unreachable. A recent reading keeps standing in through a failure.
// `async` + `spawn_blocking` for the same reason as [`usage_get`]: a cache miss
// is a bounded-but-slow curl request that must never run on the UI thread.
#[tauri::command]
pub(crate) async fn usage_account_agent(agent: String, force_refresh: bool) -> AccountReading {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(agent) = UsageAgent::from_id(&agent) else {
            return AccountReading::no_source();
        };
        let request = if force_refresh {
            AccountUsageRequest::refresh(agent)
        } else {
            AccountUsageRequest::cached(agent)
        };
        account_usage_for(request)
    })
    .await
    .unwrap_or(AccountReading {
        account: None,
        no_reading: Some(NoReading {
            reason: NoReadingReason::Unreachable,
            sign_in_command: None,
        }),
    })
}

/// How long a reading is still shown after the endpoint stops answering. A
/// refused or failed ask (a 429 with a multi-minute `retry-after`, a token that
/// expired while every Claude session sat idle, a dropped network) keeps the
/// last real reading on screen instead of blanking the meter - its windows past
/// their reset already read as 0% in the frontend - but a reading this old is
/// no longer worth showing as current.
pub(super) const LAST_READING_LIMIT: Duration = Duration::from_hours(1);

/// What the cache knows about one agent's account: when its endpoint was last
/// asked, the last reading it answered, and why the latest ask failed if it
/// did. A failed ask moves `asked_at` but keeps the reading, so a refusal
/// neither blanks the meter nor invites an immediate retry that only extends
/// the refusal.
#[derive(Clone, Default)]
pub(super) struct AccountReadings {
    asked_at: Option<Instant>,
    last_reading: Option<(Instant, AccountUsage)>,
    last_failure: Option<NoReadingReason>,
}

impl AccountReadings {
    /// Whether a request made at `requested_at` must ask the endpoint, or can be
    /// answered from what is held. Someone asking after the request was made (a
    /// concurrent caller that held the fetch gate first - another window, or the
    /// scheduler) settles it either way, so a burst of misses, forced or not,
    /// costs one request; otherwise a cached request reuses any ask, failed or
    /// not, younger than [`USAGE_CACHE_TTL`].
    fn needs_asking(
        &self,
        request: AccountUsageRequest,
        requested_at: Instant,
        now: Instant,
    ) -> bool {
        let Some(asked_at) = self.asked_at else {
            return true;
        };
        let was_asked_since_request = asked_at >= requested_at;
        let is_cache_allowed = matches!(request.cache_policy, UsageCachePolicy::UseCache);
        let is_ask_fresh = now.saturating_duration_since(asked_at) < USAGE_CACHE_TTL;
        !(was_asked_since_request || (is_cache_allowed && is_ask_fresh))
    }

    /// Record one ask made at `asked_at`; a failed one keeps the last reading
    /// and remembers why it failed.
    fn record(&mut self, answer: Result<AccountUsage, NoReadingReason>, asked_at: Instant) {
        self.asked_at = Some(asked_at);
        match answer {
            Ok(reading) => {
                self.last_reading = Some((asked_at, reading));
                self.last_failure = None;
            }
            Err(reason) => self.last_failure = Some(reason),
        }
    }

    /// What `agent`'s account command answers at `now`: the reading to show and,
    /// after a failed ask, why it failed.
    fn reading(&self, agent: UsageAgent, now: Instant) -> AccountReading {
        AccountReading {
            account: self.last_known(now),
            no_reading: self.last_failure.map(|reason| NoReading {
                reason,
                sign_in_command: crate::agents::command_for_id(agent.sign_in_agent()),
            }),
        }
    }

    /// The last reading, while it is younger than [`LAST_READING_LIMIT`].
    fn last_known(&self, now: Instant) -> Option<AccountUsage> {
        let (read_at, reading) = self.last_reading.as_ref()?;
        let is_recent = now.saturating_duration_since(*read_at) < LAST_READING_LIMIT;
        is_recent.then(|| reading.clone())
    }
}

type AccountUsageCache = Mutex<HashMap<UsageAgent, AccountReadings>>;

/// The one cache of live account readings, keyed by agent - the single source
/// for both commands, so a meter read and a scheduler read share one request.
fn account_usage_cache() -> &'static AccountUsageCache {
    static CACHE: OnceLock<AccountUsageCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A copy of what the cache holds for `agent`, taken under a short lock.
fn account_readings(agent: UsageAgent) -> AccountReadings {
    account_usage_cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&agent)
        .cloned()
        .unwrap_or_default()
}

type FetchGates = Mutex<HashMap<UsageAgent, Arc<Mutex<()>>>>;

/// `agent`'s fetch gate: held across one endpoint ask, so concurrent misses
/// (every open window polls, and all of them force a refresh at the same reset
/// boundary) wait for that one ask instead of each firing their own - a burst
/// the rate-limited endpoint answers with 429s. Per agent, so one slow vendor
/// never holds up another.
fn fetch_gate(agent: UsageAgent) -> Arc<Mutex<()>> {
    static GATES: OnceLock<FetchGates> = OnceLock::new();
    let gates = GATES.get_or_init(|| Mutex::new(HashMap::new()));
    Arc::clone(
        gates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(agent)
            .or_default(),
    )
}

/// Live account usage for `agent`, cached per agent to respect each endpoint's
/// per-token limit, with concurrent asks coalesced into one and a failed ask
/// answered by the last real reading (see [`AccountReadings`]). The cache lock
/// is only ever held for a lookup; the ask itself runs under the agent's gate.
fn account_usage_for(request: AccountUsageRequest) -> AccountReading {
    let requested_at = Instant::now();
    let readings = account_readings(request.agent);
    if !readings.needs_asking(request, requested_at, Instant::now()) {
        return readings.reading(request.agent, Instant::now());
    }

    let gate = fetch_gate(request.agent);
    let _asking = gate.lock().unwrap_or_else(PoisonError::into_inner);
    let readings = account_readings(request.agent);
    if !readings.needs_asking(request, requested_at, Instant::now()) {
        return readings.reading(request.agent, Instant::now());
    }

    let answer = request.agent.fetch_account_usage();
    let mut cache = account_usage_cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let readings = cache.entry(request.agent).or_default();
    readings.record(answer, Instant::now());
    readings.reading(request.agent, Instant::now())
}

/// Where each adapter's live figures come from, for the meter's `source`.
pub(super) const CLAUDE_OAUTH_SOURCE: &str = "oauth:api.anthropic.com";
pub(super) const CODEX_OAUTH_SOURCE: &str = "oauth:chatgpt.com";
pub(super) const COPILOT_OAUTH_SOURCE: &str = "oauth:api.github.com";
