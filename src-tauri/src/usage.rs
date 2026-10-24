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

pub(crate) mod limit_reset;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::agents::{ID_ANTIGRAVITY, ID_CLAUDE, ID_CODEX, ID_COPILOT, ID_CURSOR, ID_OPENCODE};
use crate::util::{home_directory, now_milliseconds, read_json};

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
/// An agent id and an adapter aren't 1:1: opencode signs into the same
/// `ChatGPT` subscription Codex does, so its id resolves to the Codex adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UsageAgent {
    Claude,
    Codex,
    Copilot,
    Antigravity,
    Cursor,
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
            _ => None,
        }
    }

    /// Which window heads this adapter's meter - see [`HeadlineWindow`].
    fn headline_window(self) -> HeadlineWindow {
        match self {
            Self::Claude => HeadlineWindow::WeeklyOnly,
            Self::Codex | Self::Copilot | Self::Antigravity | Self::Cursor => {
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
        }
    }

    /// The honest plan label the meter falls back to when the live account can't
    /// be read (offline, or the token expired). The other adapters already fall
    /// back to their local login inside the account read itself.
    fn offline_usage(self) -> Option<Usage> {
        match self {
            Self::Claude => claude_tier_label_usage(),
            Self::Codex => codex_tier_label_usage(),
            Self::Copilot | Self::Antigravity | Self::Cursor => None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
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
pub async fn usage_get(agent: String) -> Option<Usage> {
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
enum HeadlineWindow {
    WeeklyOnly,
    WeeklyOrMostConsumed,
}

/// The headline figure for an account's meter, mapping whatever windows the
/// endpoint returned into the flat [`Usage`] the handoff logic reads: the weekly
/// all-models cap when present, else - per `headline` - the most-consumed window,
/// else no number at all (a tier-label account with an empty window list). The one
/// home for the windows → [`Usage`] mapping every live and tier-label adapter
/// reads through (DRY).
fn usage_from_account(account: AccountUsage, headline: HeadlineWindow) -> Usage {
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
fn local_source(relative_path: &str) -> String {
    format!("local:{relative_path}")
}

/// A plan label: the vendor's product, then its tier when the account names one.
fn plan_label(product: &str, tier: Option<&str>) -> String {
    tier.map_or_else(|| product.to_string(), |tier| format!("{product} {tier}"))
}

const CLAUDE_PLAN: &str = "Claude";
const CODEX_PLAN: &str = "Codex";
const COPILOT_PLAN: &str = "Copilot";
const ANTIGRAVITY_PLAN: &str = "Antigravity";
const CURSOR_PLAN: &str = "Cursor";

/// The local Claude Code credentials file, `~/.claude/.credentials.json`.
const CLAUDE_CREDENTIALS_RELATIVE_PATH: &str = ".claude/.credentials.json";

fn claude_credentials_path() -> Option<PathBuf> {
    Some(home_directory()?.join(CLAUDE_CREDENTIALS_RELATIVE_PATH))
}

/// Offline fallback: just the subscription tier from the local credentials, with
/// the precise numbers left `None`.
fn claude_tier_label_usage() -> Option<Usage> {
    let json = read_json(&claude_credentials_path()?)?;
    let tier = json
        .pointer("/claudeAiOauth/subscriptionType")
        .and_then(serde_json::Value::as_str)?;

    Some(Usage {
        used_percentage: None,
        label: plan_label(CLAUDE_PLAN, Some(tier)),
        resets_at: None,
        source: local_source(CLAUDE_CREDENTIALS_RELATIVE_PATH),
    })
}

// ---------------------------------------------------------------------------
// Codex usage. Codex signs in with a ChatGPT account (`codex login`), storing the
// OAuth tokens in `~/.codex/auth.json`. Its TUI reads live rate limits from the
// same ChatGPT backend endpoint (`/backend-api/wham/usage`) the Codex CLI polls;
// we call it the same honest way - a real request that spends no message quota -
// and fall back to the local plan label when the token/network isn't available.
// ---------------------------------------------------------------------------

/// The local Codex credentials file as the usage source names it
/// (`$CODEX_HOME` relocates it; see [`crate::agents::codex_home`]).
const CODEX_AUTH_RELATIVE_PATH: &str = ".codex/auth.json";

/// The credentials file's name inside Codex's home.
const CODEX_AUTH_FILE: &str = "auth.json";

fn codex_auth_path() -> Option<PathBuf> {
    Some(crate::agents::codex_home()?.join(CODEX_AUTH_FILE))
}

/// Offline fallback: confirm a local `ChatGPT` auth exists and surface a bare
/// "Codex" plan label with no numbers. We don't read the plan from the local
/// id-token - it can be stale (e.g. "free" after an upgrade to Plus) - so we
/// stay honest and leave the precise figures `None` until the endpoint answers.
fn codex_tier_label_usage() -> Option<Usage> {
    let json = read_json(&codex_auth_path()?)?;
    json.pointer("/tokens/access_token")
        .and_then(serde_json::Value::as_str)?;

    Some(Usage {
        used_percentage: None,
        label: CODEX_PLAN.to_string(),
        resets_at: None,
        source: local_source(CODEX_AUTH_RELATIVE_PATH),
    })
}

// ---------------------------------------------------------------------------
// Live account usage - every rate-limit window claude.ai / `claude /usage` shows,
// read from the OAuth endpoint using the local Claude Code token. We enumerate
// whatever windows the response carries rather than hardcoding a fixed few.
// ---------------------------------------------------------------------------

/// The claude.ai API Claude Code reads its account state from.
pub(crate) const CLAUDE_API: &str = "https://api.anthropic.com/api";
/// Where the OAuth-authenticated endpoints live under [`CLAUDE_API`].
const CLAUDE_OAUTH_PATH: &str = "/oauth";
/// The `User-Agent` the usage endpoint expects: any other one lands in an
/// aggressively rate-limited bucket.
const CLAUDE_USAGE_USER_AGENT: &str = "claude-code/1.0.128";
/// How long one reading of a usage endpoint is reused. They are rate-limited per
/// token and safe at ~180s intervals, so every adapter (and the limit-reset
/// offer, read off the same endpoint) caches its answer this long.
pub(crate) const USAGE_CACHE_TTL: Duration = Duration::from_secs(170);
/// How long one usage read may take before curl gives up.
const USAGE_REQUEST_TIMEOUT_SECONDS: u32 = 12;

/// The undocumented endpoint Claude Code's `/usage` reads.
pub(crate) fn claude_usage_url() -> String {
    format!("{CLAUDE_API}{CLAUDE_OAUTH_PATH}/usage")
}

/// The two named top-level windows we give product names to; every other named
/// window the endpoint returns is surfaced generically as
/// [`UsageWindowKind::Opaque`].
const SESSION_WINDOW_KEY: &str = "five_hour";
const WEEKLY_WINDOW_KEY: &str = "seven_day";

/// The semantic kind of a rate-limit window. The endpoint returns a handful of
/// named windows (the 5-hour `five_hour` session, the 7-day `seven_day` weekly
/// all-models cap, the `extra_usage` monthly spend cap on usage credits) plus
/// per-model caps under `limits[]`; we classify each so the meter can label it and
/// the auto-resume scheduler can match the window a CLI named. A named window we
/// don't recognize passes through as `Opaque` - surfaced honestly, never dropped.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageWindowKind {
    Session,
    Weekly,
    Model,
    Credits,
    Opaque,
}

impl UsageWindowKind {
    /// The one authoritative mapping from a top-level window key to its kind.
    fn from_window_key(key: &str) -> Self {
        match key {
            SESSION_WINDOW_KEY => Self::Session,
            WEEKLY_WINDOW_KEY => Self::Weekly,
            CREDITS_KEY => Self::Credits,
            _ => Self::Opaque,
        }
    }

    /// Stable render + match order: session, then the weekly all-models cap, then
    /// per-model caps, then the credits spend cap, then anything unrecognized.
    fn order(self) -> u8 {
        match self {
            Self::Session => 0,
            Self::Weekly => 1,
            Self::Model => 2,
            Self::Credits => 3,
            Self::Opaque => 4,
        }
    }
}

/// One rate-limit window surfaced from the account response: a stable `key`, its
/// semantic `kind`, a human `label`, the 0..100 `utilization` percent, and an
/// ISO-8601 reset time when known. The generic shape the meter renders and the
/// auto-resume scheduler matches against - one entry per window the endpoint
/// actually returns, nothing invented.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    key: String,
    kind: UsageWindowKind,
    label: String,
    utilization: f64,
    resets_at: Option<String>,
}

/// The top-level key under which Claude's account response describes its
/// pay-as-you-go usage credits ("extra usage").
const CREDITS_KEY: &str = "extra_usage";
/// A window or cap consumed this far has nothing left - also the scale every
/// `utilization` is on.
const FULL_PERCENTAGE: f64 = 100.0;

/// The state of pay-as-you-go usage credits ("extra usage"). The auto-resume
/// scheduler reads it to continue a limit-stopped session the moment credits can
/// carry it - turned on server-side, which the CLI itself never notices.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreditsState {
    /// Turned off.
    Off,
    /// Turned on, with room left under the monthly cap (or no cap at all).
    Available,
    /// Turned on, but unable to carry work now: the monthly cap is spent, or the
    /// account names a reason they're held back.
    Unavailable,
}

/// The credits state an account response describes, or `None` when it carries no
/// credits object at all (every adapter but Claude's, or a Claude response that
/// omits it).
fn credits_state(response: &serde_json::Value) -> Option<CreditsState> {
    let credits = response.get(CREDITS_KEY)?;
    let is_enabled = credits
        .get("is_enabled")
        .and_then(serde_json::Value::as_bool)?;
    if !is_enabled {
        return Some(CreditsState::Off);
    }

    let is_cap_spent = credits
        .get("utilization")
        .and_then(serde_json::Value::as_f64)
        .is_some_and(|utilization| utilization >= FULL_PERCENTAGE);
    let is_spend_limit_reached = credits
        .get("spend_limit_reached")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let is_held_back = credits
        .get("disabled_reason")
        .is_some_and(serde_json::Value::is_string);
    if is_cap_spent || is_spend_limit_reached || is_held_back {
        return Some(CreditsState::Unavailable);
    }
    Some(CreditsState::Available)
}

/// An amount of usage credits - a prepaid balance, or a month's spend - ready
/// to show.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreditBalance {
    /// The amount in the currency's major units (dollars, not cents).
    amount: f64,
    /// The ISO 4217 currency code (`USD`).
    currency: String,
}

/// Stable identities for the underlying billing account behind each adapter, so
/// the frontend can dedupe agents that share one subscription (Codex + opencode
/// both bill the same `ChatGPT` account) - one identity per account, never per
/// agent.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BillingAccount {
    Anthropic,
    Chatgpt,
    Github,
    Google,
    Cursor,
}

/// Live account usage mirrored from the OAuth endpoint: every rate-limit window it
/// returns (the session + weekly windows, any per-model caps, and any future
/// windows), plus the plan label. Its `Default` is the shape of an account with
/// no credits concept, which every adapter but Claude's builds on.
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsage {
    windows: Vec<UsageWindow>,
    plan: String,
    source: String,
    /// Stable identity of the underlying billing account, so two agents on one
    /// subscription are never counted twice. `None` when the adapter can't name
    /// the account.
    account: Option<BillingAccount>,
    /// Whether usage credits are on and can carry work past an exhausted window.
    /// `None` for an account with no credits concept.
    credits: Option<CreditsState>,
    /// The prepaid usage-credits balance. Read only while credits are on, so it is
    /// present exactly when extra usage is enabled (and the balance was reachable).
    credit_balance: Option<CreditBalance>,
    /// What usage credits have cost this month when no monthly cap is set. With
    /// no cap there is no percentage to show, so the meter shows this instead.
    uncapped_spend: Option<CreditBalance>,
}

#[derive(Debug, Clone, Copy)]
enum UsageCachePolicy {
    UseCache,
    Refresh,
}

/// Why an account has no reading - so the UI can say "sign-in expired" instead
/// of a bare dash, and tell the user what to do about it.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NoReadingReason {
    /// PADE has no usage source it can read for this agent.
    NoSource,
    /// The agent's sign-in isn't on disk.
    SignedOut,
    /// The vendor rejected the stored sign-in (a 401/403), or its recorded
    /// expiry has passed. The agent's CLI refreshes it on its next run.
    SignInExpired,
    /// The vendor didn't answer usably: no network, a refusal (429), a server
    /// error, or a body that isn't a usage response. Transient, so the last
    /// reading keeps standing in.
    Unreachable,
}

/// A failed usage request, as far as the reading cares: the vendor refused the
/// credentials, or it could not be reached / did not answer usably.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestFailure {
    Rejected,
    Unreachable,
}

impl From<RequestFailure> for NoReadingReason {
    fn from(failure: RequestFailure) -> Self {
        match failure {
            RequestFailure::Rejected => Self::SignInExpired,
            RequestFailure::Unreachable => Self::Unreachable,
        }
    }
}

/// Why the latest ask brought no reading, and the CLI that fixes a sign-in
/// problem (`codex`), straight from the agent registry.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NoReading {
    reason: NoReadingReason,
    sign_in_command: Option<&'static str>,
}

/// What the account command answers: the reading to show - fresh, or the last
/// good one held over while the vendor fails (see [`LAST_READING_LIMIT`]) - and,
/// when the latest ask brought none, why. A fresh reading carries no
/// `no_reading`; a held-over one carries both.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AccountReading {
    account: Option<AccountUsage>,
    no_reading: Option<NoReading>,
}

impl AccountReading {
    /// The answer for an agent PADE has no usage adapter for.
    const fn no_source() -> Self {
        Self {
            account: None,
            no_reading: Some(NoReading {
                reason: NoReadingReason::NoSource,
                sign_in_command: None,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AccountUsageRequest {
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
pub async fn usage_account_agent(agent: String, force_refresh: bool) -> AccountReading {
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
const LAST_READING_LIMIT: Duration = Duration::from_hours(1);

/// What the cache knows about one agent's account: when its endpoint was last
/// asked, the last reading it answered, and why the latest ask failed if it
/// did. A failed ask moves `asked_at` but keeps the reading, so a refusal
/// neither blanks the meter nor invites an immediate retry that only extends
/// the refusal.
#[derive(Clone, Default)]
struct AccountReadings {
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

/// A credentials file an adapter reads, parsed - or [`NoReadingReason::SignedOut`]
/// when it is missing or unreadable, which is what a CLI that never signed in
/// (or signed out) leaves behind.
fn read_sign_in(path: Option<PathBuf>) -> Result<serde_json::Value, NoReadingReason> {
    path.as_deref()
        .and_then(read_json)
        .ok_or(NoReadingReason::SignedOut)
}

/// A string the sign-in must carry (the token, the account id); its absence
/// means there is no usable sign-in.
fn sign_in_field<'json>(
    json: &'json serde_json::Value,
    pointer: &str,
) -> Result<&'json str, NoReadingReason> {
    json.pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(NoReadingReason::SignedOut)
}

/// A usage endpoint's body as JSON. One that doesn't parse is not a usage
/// response - the vendor didn't answer usably.
fn parse_usage_body(body: &str) -> Result<serde_json::Value, NoReadingReason> {
    serde_json::from_str(body).map_err(|_| NoReadingReason::Unreachable)
}

/// Claude Code's stored OAuth login, while its token has not expired - a
/// request on a token already known to be stale is wasted.
fn live_claude_oauth() -> Result<serde_json::Value, NoReadingReason> {
    let mut json = read_sign_in(claude_credentials_path())?;
    let oauth = json
        .get_mut("claudeAiOauth")
        .ok_or(NoReadingReason::SignedOut)?
        .take();
    let expires_at_milliseconds = oauth.get("expiresAt").and_then(serde_json::Value::as_u64);
    let has_expired =
        expires_at_milliseconds.is_some_and(|expires_at| expires_at <= now_milliseconds());
    if has_expired {
        return Err(NoReadingReason::SignInExpired);
    }

    Ok(oauth)
}

/// Claude Code's OAuth access token, while it is still live.
pub(crate) fn claude_access_token() -> Option<String> {
    live_claude_oauth()
        .ok()?
        .get("accessToken")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn fetch_claude_account_usage() -> Result<AccountUsage, NoReadingReason> {
    let oauth = live_claude_oauth()?;
    let token = sign_in_field(&oauth, "/accessToken")?;

    let plan = plan_label(
        CLAUDE_PLAN,
        oauth
            .get("subscriptionType")
            .and_then(serde_json::Value::as_str),
    );

    let body = read_claude_usage_endpoint(&ClaudeUsageRead {
        url: &claude_usage_url(),
        token,
        organization: None,
    })?;
    let response = parse_usage_body(&body)?;

    let windows = collect_windows(&response);
    // A body that carries no windows isn't a usage response.
    if windows.is_empty() {
        return Err(NoReadingReason::Unreachable);
    }

    let credits = credits_state(&response);
    let is_credits_on = credits.is_some_and(|state| state != CreditsState::Off);
    let credit_balance = is_credits_on
        .then(|| claude_credit_balance(token))
        .flatten();

    Ok(AccountUsage {
        windows,
        plan,
        source: CLAUDE_OAUTH_SOURCE.to_string(),
        account: Some(BillingAccount::Anthropic),
        credits,
        credit_balance,
        uncapped_spend: uncapped_credits_spend(&response),
    })
}

/// Where each adapter's live figures come from, for the meter's `source`.
const CLAUDE_OAUTH_SOURCE: &str = "oauth:api.anthropic.com";
const CODEX_OAUTH_SOURCE: &str = "oauth:chatgpt.com";
const COPILOT_OAUTH_SOURCE: &str = "oauth:api.github.com";

/// Claude Code's profile, `~/.claude.json`, which records the signed-in account's
/// organization.
const CLAUDE_PROFILE_RELATIVE_PATH: &str = ".claude.json";

/// Claude Code's profile, parsed once. The file also carries every project's
/// history and grows large, so a caller needing several fields reads it once
/// and asks this rather than re-reading per field.
pub(crate) struct ClaudeProfile(serde_json::Value);

impl ClaudeProfile {
    /// The profile on disk, or `None` when Claude Code has not written one.
    pub(crate) fn read() -> Option<Self> {
        read_json(&home_directory()?.join(CLAUDE_PROFILE_RELATIVE_PATH)).map(Self)
    }

    fn field(&self, pointer: &str) -> Option<&str> {
        self.0.pointer(pointer).and_then(serde_json::Value::as_str)
    }

    /// The signed-in Claude account's organization uuid.
    pub(crate) fn organization(&self) -> Option<&str> {
        self.field("/oauthAccount/organizationUuid")
    }

    /// The installed Claude Code's version, as the profile last recorded it (the
    /// release notes it last showed, which it shows on every update).
    pub(crate) fn cli_version(&self) -> Option<&str> {
        self.field("/lastReleaseNotesSeen")
    }
}

/// The organization's prepaid usage-credits balance, from the endpoint Claude
/// Code's `/usage-credits` reads. Only asked for while credits are on.
fn claude_credit_balance(token: &str) -> Option<CreditBalance> {
    let profile = ClaudeProfile::read()?;
    let organization = profile.organization()?;
    let url =
        format!("{CLAUDE_API}{CLAUDE_OAUTH_PATH}/organizations/{organization}/prepaid/credits");
    let body = read_claude_usage_endpoint(&ClaudeUsageRead {
        url: &url,
        token,
        organization: Some(organization),
    })
    .ok()?;
    credit_balance(&serde_json::from_str(&body).ok()?)
}

/// Where a prepaid balance response states its amount - in money, or in credits.
const BALANCE_KINDS: [&str; 2] = ["money", "credits"];

/// The prepaid balance a `prepaid/credits` response describes: its minor-unit
/// amount scaled by its own exponent, in its currency. `None` for any other shape.
fn credit_balance(response: &serde_json::Value) -> Option<CreditBalance> {
    let currency = response
        .get("currency")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    let balance = response.get("balance")?;
    let amount = BALANCE_KINDS.iter().find_map(|kind| {
        let entry = balance.get(kind).filter(|entry| !entry.is_null())?;
        let minor_units = i32::try_from(entry.get("amount_minor")?.as_i64()?).ok()?;
        let exponent = i32::try_from(entry.get("exponent")?.as_u64()?).ok()?;
        Some(major_units(f64::from(minor_units), exponent))
    })?;
    Some(CreditBalance { amount, currency })
}

/// An amount in minor units (cents) as major units (dollars), scaled by the
/// currency's own `exponent` of ten.
fn major_units(minor_units: f64, exponent: i32) -> f64 {
    const DECIMAL_BASE: f64 = 10.0;
    minor_units / DECIMAL_BASE.powi(exponent)
}

/// A string field of a JSON object, owned.
fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// What usage credits have cost this month, when they are on with no monthly
/// cap: `used_credits` in minor units, scaled by the response's own
/// `decimal_places`. `None` while a cap is set - its percentage says it - and
/// for any other shape.
fn uncapped_credits_spend(response: &serde_json::Value) -> Option<CreditBalance> {
    let credits = response.get(CREDITS_KEY)?;
    let is_enabled = credits.get("is_enabled")?.as_bool()?;
    let has_cap = !credits.get("monthly_limit")?.is_null();
    if !is_enabled || has_cap {
        return None;
    }

    // Sent as a float ("29025.0"), though it counts whole minor units.
    let minor_units = credits.get("used_credits")?.as_f64()?;
    let exponent = i32::try_from(credits.get("decimal_places")?.as_u64()?).ok()?;
    let currency = credits.get("currency")?.as_str()?.to_string();
    Some(CreditBalance {
        amount: major_units(minor_units, exponent),
        currency,
    })
}

/// Every rate-limit window the account response carries, in a stable order: the
/// named top-level windows (`five_hour`, `seven_day`, and any others the endpoint
/// adds) followed by the per-model caps under `limits[]`. Whatever the API returns
/// - nothing is invented.
fn collect_windows(response: &serde_json::Value) -> Vec<UsageWindow> {
    let mut windows: Vec<UsageWindow> = response
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| named_window(key, value))
        .chain(model_windows(response))
        .collect();
    windows.sort_by_key(|window| window.kind.order());
    windows
}

/// A top-level entry as a window when it is window-shaped - an object carrying a
/// numeric `utilization`. Non-window fields (`limits`, the plan, an error blob)
/// have no `utilization`, so they fall away here.
fn named_window(key: &str, value: &serde_json::Value) -> Option<UsageWindow> {
    let utilization = value
        .get("utilization")
        .and_then(serde_json::Value::as_f64)?;
    Some(UsageWindow {
        key: key.to_string(),
        kind: UsageWindowKind::from_window_key(key),
        label: humanize_key(key),
        utilization,
        resets_at: string_field(value, "resets_at"),
    })
}

/// The per-model caps under `limits[]` - each row scoped to a named model
/// (`scope.model.display_name`), which both identifies it as a per-model window
/// and skips the unscoped session/weekly rows the array may also carry. Every
/// window is surfaced as-is - the frontend (`usage-groups.ts`) shows them all,
/// including any still at 0%.
fn model_windows(response: &serde_json::Value) -> Vec<UsageWindow> {
    response
        .get("limits")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(model_window)
        .collect()
}

/// One `limits[]` row as a per-model window, when it is scoped to a named model.
fn model_window(row: &serde_json::Value) -> Option<UsageWindow> {
    let utilization = row.get("percent").and_then(serde_json::Value::as_f64)?;
    let name = row
        .pointer("/scope/model/display_name")
        .and_then(serde_json::Value::as_str)?;
    Some(UsageWindow {
        key: name.to_string(),
        kind: UsageWindowKind::Model,
        label: name.to_string(),
        utilization,
        resets_at: string_field(row, "resets_at"),
    })
}

/// A human label for a named window, sentence-cased from its `snake_case` key
/// (`seven_day` → "Seven day") - the fallback the frontend shows for a window
/// whose kind it gives no product name.
fn humanize_key(key: &str) -> String {
    let mut label = key.replace('_', " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}

/// Whether `token` is safe to interpolate into a quoted `header = "…"` directive
/// of a curl `--config` file.
///
/// The token is spliced verbatim inside the double quotes, so a stray quote or
/// newline could close the string and inject an arbitrary curl directive
/// (config-file directive injection). We therefore accept only the OAuth-token
/// character set - non-empty, ASCII alphanumerics plus `.`, `_`, `-` - and
/// reject everything else.
pub(crate) fn is_safe_header_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

const AUTHORIZATION_HEADER: &str = "Authorization";
const USER_AGENT_HEADER: &str = "User-Agent";
/// The beta flag every claude.ai OAuth endpoint requires.
const CLAUDE_OAUTH_BETA: &str = "oauth-2025-04-20";

/// One request to a claude.ai OAuth endpoint, made the way Claude Code makes it:
/// the bearer token, the `User-Agent` that endpoint expects, and - for an
/// organization-scoped endpoint - the organization uuid it is addressed to. A
/// `json_body` turns it into a POST.
pub(crate) struct ClaudeOauthRequest<'request> {
    pub(crate) url: &'request str,
    pub(crate) token: &'request str,
    pub(crate) user_agent: &'request str,
    pub(crate) organization: Option<&'request str>,
    pub(crate) json_body: Option<&'request str>,
    pub(crate) timeout_seconds: u32,
}

/// Call a claude.ai OAuth endpoint via `curl`. The token and organization uuid
/// are interpolated into quoted config directives, so anything outside the
/// OAuth-token character set is refused (as a rejected sign-in) - see
/// [`is_safe_header_token`].
pub(crate) fn curl_claude_oauth(request: &ClaudeOauthRequest) -> Result<String, RequestFailure> {
    let ClaudeOauthRequest {
        url,
        token,
        user_agent,
        organization,
        json_body,
        timeout_seconds,
    } = *request;
    let is_organization_safe = organization.is_none_or(is_safe_header_token);
    if !is_safe_header_token(token) || !is_organization_safe {
        return Err(RequestFailure::Rejected);
    }

    let authorization = format!("Bearer {token}");
    let mut headers = vec![
        (AUTHORIZATION_HEADER, authorization.as_str()),
        ("anthropic-beta", CLAUDE_OAUTH_BETA),
        (USER_AGENT_HEADER, user_agent),
        ("Content-Type", "application/json"),
    ];
    if let Some(organization) = organization {
        headers.push(("x-organization-uuid", organization));
    }
    curl_json(&CurlRequest {
        url,
        timeout_seconds,
        headers: &headers,
        json_body,
    })
}

/// One read of a claude.ai usage endpoint, as Claude Code's `/usage` makes it.
struct ClaudeUsageRead<'request> {
    url: &'request str,
    token: &'request str,
    organization: Option<&'request str>,
}

fn read_claude_usage_endpoint(read: &ClaudeUsageRead) -> Result<String, RequestFailure> {
    curl_claude_oauth(&ClaudeOauthRequest {
        url: read.url,
        token: read.token,
        user_agent: CLAUDE_USAGE_USER_AGENT,
        organization: read.organization,
        json_body: None,
        timeout_seconds: USAGE_REQUEST_TIMEOUT_SECONDS,
    })
}

/// One HTTP request, described as a curl `--config` document.
struct CurlRequest<'request> {
    url: &'request str,
    timeout_seconds: u32,
    /// `(name, value)` pairs, each already checked safe to quote.
    headers: &'request [(&'request str, &'request str)],
    /// A JSON body, which turns the request into a POST.
    json_body: Option<&'request str>,
}

/// The curl `--config` document for `request`. `write-out` appends the HTTP
/// status on a final line of its own, so [`http_answer`] can tell a rejected
/// sign-in from a refusal and never reads an error body as an answer.
fn curl_config(request: &CurlRequest) -> String {
    let CurlRequest {
        url,
        timeout_seconds,
        headers,
        json_body,
    } = request;
    let mut config = format!(
        "silent\nshow-error\nwrite-out = \"\\n%{{http_code}}\"\nmax-time = {timeout_seconds}\nurl = \"{url}\"\n"
    );
    // Writing into a `String` cannot fail, so the `fmt::Result`s carry nothing.
    for (name, value) in *headers {
        let _ = writeln!(config, "header = \"{name}: {value}\"");
    }
    if let Some(body) = json_body {
        let quoted = body.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = write!(config, "request = \"POST\"\ndata = \"{quoted}\"\n");
    }
    config
}

/// Run `request` by feeding its curl `--config` document on stdin - so the
/// bearer token never appears in the process arguments. The one transport
/// behind every vendor's usage call and the limit-reset calls.
fn curl_json(request: &CurlRequest) -> Result<String, RequestFailure> {
    let config = curl_config(request);
    let mut child = crate::util::command("curl")
        .arg("--config")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| RequestFailure::Unreachable)?;

    // Write the config, then drop stdin so curl sees EOF and runs the request.
    child
        .stdin
        .take()
        .ok_or(RequestFailure::Unreachable)?
        .write_all(config.as_bytes())
        .map_err(|_| RequestFailure::Unreachable)?;

    let output = child
        .wait_with_output()
        .map_err(|_| RequestFailure::Unreachable)?;
    // A transport failure (no network, a timeout) exits non-zero.
    if !output.status.success() {
        return Err(RequestFailure::Unreachable);
    }
    let stdout = String::from_utf8(output.stdout).map_err(|_| RequestFailure::Unreachable)?;
    http_answer(&stdout)
}

/// HTTP statuses that mean the vendor refused the credentials themselves.
const UNAUTHORIZED_STATUS: u16 = 401;
const FORBIDDEN_STATUS: u16 = 403;

/// curl's output split into the body and the status `write-out` appended on the
/// last line: the body on a success, otherwise what the status says - a 401 or
/// 403 is a rejected sign-in, anything else (a 429, a server error, no status
/// at all) leaves the vendor unreachable for now.
fn http_answer(output: &str) -> Result<String, RequestFailure> {
    let (body, status) = output.rsplit_once('\n').unwrap_or(("", output));
    let status: u16 = status
        .trim()
        .parse()
        .map_err(|_| RequestFailure::Unreachable)?;
    match status {
        200..=299 => Ok(body.to_string()),
        UNAUTHORIZED_STATUS | FORBIDDEN_STATUS => Err(RequestFailure::Rejected),
        _ => Err(RequestFailure::Unreachable),
    }
}

// ---------------------------------------------------------------------------
// Codex live account usage - the ChatGPT rate-limit windows the Codex CLI reads
// from `/backend-api/wham/usage` with the locally-stored ChatGPT OAuth token.
// ---------------------------------------------------------------------------

/// The `ChatGPT` backend endpoint the Codex CLI (`codex-rs/backend-client`) polls
/// for rate limits - it sends the `ChatGPT` `Bearer` token and the account id, and
/// answers whether or not any quota is spent, so it's safe at the same ~180s
/// cadence as the Claude endpoint.
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
/// A Codex-CLI-shaped User-Agent, matching what `get_codex_user_agent()` sends.
const CODEX_USAGE_USER_AGENT: &str = "codex_cli_rs/0.55.0";

/// The `ChatGPT` login Codex stores: the bearer token and the account it acts for.
struct CodexLogin<'login> {
    token: &'login str,
    account_id: &'login str,
}

/// Codex's two account-wide rate-limit window durations, used to classify each
/// window the endpoint returns into the same session/weekly kinds the meter and
/// the auto-resume scheduler understand. Any other duration passes through as
/// [`UsageWindowKind::Opaque`].
const CODEX_SESSION_WINDOW_SECONDS: i64 = 5 * SECONDS_PER_HOUR;
const CODEX_WEEKLY_WINDOW_SECONDS: i64 = DAYS_PER_WEEK * SECONDS_PER_DAY;

const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;
const DAYS_PER_WEEK: i64 = 7;

/// The feature name a Codex per-feature limit falls back to when it names none.
const CODEX_UNNAMED_FEATURE: &str = "codex";

fn fetch_codex_account_usage() -> Result<AccountUsage, NoReadingReason> {
    let json = read_sign_in(codex_auth_path())?;
    let token = sign_in_field(&json, "/tokens/access_token")?;
    let account_id = sign_in_field(&json, "/tokens/account_id")?;

    let body = read_codex_usage_endpoint(&CodexLogin { token, account_id })?;
    let response = parse_usage_body(&body)?;

    let windows = collect_codex_windows(&response);
    // A body that carries no windows isn't a usage response.
    if windows.is_empty() {
        return Err(NoReadingReason::Unreachable);
    }

    let plan = plan_label(
        CODEX_PLAN,
        response
            .get("plan_type")
            .and_then(serde_json::Value::as_str),
    );
    Ok(AccountUsage {
        windows,
        plan,
        source: CODEX_OAUTH_SOURCE.to_string(),
        account: Some(BillingAccount::Chatgpt),
        ..AccountUsage::default()
    })
}

/// Every rate-limit window the Codex usage response carries: the account-wide
/// `rate_limit` primary/secondary caps (classified by duration into session /
/// weekly / opaque), then any per-feature `additional_rate_limits`, each surfaced
/// opaquely under its own feature name. Ordered like the Claude windows.
fn collect_codex_windows(response: &serde_json::Value) -> Vec<UsageWindow> {
    let account_wide = response
        .get("rate_limit")
        .into_iter()
        .flat_map(|rate_limit| codex_rate_limit_windows(rate_limit, None));
    let per_feature = response
        .get("additional_rate_limits")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|entry| {
            let feature = entry
                .get("limit_name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(CODEX_UNNAMED_FEATURE);
            entry
                .get("rate_limit")
                .into_iter()
                .flat_map(move |rate_limit| codex_rate_limit_windows(rate_limit, Some(feature)))
        });
    let mut windows: Vec<UsageWindow> = account_wide.chain(per_feature).collect();
    windows.sort_by_key(|window| window.kind.order());
    windows
}

/// The `primary_window` / `secondary_window` of one `rate_limit` object as usage
/// windows. `feature` is `None` for the account-wide caps (classified by their
/// duration) and `Some(name)` for a named metered feature (surfaced opaquely,
/// so a 5-hour code-review cap isn't mislabeled the account "Session").
fn codex_rate_limit_windows<'response>(
    rate_limit: &'response serde_json::Value,
    feature: Option<&'response str>,
) -> impl Iterator<Item = UsageWindow> + 'response {
    ["primary_window", "secondary_window"]
        .into_iter()
        .filter_map(move |slot| {
            rate_limit
                .get(slot)
                .and_then(|window| codex_window(window, feature))
        })
}

/// One Codex rate-limit window (a `{ used_percent, limit_window_seconds, reset_at }`
/// object) as a [`UsageWindow`], or `None` when it isn't window-shaped (e.g. a
/// `null` secondary window). `reset_at` is a Unix timestamp, normalized to the
/// ISO-8601 string the frontend's countdown parses.
fn codex_window(value: &serde_json::Value, feature: Option<&str>) -> Option<UsageWindow> {
    let utilization = value
        .get("used_percent")
        .and_then(serde_json::Value::as_f64)?;
    let seconds = value
        .get("limit_window_seconds")
        .and_then(serde_json::Value::as_i64)?;
    let resets_at = value
        .get("reset_at")
        .and_then(serde_json::Value::as_i64)
        .map(unix_seconds_to_iso8601);

    let (kind, key, label) = match feature {
        Some(name) => (
            UsageWindowKind::Opaque,
            format!("codex_{name}_{seconds}s"),
            humanize_key(name),
        ),
        None => (
            codex_window_kind(seconds),
            format!("codex_{seconds}s"),
            humanize_window_seconds(seconds),
        ),
    };
    Some(UsageWindow {
        key,
        kind,
        label,
        utilization,
        resets_at,
    })
}

/// Classify an account-wide Codex window by its duration.
fn codex_window_kind(seconds: i64) -> UsageWindowKind {
    match seconds {
        CODEX_SESSION_WINDOW_SECONDS => UsageWindowKind::Session,
        CODEX_WEEKLY_WINDOW_SECONDS => UsageWindowKind::Weekly,
        _ => UsageWindowKind::Opaque,
    }
}

/// A human label for a window duration in seconds (`18000` → "5-hour", `604800`
/// → "Weekly", `86400` → "1-day"). Session/weekly windows get product names from
/// the frontend; this is what an opaque window shows.
fn humanize_window_seconds(seconds: i64) -> String {
    let is_whole = |unit_seconds: i64| seconds > 0 && seconds % unit_seconds == 0;
    if is_whole(SECONDS_PER_DAY) {
        let days = seconds / SECONDS_PER_DAY;
        if days == DAYS_PER_WEEK {
            return "Weekly".to_string();
        }
        return format!("{days}-day");
    }
    if is_whole(SECONDS_PER_HOUR) {
        return format!("{}-hour", seconds / SECONDS_PER_HOUR);
    }
    if is_whole(SECONDS_PER_MINUTE) {
        return format!("{}-minute", seconds / SECONDS_PER_MINUTE);
    }
    format!("{seconds}-second")
}

/// A Unix timestamp (seconds since the epoch) as an ISO-8601 UTC string
/// (`YYYY-MM-DDTHH:MM:SSZ`). Codex reports window resets as epoch seconds; the
/// frontend countdown parses ISO strings (like the Claude windows), so we
/// normalize here. Pure `std` arithmetic - Howard Hinnant's `civil_from_days`,
/// no date crate.
fn unix_seconds_to_iso8601(seconds: i64) -> String {
    // The algorithm counts from 0000-03-01, so leap days fall at a year's end.
    const DAYS_FROM_MARCH_ZERO_TO_UNIX_EPOCH: i64 = 719_468;
    const DAYS_PER_ERA: i64 = 146_097;

    let days = seconds.div_euclid(SECONDS_PER_DAY);
    let seconds_of_day = seconds.rem_euclid(SECONDS_PER_DAY);
    let hour = seconds_of_day / SECONDS_PER_HOUR;
    let minute = (seconds_of_day % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    let second = seconds_of_day % SECONDS_PER_MINUTE;

    let shifted = days + DAYS_FROM_MARCH_ZERO_TO_UNIX_EPOCH;
    let era = shifted.div_euclid(DAYS_PER_ERA);
    let day_of_era = shifted - era * DAYS_PER_ERA;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_pointer = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_pointer + 2) / 5 + 1;
    let month = if month_pointer < 10 {
        month_pointer + 3
    } else {
        month_pointer - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// GET the Codex usage endpoint. The token and account id are interpolated into
/// quoted header directives, so anything outside the token character set is
/// refused - see [`is_safe_header_token`].
fn read_codex_usage_endpoint(login: &CodexLogin) -> Result<String, RequestFailure> {
    let CodexLogin { token, account_id } = *login;
    if !is_safe_header_token(token) || !is_safe_header_token(account_id) {
        return Err(RequestFailure::Rejected);
    }

    let authorization = format!("Bearer {token}");
    curl_json(&CurlRequest {
        url: CODEX_USAGE_URL,
        timeout_seconds: USAGE_REQUEST_TIMEOUT_SECONDS,
        headers: &[
            (AUTHORIZATION_HEADER, &authorization),
            ("ChatGPT-Account-Id", account_id),
            (USER_AGENT_HEADER, CODEX_USAGE_USER_AGENT),
        ],
        json_body: None,
    })
}

// ---------------------------------------------------------------------------
// Copilot usage. GitHub's Copilot CLI signs in with a GitHub OAuth token and
// meters "premium requests" against a monthly entitlement. The editor Copilot
// clients read that entitlement from the internal `copilot_internal/user`
// endpoint (the same one VS Code polls to render its usage meter) - an
// entitlement/status read that spends no premium request, so it's safe at the
// same ~180s cadence as the Claude/Codex endpoints. We reach it with the token
// the CLI's precedence order exposes: a `GITHUB_TOKEN`/`GH_TOKEN` env override,
// then the editor OAuth store (`apps.json`/`hosts.json`). The CLI's own default
// store is the OS keychain, which no std-only read can open - so when only the
// keychain holds the token we fall back to a bare "Copilot" plan label.
// ---------------------------------------------------------------------------

/// The internal entitlement endpoint the editor Copilot clients read the user's
/// plan and premium-request quota from. Undocumented but stable, and a status
/// read (not a chat/completion), so polling it costs no premium request.
const COPILOT_USAGE_URL: &str = "https://api.github.com/copilot_internal/user";
/// A Copilot-Chat-shaped `User-Agent`; the endpoint gates on an editor-flavored
/// client identifying itself, like the Claude/Codex endpoints do.
const COPILOT_USAGE_USER_AGENT: &str = "GitHubCopilotChat/0.26.7";
/// The `Editor-Version` the Copilot endpoints expect from an editor client.
const COPILOT_EDITOR_VERSION: &str = "vscode/1.99.0";
/// Where the fallback plan label comes from when only a local token (no live
/// numbers) is available.
const COPILOT_LOCAL_SOURCE: &str = "local:github-copilot";

/// The Copilot account: live entitlement windows when the endpoint answers,
/// otherwise a bare plan label confirming a local token exists. With no token
/// reachable at all (a keychain-only login, no env override, no editor OAuth
/// store) PADE has no source it can read - not "signed out", since the CLI may
/// well be signed in through the keychain. A token GitHub rejects is an expired
/// sign-in.
fn fetch_copilot_account_usage() -> Result<AccountUsage, NoReadingReason> {
    let token = copilot_token().ok_or(NoReadingReason::NoSource)?;
    match copilot_live_usage(&token) {
        Ok(account) => Ok(account),
        Err(NoReadingReason::SignInExpired) => Err(NoReadingReason::SignInExpired),
        Err(_) => Ok(AccountUsage {
            plan: COPILOT_PLAN.to_string(),
            source: COPILOT_LOCAL_SOURCE.to_string(),
            account: Some(BillingAccount::Github),
            ..AccountUsage::default()
        }),
    }
}

/// Live Copilot entitlement, or why there is none: the endpoint refused the
/// token, could not be reached, or answered a body that isn't a usage response
/// (neither `copilot_plan` nor any quota snapshot).
fn copilot_live_usage(token: &str) -> Result<AccountUsage, NoReadingReason> {
    let body = read_copilot_usage_endpoint(token)?;
    let response = parse_usage_body(&body)?;

    let has_plan = response.get("copilot_plan").is_some();
    let windows = collect_copilot_windows(&response);
    if windows.is_empty() && !has_plan {
        return Err(NoReadingReason::Unreachable);
    }

    let tier = response
        .get("copilot_plan")
        .and_then(serde_json::Value::as_str)
        .map(|tier| tier.replace('_', " "));
    Ok(AccountUsage {
        windows,
        plan: plan_label(COPILOT_PLAN, tier.as_deref()),
        source: COPILOT_OAUTH_SOURCE.to_string(),
        account: Some(BillingAccount::Github),
        ..AccountUsage::default()
    })
}

/// The premium-request / chat / completions quota snapshots the Copilot
/// entitlement response carries under `quota_snapshots`, one window each. A
/// snapshot flagged `unlimited` carries no meaningful percentage, so it's
/// skipped; every metered snapshot becomes an opaque window (a monthly
/// entitlement, not one of the session/weekly time windows), sharing the
/// account-wide `quota_reset_date`. Nothing is invented - a snapshot with no
/// derivable percentage falls away.
fn collect_copilot_windows(response: &serde_json::Value) -> Vec<UsageWindow> {
    let resets_at = string_field(response, "quota_reset_date");
    response
        .get("quota_snapshots")
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(name, snapshot)| copilot_window(name, snapshot, resets_at.clone()))
        .collect()
}

/// One Copilot quota snapshot as a [`UsageWindow`], or `None` when it's an
/// unlimited bucket or carries no derivable percent-used. Prefers the endpoint's
/// own `percent_remaining`; otherwise derives it from `remaining`/`entitlement`.
fn copilot_window(
    name: &str,
    snapshot: &serde_json::Value,
    resets_at: Option<String>,
) -> Option<UsageWindow> {
    let is_unlimited = snapshot
        .get("unlimited")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    if is_unlimited {
        return None;
    }
    let utilization = copilot_percent_used(snapshot)?;
    Some(UsageWindow {
        key: format!("copilot_{name}"),
        kind: UsageWindowKind::Opaque,
        label: humanize_key(name),
        utilization,
        resets_at,
    })
}

/// The percent *consumed* (0..100) of a Copilot quota snapshot: `100 -
/// percent_remaining` when the endpoint reports it, else derived from the
/// `remaining`/`entitlement` counts, else `None`.
fn copilot_percent_used(snapshot: &serde_json::Value) -> Option<f64> {
    if let Some(remaining) = snapshot
        .get("percent_remaining")
        .and_then(serde_json::Value::as_f64)
    {
        return Some(FULL_PERCENTAGE - remaining);
    }
    let remaining = snapshot
        .get("remaining")
        .and_then(serde_json::Value::as_f64)?;
    let entitlement = snapshot
        .get("entitlement")
        .and_then(serde_json::Value::as_f64)?;
    if entitlement <= 0.0 {
        return None;
    }
    Some((1.0 - remaining / entitlement) * FULL_PERCENTAGE)
}

/// The env overrides the Copilot CLI reads its GitHub token from, in its own
/// precedence order.
const COPILOT_TOKEN_VARIABLES: [&str; 3] = ["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"];
/// The editor OAuth store files, oldest layout first.
const COPILOT_STORE_FILES: [&str; 2] = ["apps.json", "hosts.json"];

/// The GitHub token to reach the Copilot entitlement endpoint with, in the
/// precedence the CLI itself uses: the [`COPILOT_TOKEN_VARIABLES`] env
/// overrides, then the editor OAuth store on disk. The CLI's own default store
/// is the OS keychain, which no dependency-free read can open - so a
/// keychain-only login yields `None` here (and the plan-label fallback).
fn copilot_token() -> Option<String> {
    COPILOT_TOKEN_VARIABLES
        .into_iter()
        .find_map(|variable| {
            std::env::var(variable)
                .ok()
                .filter(|token| !token.is_empty())
        })
        .or_else(copilot_token_from_store)
}

/// The Copilot OAuth token from the editor store - `apps.json` (and the newer
/// `hosts.json`), each an object keyed by host whose values carry an
/// `oauth_token`. The first token found wins. On Windows the store lives under
/// `%USERPROFILE%\AppData\Local\github-copilot`, elsewhere `~/.config/github-copilot`.
fn copilot_token_from_store() -> Option<String> {
    let home = home_directory()?;
    let directory = if cfg!(windows) {
        home.join("AppData/Local/github-copilot")
    } else {
        home.join(".config/github-copilot")
    };
    COPILOT_STORE_FILES.into_iter().find_map(|file| {
        let json = read_json(&directory.join(file))?;
        json.as_object()?
            .values()
            .filter_map(|host| host.get("oauth_token").and_then(serde_json::Value::as_str))
            .find(|token| !token.is_empty())
            .map(str::to_string)
    })
}

/// GET the Copilot entitlement endpoint. It uses the `token` (not `Bearer`)
/// authorization scheme and expects an editor `User-Agent` + `Editor-Version`.
/// Rejects anything outside the token character set - see
/// [`is_safe_header_token`].
fn read_copilot_usage_endpoint(token: &str) -> Result<String, RequestFailure> {
    if !is_safe_header_token(token) {
        return Err(RequestFailure::Rejected);
    }

    let authorization = format!("token {token}");
    curl_json(&CurlRequest {
        url: COPILOT_USAGE_URL,
        timeout_seconds: USAGE_REQUEST_TIMEOUT_SECONDS,
        headers: &[
            (AUTHORIZATION_HEADER, &authorization),
            (USER_AGENT_HEADER, COPILOT_USAGE_USER_AGENT),
            ("Editor-Version", COPILOT_EDITOR_VERSION),
            ("Accept", "application/json"),
        ],
        json_body: None,
    })
}

// ---------------------------------------------------------------------------
// Antigravity usage (tier-label only). Google's Antigravity CLI stores a Google
// OAuth token under `~/.gemini/antigravity-cli/antigravity-oauth-token`. Its
// remaining-quota does surface from a private `cloudcode-pa.googleapis.com`
// `v1internal:fetchAvailableModels` call, but that endpoint is undocumented,
// reverse-engineered, POST-only, and known to report headroom while generation
// is already rate-limited (a number that would mislead the meter) - so we do NOT
// poll it. We confirm the local login exists and surface just the plan label.
// ---------------------------------------------------------------------------

/// The Antigravity CLI's stored Google login, under the home directory.
const ANTIGRAVITY_TOKEN_RELATIVE_PATH: &str = ".gemini/antigravity-cli/antigravity-oauth-token";

/// Confirm the Antigravity OAuth token exists on disk and surface a bare plan
/// label with no numbers (no quota-free usage endpoint we trust). Signed out
/// when no login is present.
fn fetch_antigravity_account_usage() -> Result<AccountUsage, NoReadingReason> {
    let json =
        read_sign_in(home_directory().map(|home| home.join(ANTIGRAVITY_TOKEN_RELATIVE_PATH)))?;
    sign_in_field(&json, "/token/access_token")?;

    Ok(AccountUsage {
        plan: ANTIGRAVITY_PLAN.to_string(),
        source: local_source(ANTIGRAVITY_TOKEN_RELATIVE_PATH),
        account: Some(BillingAccount::Google),
        ..AccountUsage::default()
    })
}

// ---------------------------------------------------------------------------
// Cursor usage (tier-label only). The Cursor CLI (`cursor-agent`) stores a JWT
// under a version-dependent path (`~/.config/cursor/auth.json`,
// `~/.cursor/auth.json`, …). Cursor's usage does surface from `cursor.com/api/
// usage`, but only via a reverse-engineered `WorkosCursorSessionToken` *cookie*
// (userId::JWT) that a `CURSOR_API_KEY` can't even build, on an undocumented,
// version-fragile endpoint - too unreliable to poll. We confirm the local login
// exists and surface just the plan label.
// ---------------------------------------------------------------------------

/// Where the Cursor plan label comes from: a local login, wherever it was found.
const CURSOR_LOCAL_SOURCE: &str = "local:cursor";

/// The auth files `cursor-agent` has kept its login in, by version, under the
/// home directory. The layout drifts between versions, so every one is probed.
const CURSOR_AUTH_RELATIVE_PATHS: [&str; 5] = [
    ".config/cursor/auth.json",
    ".cursor/auth.json",
    ".config/cursor-agent/auth.json",
    "AppData/Roaming/cursor-agent/auth.json",
    ".cursor/cli-config.json",
];

/// Confirm a Cursor login exists on disk (or a `CURSOR_API_KEY` override) and
/// surface a bare plan label with no numbers. Signed out when no login is
/// present.
fn fetch_cursor_account_usage() -> Result<AccountUsage, NoReadingReason> {
    if !is_cursor_login_present() {
        return Err(NoReadingReason::SignedOut);
    }
    Ok(AccountUsage {
        plan: CURSOR_PLAN.to_string(),
        source: CURSOR_LOCAL_SOURCE.to_string(),
        account: Some(BillingAccount::Cursor),
        ..AccountUsage::default()
    })
}

/// Whether a Cursor login is present: the documented `CURSOR_API_KEY` override,
/// or a stored session in one of the [`CURSOR_AUTH_RELATIVE_PATHS`] (a JWT under
/// `accessToken`/`access_token`, or the `authInfo` marker the CLI writes to
/// `cli-config.json`). A wrong guess simply yields "not present".
fn is_cursor_login_present() -> bool {
    if std::env::var("CURSOR_API_KEY").is_ok_and(|key| !key.is_empty()) {
        return true;
    }
    let Some(home) = home_directory() else {
        return false;
    };
    CURSOR_AUTH_RELATIVE_PATHS
        .iter()
        .any(|relative_path| auth_file_has_login(&home.join(relative_path)))
}

/// Whether an auth file names a stored login - a non-empty `accessToken`/
/// `access_token` JWT, or the `authInfo` block the CLI writes once signed in.
fn auth_file_has_login(path: &std::path::Path) -> bool {
    let Some(json) = read_json(path) else {
        return false;
    };
    let has_named_token = ["accessToken", "access_token"].iter().any(|key| {
        json.get(key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|token| !token.is_empty())
    });
    has_named_token || json.get("authInfo").is_some()
}

#[cfg(test)]
mod tests {
    use super::{
        collect_codex_windows, collect_copilot_windows, collect_windows, credit_balance,
        credits_state, curl_config, http_answer, humanize_key, humanize_window_seconds,
        is_safe_header_token, uncapped_credits_spend, unix_seconds_to_iso8601, usage_from_account,
        AccountReading, AccountUsage, BillingAccount, ClaudeProfile, CreditsState, CurlRequest,
        HeadlineWindow, NoReadingReason, RequestFailure, Usage, UsageAgent, UsageWindow,
        UsageWindowKind,
    };
    use super::{AccountReadings, AccountUsageRequest, LAST_READING_LIMIT, USAGE_CACHE_TTL};
    use crate::agents::{ID_CODEX, ID_OPENCODE};
    use std::time::{Duration, Instant};

    const ONE_SECOND: Duration = Duration::from_secs(1);

    fn reading(plan: &str) -> AccountUsage {
        AccountUsage {
            plan: plan.to_string(),
            ..AccountUsage::default()
        }
    }

    fn readings_answered_at(asked_at: Instant) -> AccountReadings {
        let mut readings = AccountReadings::default();
        readings.record(Ok(reading("Max")), asked_at);
        readings
    }

    #[test]
    fn a_fresh_reading_says_nothing_is_missing() {
        let asked_at = Instant::now();
        let reading = readings_answered_at(asked_at).reading(UsageAgent::Codex, asked_at);

        assert_eq!(
            reading.account.map(|usage| usage.plan),
            Some("Max".to_string())
        );
        assert_eq!(reading.no_reading, None);
    }

    #[test]
    fn an_expired_sign_in_names_the_cli_that_refreshes_it() {
        let asked_at = Instant::now();
        let mut readings = AccountReadings::default();
        readings.record(Err(NoReadingReason::SignInExpired), asked_at);

        let reading = readings.reading(UsageAgent::Codex, asked_at);

        assert!(reading.account.is_none());
        let no_reading = reading.no_reading.expect("a reason");
        assert_eq!(no_reading.reason, NoReadingReason::SignInExpired);
        assert_eq!(no_reading.sign_in_command, Some("codex"));
    }

    #[test]
    fn a_failure_after_a_reading_holds_it_over_and_says_why() {
        let first_ask = Instant::now();
        let mut readings = readings_answered_at(first_ask);
        readings.record(
            Err(NoReadingReason::Unreachable),
            first_ask + USAGE_CACHE_TTL,
        );

        let reading = readings.reading(UsageAgent::Claude, first_ask + USAGE_CACHE_TTL);

        assert!(reading.account.is_some());
        assert_eq!(
            reading.no_reading.map(|no_reading| no_reading.reason),
            Some(NoReadingReason::Unreachable)
        );
    }

    #[test]
    fn a_reading_after_a_failure_clears_the_reason() {
        let asked_at = Instant::now();
        let mut readings = AccountReadings::default();
        readings.record(Err(NoReadingReason::SignedOut), asked_at);
        readings.record(Ok(reading("Plus")), asked_at + USAGE_CACHE_TTL);

        assert_eq!(
            readings
                .reading(UsageAgent::Codex, asked_at + USAGE_CACHE_TTL)
                .no_reading,
            None
        );
    }

    #[test]
    fn opencode_is_told_to_refresh_codex_whose_sign_in_it_reads() {
        let agent = UsageAgent::from_id(ID_OPENCODE).expect("an adapter");
        assert_eq!(
            crate::agents::command_for_id(agent.sign_in_agent()),
            Some("codex")
        );
    }

    #[test]
    fn the_reading_wire_shape_is_what_the_frontend_parses() {
        let wire = serde_json::to_value(AccountReading::no_source()).expect("serializes");
        assert_eq!(
            wire,
            serde_json::json!({
                "account": null,
                "noReading": { "reason": "no_source", "signInCommand": null }
            })
        );

        let reasons = [
            (NoReadingReason::SignedOut, "signed_out"),
            (NoReadingReason::SignInExpired, "sign_in_expired"),
            (NoReadingReason::Unreachable, "unreachable"),
        ];
        for (reason, wire) in reasons {
            assert_eq!(serde_json::to_value(reason).expect("serializes"), wire);
        }
    }

    #[test]
    fn a_success_status_answers_the_body() {
        assert_eq!(
            http_answer("{\"plan_type\":\"plus\"}\n200"),
            Ok("{\"plan_type\":\"plus\"}".to_string())
        );
    }

    #[test]
    fn an_unauthorized_or_forbidden_status_is_a_rejected_sign_in() {
        // The real Codex answer to an expired token, trimmed.
        let expired = "{\"error\":{\"code\":\"token_expired\"}}\n401";
        assert_eq!(http_answer(expired), Err(RequestFailure::Rejected));
        assert_eq!(http_answer("{}\n403"), Err(RequestFailure::Rejected));
    }

    #[test]
    fn a_refusal_or_no_status_leaves_the_vendor_unreachable() {
        assert_eq!(http_answer("{}\n429"), Err(RequestFailure::Unreachable));
        assert_eq!(http_answer("\n503"), Err(RequestFailure::Unreachable));
        assert_eq!(http_answer("\n000"), Err(RequestFailure::Unreachable));
        assert_eq!(http_answer(""), Err(RequestFailure::Unreachable));
    }

    #[test]
    fn a_rejected_request_is_an_expired_sign_in() {
        assert_eq!(
            NoReadingReason::from(RequestFailure::Rejected),
            NoReadingReason::SignInExpired
        );
        assert_eq!(
            NoReadingReason::from(RequestFailure::Unreachable),
            NoReadingReason::Unreachable
        );
    }

    #[test]
    fn an_agent_never_asked_must_be_asked() {
        let now = Instant::now();
        let request = AccountUsageRequest::cached(UsageAgent::Claude);
        assert!(AccountReadings::default().needs_asking(request, now, now));
    }

    #[test]
    fn a_cached_request_reuses_a_fresh_ask() {
        let asked_at = Instant::now();
        let requested_at = asked_at + USAGE_CACHE_TTL.saturating_sub(ONE_SECOND);
        let request = AccountUsageRequest::cached(UsageAgent::Claude);
        let readings = readings_answered_at(asked_at);
        assert!(!readings.needs_asking(request, requested_at, requested_at));

        let expired_at = asked_at + USAGE_CACHE_TTL;
        assert!(readings.needs_asking(request, expired_at, expired_at));
    }

    #[test]
    fn a_forced_request_asks_unless_someone_asked_after_it_was_made() {
        let asked_at = Instant::now();
        let request = AccountUsageRequest::refresh(UsageAgent::Claude);
        let readings = readings_answered_at(asked_at);
        let made_later = asked_at + ONE_SECOND;
        assert!(readings.needs_asking(request, made_later, made_later));

        // A burst of forced refreshes waits on the one ask in flight, then reuses it.
        let made_earlier = asked_at;
        assert!(!readings.needs_asking(request, made_earlier, asked_at + ONE_SECOND));
    }

    #[test]
    fn a_failed_ask_keeps_the_last_reading_and_holds_off_a_retry() {
        let first_ask = Instant::now();
        let mut readings = readings_answered_at(first_ask);
        let failed_ask = first_ask + USAGE_CACHE_TTL;
        readings.record(Err(NoReadingReason::Unreachable), failed_ask);

        let shortly_after = failed_ask + ONE_SECOND;
        let request = AccountUsageRequest::cached(UsageAgent::Claude);
        assert!(!readings.needs_asking(request, shortly_after, shortly_after));
        assert_eq!(
            readings.last_known(shortly_after).map(|usage| usage.plan),
            Some("Max".to_string())
        );
    }

    #[test]
    fn a_reading_past_its_limit_is_no_longer_shown() {
        let asked_at = Instant::now();
        let mut readings = readings_answered_at(asked_at);
        readings.record(
            Err(NoReadingReason::Unreachable),
            asked_at + LAST_READING_LIMIT,
        );
        assert!(readings.last_known(asked_at + LAST_READING_LIMIT).is_none());
    }

    #[test]
    fn a_failure_before_any_reading_answers_nothing() {
        let asked_at = Instant::now();
        let mut readings = AccountReadings::default();
        readings.record(Err(NoReadingReason::Unreachable), asked_at);
        assert!(readings.last_known(asked_at).is_none());
    }

    #[test]
    fn opencode_resolves_to_the_codex_adapter() {
        // One ChatGPT subscription, one source of truth: both agent ids read the
        // same adapter, so both surface the same account.
        assert!(matches!(
            UsageAgent::from_id(ID_OPENCODE),
            Some(UsageAgent::Codex)
        ));
        assert!(matches!(
            UsageAgent::from_id(ID_CODEX),
            Some(UsageAgent::Codex)
        ));
    }

    #[test]
    fn collect_windows_enumerates_named_then_model_windows() {
        let response = serde_json::json!({
            "five_hour": { "utilization": 40.0, "resets_at": "2026-01-01T00:00:00Z" },
            "seven_day": { "utilization": 80.0 },
            "limits": [
                { "percent": 12.0, "scope": { "model": { "display_name": "Claude Opus" } } },
                { "percent": 5.0, "scope": { "org": true } }
            ]
        });

        let windows = collect_windows(&response);

        // session, weekly, then the one named-model window; the scopeless limit
        // row and the non-window `limits` key are skipped.
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].kind, UsageWindowKind::Session);
        assert_eq!(windows[0].key, "five_hour");
        assert!((windows[0].utilization - 40.0).abs() < f64::EPSILON);
        assert_eq!(
            windows[0].resets_at.as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
        assert_eq!(windows[1].kind, UsageWindowKind::Weekly);
        assert_eq!(windows[1].resets_at, None);
        assert_eq!(windows[2].kind, UsageWindowKind::Model);
        assert_eq!(windows[2].label, "Claude Opus");
    }

    #[test]
    fn an_unrecognized_named_window_passes_through_as_opaque() {
        let response = serde_json::json!({ "thirty_day": { "utilization": 3.0 } });

        let windows = collect_windows(&response);

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].kind, UsageWindowKind::Opaque);
        assert_eq!(windows[0].key, "thirty_day");
        assert_eq!(windows[0].label, "Thirty day");
    }

    #[test]
    fn credits_turned_on_with_room_are_available() {
        // The real shape, probed live from a Max account with credits on.
        let response = serde_json::json!({
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": 20000,
                "used_credits": 11333,
                "utilization": 56.665,
                "currency": "USD",
                "decimal_places": 2,
                "disabled_reason": null,
                "user_disabled": false,
                "spend_limit_reached": false
            }
        });

        assert_eq!(credits_state(&response), Some(CreditsState::Available));
    }

    #[test]
    fn uncapped_credits_are_available() {
        let response = serde_json::json!({
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": null,
                "used_credits": null,
                "utilization": null
            }
        });

        assert_eq!(credits_state(&response), Some(CreditsState::Available));
    }

    #[test]
    fn spent_credits_are_unavailable() {
        let response = serde_json::json!({
            "extra_usage": { "is_enabled": true, "monthly_limit": 5000, "utilization": 100.0 }
        });

        assert_eq!(credits_state(&response), Some(CreditsState::Unavailable));
    }

    #[test]
    fn credits_held_back_by_the_account_are_unavailable() {
        let reached = serde_json::json!({
            "extra_usage": { "is_enabled": true, "utilization": 40.0, "spend_limit_reached": true }
        });
        let disabled = serde_json::json!({
            "extra_usage": {
                "is_enabled": true,
                "utilization": 40.0,
                "disabled_reason": "org_level_disabled_until"
            }
        });

        assert_eq!(credits_state(&reached), Some(CreditsState::Unavailable));
        assert_eq!(credits_state(&disabled), Some(CreditsState::Unavailable));
    }

    #[test]
    fn credits_turned_off_are_off_even_with_a_stale_cap_reading() {
        let response = serde_json::json!({
            "extra_usage": { "is_enabled": false, "monthly_limit": 5000, "utilization": 12.0 }
        });

        assert_eq!(credits_state(&response), Some(CreditsState::Off));
    }

    #[test]
    fn a_prepaid_balance_reads_in_major_units() {
        // The real `prepaid/credits` shape, probed live (trimmed of its tranches).
        let response = serde_json::json!({
            "amount": 4689,
            "currency": "USD",
            "balance": { "money": null, "credits": { "amount_minor": 4689, "exponent": 2 } },
            "balance_credits": 46,
            "auto_reload_settings": { "enabled": true }
        });

        let balance = credit_balance(&response).expect("a balance");

        assert!((balance.amount - 46.89).abs() < 1e-9);
        assert_eq!(balance.currency, "USD");
    }

    #[test]
    fn a_money_balance_and_its_own_exponent_are_honored() {
        let response = serde_json::json!({
            "currency": "JPY",
            "balance": { "money": { "amount_minor": 1200, "exponent": 0 }, "credits": null }
        });

        let balance = credit_balance(&response).expect("a balance");

        assert!((balance.amount - 1200.0).abs() < 1e-9);
    }

    #[test]
    fn a_body_without_a_balance_has_none() {
        let error_body = serde_json::json!({ "error": { "message": "not found" } });
        let no_amount =
            serde_json::json!({ "currency": "USD", "balance": { "money": null, "credits": null } });

        assert_eq!(credit_balance(&error_body), None);
        assert_eq!(credit_balance(&no_amount), None);
    }

    #[test]
    fn uncapped_credits_report_their_spend() {
        // The real shape, probed live from a Max account with no monthly cap.
        let response = serde_json::json!({
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": null,
                "used_credits": 25918.0,
                "utilization": null,
                "currency": "USD",
                "decimal_places": 2,
                "disabled_reason": null,
                "spend_limit_reached": false
            }
        });

        let spend = uncapped_credits_spend(&response).expect("a spend");
        assert!((spend.amount - 259.18).abs() < 1e-9);
        assert_eq!(spend.currency, "USD");
    }

    #[test]
    fn capped_or_disabled_credits_report_no_spend() {
        let capped = serde_json::json!({
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": 20000,
                "used_credits": 11333,
                "currency": "USD",
                "decimal_places": 2
            }
        });
        let disabled = serde_json::json!({
            "extra_usage": {
                "is_enabled": false,
                "monthly_limit": null,
                "used_credits": 0,
                "currency": "USD",
                "decimal_places": 2
            }
        });

        assert_eq!(uncapped_credits_spend(&capped), None);
        assert_eq!(uncapped_credits_spend(&disabled), None);
    }

    #[test]
    fn the_extra_usage_cap_is_the_credits_window() {
        let response = serde_json::json!({
            "extra_usage": { "is_enabled": true, "utilization": 56.665 },
            "five_hour": { "utilization": 100.0 }
        });

        let windows = collect_windows(&response);

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].kind, UsageWindowKind::Session);
        assert_eq!(windows[1].kind, UsageWindowKind::Credits);
    }

    #[test]
    fn a_response_without_credits_has_no_credits_state() {
        let without_key = serde_json::json!({ "five_hour": { "utilization": 40.0 } });
        let null_credits = serde_json::json!({ "extra_usage": null });

        assert_eq!(credits_state(&without_key), None);
        assert_eq!(credits_state(&null_credits), None);
    }

    #[test]
    fn a_non_usage_body_yields_no_windows() {
        let error_body = serde_json::json!({ "error": { "message": "unauthorized" } });

        assert!(collect_windows(&error_body).is_empty());
    }

    #[test]
    fn collect_codex_windows_maps_a_weekly_primary_window() {
        // The real `/backend-api/wham/usage` shape captured from a ChatGPT account:
        // a single 7-day primary window, no secondary, no additional limits.
        let response = serde_json::json!({
            "plan_type": "plus",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 5,
                    "limit_window_seconds": 604_800,
                    "reset_after_seconds": 577_913,
                    "reset_at": 1_785_074_467_i64
                },
                "secondary_window": serde_json::Value::Null
            },
            "additional_rate_limits": serde_json::Value::Null
        });

        let windows = collect_codex_windows(&response);

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].kind, UsageWindowKind::Weekly);
        assert!((windows[0].utilization - 5.0).abs() < f64::EPSILON);
        assert_eq!(windows[0].label, "Weekly");
        // Epoch seconds normalized to the ISO string the frontend countdown parses.
        assert_eq!(
            windows[0].resets_at.as_deref(),
            Some("2026-07-26T14:01:07Z")
        );
    }

    #[test]
    fn collect_codex_windows_classifies_session_before_weekly() {
        let response = serde_json::json!({
            "rate_limit": {
                "primary_window": { "used_percent": 20, "limit_window_seconds": 18_000 },
                "secondary_window": { "used_percent": 60, "limit_window_seconds": 604_800 }
            }
        });

        let windows = collect_codex_windows(&response);

        assert_eq!(windows.len(), 2);
        // Session (5-hour) sorts ahead of the weekly cap, matching the Claude order.
        assert_eq!(windows[0].kind, UsageWindowKind::Session);
        assert!((windows[0].utilization - 20.0).abs() < f64::EPSILON);
        assert_eq!(windows[1].kind, UsageWindowKind::Weekly);
        assert!((windows[1].utilization - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn collect_codex_windows_surfaces_additional_limits_opaquely() {
        let response = serde_json::json!({
            "rate_limit": {
                "primary_window": { "used_percent": 10, "limit_window_seconds": 604_800 }
            },
            "additional_rate_limits": [{
                "limit_name": "code_review",
                "rate_limit": {
                    "primary_window": { "used_percent": 33, "limit_window_seconds": 18_000 }
                }
            }]
        });

        let windows = collect_codex_windows(&response);

        assert_eq!(windows.len(), 2);
        // The account weekly cap, then the named feature surfaced opaquely under
        // its own name - never mislabeled the account "Session".
        assert_eq!(windows[0].kind, UsageWindowKind::Weekly);
        assert_eq!(windows[1].kind, UsageWindowKind::Opaque);
        assert_eq!(windows[1].label, "Code review");
    }

    #[test]
    fn a_codex_body_without_rate_limits_yields_no_windows() {
        let empty =
            serde_json::json!({ "plan_type": "plus", "rate_limit": serde_json::Value::Null });

        assert!(collect_codex_windows(&empty).is_empty());
    }

    #[test]
    fn unix_seconds_to_iso8601_matches_known_epochs() {
        assert_eq!(unix_seconds_to_iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(
            unix_seconds_to_iso8601(1_704_067_200),
            "2024-01-01T00:00:00Z"
        );
        assert_eq!(
            unix_seconds_to_iso8601(1_785_074_467),
            "2026-07-26T14:01:07Z"
        );
    }

    #[test]
    fn humanize_window_seconds_labels_common_durations() {
        assert_eq!(humanize_window_seconds(18_000), "5-hour");
        assert_eq!(humanize_window_seconds(604_800), "Weekly");
        assert_eq!(humanize_window_seconds(86_400), "1-day");
        assert_eq!(humanize_window_seconds(3_600), "1-hour");
    }

    #[test]
    fn one_profile_read_answers_both_the_organization_and_the_version() {
        let profile = ClaudeProfile(serde_json::json!({
            "oauthAccount": { "organizationUuid": "ef60a6fb-ea18-46d3-a019-46012137d46e" },
            "lastReleaseNotesSeen": "2.1.227",
            "projects": {}
        }));

        assert_eq!(
            profile.organization(),
            Some("ef60a6fb-ea18-46d3-a019-46012137d46e")
        );
        assert_eq!(profile.cli_version(), Some("2.1.227"));
        assert_eq!(ClaudeProfile(serde_json::json!({})).organization(), None);
    }

    #[test]
    fn the_wire_names_stay_what_the_frontend_reads() {
        let usage = serde_json::to_value(Usage {
            used_percentage: Some(12.5),
            label: "Codex".to_string(),
            resets_at: None,
            source: "local:.codex/auth.json".to_string(),
        })
        .expect("usage serializes");
        assert_eq!(usage["usedPct"], serde_json::json!(12.5));

        let accounts = [
            (BillingAccount::Anthropic, "anthropic"),
            (BillingAccount::Chatgpt, "chatgpt"),
            (BillingAccount::Github, "github"),
            (BillingAccount::Google, "google"),
            (BillingAccount::Cursor, "cursor"),
        ];
        for (account, wire) in accounts {
            assert_eq!(serde_json::to_value(account).expect("serializes"), wire);
        }
    }

    #[test]
    fn a_curl_config_carries_every_header_and_turns_a_body_into_a_quoted_post() {
        let config = curl_config(&CurlRequest {
            url: "https://example.com/reset",
            timeout_seconds: 20,
            headers: &[("Authorization", "Bearer abc"), ("User-Agent", "pade")],
            json_body: Some(r#"{"id":"a\b"}"#),
        });

        assert_eq!(
            config,
            "silent\nshow-error\nwrite-out = \"\\n%{http_code}\"\nmax-time = 20\n\
             url = \"https://example.com/reset\"\n\
             header = \"Authorization: Bearer abc\"\n\
             header = \"User-Agent: pade\"\n\
             request = \"POST\"\ndata = \"{\\\"id\\\":\\\"a\\\\b\\\"}\"\n"
        );
    }

    #[test]
    fn a_curl_config_without_a_body_is_a_get() {
        let config = curl_config(&CurlRequest {
            url: "https://example.com/usage",
            timeout_seconds: 12,
            headers: &[],
            json_body: None,
        });

        assert!(!config.contains("request ="), "{config}");
        assert!(
            config.ends_with("url = \"https://example.com/usage\"\n"),
            "{config}"
        );
    }

    #[test]
    fn a_chatgpt_account_id_is_a_safe_header_token() {
        assert!(is_safe_header_token("ef60a6fb-ea18-46d3-a019-46012137d46e"));
    }

    #[test]
    fn humanize_key_sentence_cases_a_snake_case_key() {
        assert_eq!(humanize_key("seven_day"), "Seven day");
        assert_eq!(humanize_key("five_hour"), "Five hour");
        assert_eq!(humanize_key(""), "");
    }

    #[test]
    fn a_realistic_oauth_token_is_accepted() {
        assert!(is_safe_header_token(
            "sk-ant-oat01-AbCdEfGh_1234567890-IjKlMnOp.QrStUvWxYz"
        ));
    }

    #[test]
    fn an_embedded_double_quote_is_rejected() {
        assert!(!is_safe_header_token("abc\"\nurl = \"http://evil\""));
        assert!(!is_safe_header_token("\""));
    }

    #[test]
    fn an_embedded_newline_is_rejected() {
        assert!(!is_safe_header_token("abc\ndef"));
    }

    #[test]
    fn an_embedded_carriage_return_is_rejected() {
        assert!(!is_safe_header_token("abc\rdef"));
    }

    #[test]
    fn an_empty_token_is_rejected() {
        assert!(!is_safe_header_token(""));
    }

    #[test]
    fn whitespace_and_shell_ish_punctuation_are_rejected() {
        assert!(!is_safe_header_token("abc def"));
        assert!(!is_safe_header_token("abc\\def"));
        assert!(!is_safe_header_token("abc;def"));
        assert!(!is_safe_header_token("abc:def"));
        assert!(!is_safe_header_token("abc=def"));
    }

    #[test]
    fn base64_padding_and_separator_characters_are_rejected() {
        assert!(!is_safe_header_token("abc+def"));
        assert!(!is_safe_header_token("abc/def"));
    }

    #[test]
    fn non_ascii_characters_are_rejected() {
        assert!(!is_safe_header_token("abcé"));
        assert!(!is_safe_header_token("токен"));
    }

    #[test]
    fn collect_copilot_windows_maps_the_metered_premium_snapshot() {
        // The `copilot_internal/user` shape: a metered premium-request snapshot
        // plus unlimited chat/completions on a paid plan, and the shared reset.
        let response = serde_json::json!({
            "copilot_plan": "individual_pro",
            "quota_reset_date": "2026-08-01",
            "quota_snapshots": {
                "chat": { "unlimited": true, "percent_remaining": 100 },
                "completions": { "unlimited": true },
                "premium_interactions": {
                    "unlimited": false,
                    "entitlement": 300,
                    "remaining": 210,
                    "percent_remaining": 70
                }
            }
        });

        let windows = collect_copilot_windows(&response);

        // Only the metered snapshot surfaces - the unlimited buckets carry no
        // meaningful percentage and are skipped.
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].kind, UsageWindowKind::Opaque);
        assert_eq!(windows[0].key, "copilot_premium_interactions");
        assert_eq!(windows[0].label, "Premium interactions");
        // 70% remaining ⇒ 30% consumed.
        assert!((windows[0].utilization - 30.0).abs() < f64::EPSILON);
        assert_eq!(windows[0].resets_at.as_deref(), Some("2026-08-01"));
    }

    #[test]
    fn collect_copilot_windows_derives_percent_from_counts_when_absent() {
        let response = serde_json::json!({
            "copilot_plan": "business",
            "quota_snapshots": {
                "premium_interactions": { "unlimited": false, "entitlement": 400, "remaining": 100 }
            }
        });

        let windows = collect_copilot_windows(&response);

        assert_eq!(windows.len(), 1);
        // 100 of 400 remaining ⇒ 75% consumed.
        assert!((windows[0].utilization - 75.0).abs() < f64::EPSILON);
        // No account reset date ⇒ no reset surfaced, never invented.
        assert_eq!(windows[0].resets_at, None);
    }

    #[test]
    fn a_copilot_body_without_snapshots_yields_no_windows() {
        // A 401 error blob is not a usage response - no snapshots, no windows.
        let error_body = serde_json::json!({ "message": "Bad credentials" });

        assert!(collect_copilot_windows(&error_body).is_empty());
    }

    #[test]
    fn usage_from_account_prefers_the_weekly_window() {
        let account = AccountUsage {
            plan: "Copilot business".to_string(),
            source: "oauth:api.github.com".to_string(),
            account: None,
            credits: None,
            credit_balance: None,
            uncapped_spend: None,
            windows: vec![
                UsageWindow {
                    key: "a".to_string(),
                    kind: UsageWindowKind::Opaque,
                    label: "A".to_string(),
                    utilization: 90.0,
                    resets_at: None,
                },
                UsageWindow {
                    key: "b".to_string(),
                    kind: UsageWindowKind::Weekly,
                    label: "B".to_string(),
                    utilization: 40.0,
                    resets_at: Some("2026-08-01".to_string()),
                },
            ],
        };

        let usage = usage_from_account(account, HeadlineWindow::WeeklyOrMostConsumed);

        // The weekly cap is the headline even when another window is hotter.
        assert_eq!(usage.used_percentage, Some(40.0));
        assert_eq!(usage.resets_at.as_deref(), Some("2026-08-01"));
        assert_eq!(usage.label, "Copilot business");
    }

    #[test]
    fn usage_from_account_falls_back_to_the_most_consumed_window() {
        let account = AccountUsage {
            plan: "Copilot".to_string(),
            source: "oauth:api.github.com".to_string(),
            account: None,
            credits: None,
            credit_balance: None,
            uncapped_spend: None,
            windows: vec![
                UsageWindow {
                    key: "a".to_string(),
                    kind: UsageWindowKind::Opaque,
                    label: "A".to_string(),
                    utilization: 12.0,
                    resets_at: None,
                },
                UsageWindow {
                    key: "b".to_string(),
                    kind: UsageWindowKind::Opaque,
                    label: "B".to_string(),
                    utilization: 88.0,
                    resets_at: None,
                },
            ],
        };

        assert_eq!(
            usage_from_account(account, HeadlineWindow::WeeklyOrMostConsumed).used_percentage,
            Some(88.0)
        );
    }

    #[test]
    fn weekly_only_ignores_a_non_weekly_window() {
        // Claude's strategy: with no weekly cap present, the meter shows no number
        // rather than falling back to the most-consumed window.
        let account = AccountUsage {
            plan: "Claude Pro".to_string(),
            source: "oauth:api.anthropic.com".to_string(),
            account: None,
            credits: None,
            credit_balance: None,
            uncapped_spend: None,
            windows: vec![UsageWindow {
                key: "five_hour".to_string(),
                kind: UsageWindowKind::Session,
                label: "Session".to_string(),
                utilization: 55.0,
                resets_at: Some("2026-08-01".to_string()),
            }],
        };

        let usage = usage_from_account(account, HeadlineWindow::WeeklyOnly);

        assert_eq!(usage.used_percentage, None);
        assert_eq!(usage.resets_at, None);
        assert_eq!(usage.label, "Claude Pro");
    }

    #[test]
    fn usage_from_account_leaves_a_tier_label_account_without_numbers() {
        // The Antigravity / Cursor shape: a plan label, no windows - an honest
        // "we know your login, not your usage".
        let account = AccountUsage {
            plan: "Antigravity".to_string(),
            source: "local:.gemini/antigravity-cli/antigravity-oauth-token".to_string(),
            windows: Vec::new(),
            account: Some(BillingAccount::Google),
            credits: None,
            credit_balance: None,
            uncapped_spend: None,
        };

        let usage = usage_from_account(account, HeadlineWindow::WeeklyOrMostConsumed);

        assert_eq!(usage.used_percentage, None);
        assert_eq!(usage.resets_at, None);
        assert_eq!(usage.label, "Antigravity");
    }
}
