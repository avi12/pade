//! Agent usage / quota meter.
//!
//! Surfaces how much of the active agent's quota is left, sourced WITHOUT
//! spending any message quota — we never invoke the agent CLI. Two sources,
//! best-effort per agent:
//!  - The vendor's OAuth usage endpoint (the same numbers claude.ai shows),
//!    called with the locally-stored access token and cached for ~3 minutes —
//!    a real network request, but one that consumes no quota.
//!  - Data the agent already persisted locally (e.g. the subscription tier
//!    label) when the network / token isn't available, leaving
//!    `used_pct`/`resets_at` `None` if the precise numbers aren't on disk.
//!
//! When nothing reliable is found we return `None` and the UI shows an honest
//! "usage unavailable" state — we never fabricate numbers.

use std::collections::HashMap;
use std::io::Write;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::util::home_dir;

/// The agents we can source usage for, and the one authoritative mapping from
/// the registry id string the frontend sends (`session.agent.id`). An agent we
/// have no usage adapter for isn't in this set, so its usage stays an honest "—".
///
/// Two adapter shapes live here. A *live* adapter (Claude, Codex, Copilot) reads
/// real rate-limit windows from the vendor's own usage endpoint with the locally
/// stored token. A *tier-label* adapter (Antigravity, Cursor) has no quota-free
/// usage endpoint we can rely on, so it confirms the local login exists and
/// surfaces just the plan label with no numbers. Agents that authenticate with
/// the user's *own* pay-as-you-go provider API key rather than a metered
/// subscription — Grok (an xAI key) and aider (the user's OpenAI/Anthropic key) —
/// have no plan quota at all, so they are deliberately absent and stay "—".
/// An agent id and an adapter aren't 1:1: opencode signs into the same
/// `ChatGPT` subscription Codex does, so its id resolves to the Codex adapter.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum UsageAgent {
    Claude,
    Codex,
    Copilot,
    Antigravity,
    Cursor,
}

const AGENT_ID_CLAUDE: &str = "claude";
const AGENT_ID_CODEX: &str = "codex";
const AGENT_ID_COPILOT: &str = "copilot";
const AGENT_ID_ANTIGRAVITY: &str = "antigravity";
const AGENT_ID_CURSOR: &str = "cursor";
const AGENT_ID_OPENCODE: &str = "opencode";

impl UsageAgent {
    fn from_id(id: &str) -> Option<Self> {
        match id {
            AGENT_ID_CLAUDE => Some(Self::Claude),
            // opencode authenticates against the same ChatGPT subscription Codex
            // does, so it reads the Codex adapter — one account, one source of
            // truth.
            AGENT_ID_CODEX | AGENT_ID_OPENCODE => Some(Self::Codex),
            AGENT_ID_COPILOT => Some(Self::Copilot),
            AGENT_ID_ANTIGRAVITY => Some(Self::Antigravity),
            AGENT_ID_CURSOR => Some(Self::Cursor),
            _ => None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    /// Percent of quota consumed, 0..100, when a precise number is known.
    pub used_pct: Option<f64>,
    /// Short human label for the meter (e.g. a plan/tier name or a summary).
    pub label: String,
    /// When the quota window resets, as an ISO-8601 string, when known.
    pub resets_at: Option<String>,
    /// Where the figure came from — so the UI can be honest about it.
    pub source: String,
}

/// Remaining usage for `agent` — live account windows when the OAuth endpoint
/// is reachable, local tier label otherwise. `None` when we have no reliable
/// signal for that agent (the UI then shows "usage —").
#[tauri::command]
pub fn usage_get(agent: String) -> Option<Usage> {
    match UsageAgent::from_id(&agent)? {
        UsageAgent::Claude => claude_usage(),
        UsageAgent::Codex => codex_usage(),
        UsageAgent::Copilot => copilot_usage(),
        UsageAgent::Antigravity => antigravity_usage(),
        UsageAgent::Cursor => cursor_usage(),
    }
}

/// The headline figure for an account's meter, mapping whatever windows the
/// endpoint returned into the flat [`Usage`] the handoff logic reads: the weekly
/// all-models cap when present, else the most-consumed window, else no number at
/// all (a tier-label account with an empty window list). Shared by every live and
/// tier-label agent that reads through [`account_usage_for`] (DRY).
fn usage_from_account(account: AccountUsage) -> Usage {
    let headline = account
        .windows
        .iter()
        .find(|window| window.kind == UsageWindowKind::Weekly)
        .or_else(|| {
            account
                .windows
                .iter()
                .max_by(|first, second| first.utilization.total_cmp(&second.utilization))
        });
    let (used_pct, resets_at) = headline.map_or((None, None), |window| {
        (Some(window.utilization), window.resets_at.clone())
    });
    Usage {
        used_pct,
        label: account.plan,
        resets_at,
        source: account.source,
    }
}

/// Claude usage for the meter. Prefers the live account windows (the real weekly
/// %, mirroring claude.ai) and falls back to the honest subscription-tier label
/// when the network / token isn't available.
fn claude_usage() -> Option<Usage> {
    let Some(account) = account_usage_for(UsageAgent::Claude) else {
        return tier_label_usage();
    };

    let weekly = account
        .windows
        .iter()
        .find(|window| window.kind == UsageWindowKind::Weekly);
    let (used_pct, resets_at) = weekly.map_or((None, None), |window| {
        (Some(window.utilization), window.resets_at.clone())
    });
    Some(Usage {
        used_pct,
        label: account.plan,
        resets_at,
        source: account.source,
    })
}

/// Offline fallback: just the subscription tier from the local credentials, with
/// the precise numbers left `None`.
fn tier_label_usage() -> Option<Usage> {
    let creds = home_dir()?.join(".claude").join(".credentials.json");
    let raw = std::fs::read_to_string(&creds).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;

    let tier = json
        .get("claudeAiOauth")
        .and_then(|oauth| oauth.get("subscriptionType"))
        .and_then(serde_json::Value::as_str)?;

    Some(Usage {
        used_pct: None,
        label: format!("Claude {tier}"),
        resets_at: None,
        source: "local:.claude/.credentials.json".to_string(),
    })
}

// ---------------------------------------------------------------------------
// Codex usage. Codex signs in with a ChatGPT account (`codex login`), storing the
// OAuth tokens in `~/.codex/auth.json`. Its TUI reads live rate limits from the
// same ChatGPT backend endpoint (`/backend-api/wham/usage`) the Codex CLI polls;
// we call it the same honest way — a real request that spends no message quota —
// and fall back to the local plan label when the token/network isn't available.
// ---------------------------------------------------------------------------

/// Codex usage for the meter. Prefers the live weekly window (falling back to the
/// most-consumed window the endpoint returns), and the honest local plan label
/// when the network / token isn't available.
fn codex_usage() -> Option<Usage> {
    let Some(account) = account_usage_for(UsageAgent::Codex) else {
        return codex_tier_label_usage();
    };

    let headline = account
        .windows
        .iter()
        .find(|window| window.kind == UsageWindowKind::Weekly)
        .or_else(|| {
            account
                .windows
                .iter()
                .max_by(|first, second| first.utilization.total_cmp(&second.utilization))
        });
    let (used_pct, resets_at) = headline.map_or((None, None), |window| {
        (Some(window.utilization), window.resets_at.clone())
    });
    Some(Usage {
        used_pct,
        label: account.plan,
        resets_at,
        source: account.source,
    })
}

/// Offline fallback: confirm a local `ChatGPT` auth exists and surface a bare
/// "Codex" plan label with no numbers. We don't read the plan from the local
/// id-token — it can be stale (e.g. "free" after an upgrade to Plus) — so we
/// stay honest and leave the precise figures `None` until the endpoint answers.
fn codex_tier_label_usage() -> Option<Usage> {
    let auth = home_dir()?.join(".codex").join("auth.json");
    let raw = std::fs::read_to_string(&auth).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    json.get("tokens")
        .and_then(|tokens| tokens.get("access_token"))
        .and_then(serde_json::Value::as_str)?;

    Some(Usage {
        used_pct: None,
        label: "Codex".to_string(),
        resets_at: None,
        source: "local:.codex/auth.json".to_string(),
    })
}

// ---------------------------------------------------------------------------
// Live account usage — every rate-limit window claude.ai / `claude /usage` shows,
// read from the OAuth endpoint using the local Claude Code token. We enumerate
// whatever windows the response carries rather than hardcoding a fixed few.
// ---------------------------------------------------------------------------

/// The undocumented endpoint Claude Code's `/usage` reads. It requires the
/// `claude-code/<version>` User-Agent (any other UA lands in an aggressively
/// rate-limited bucket) and is safe at ~180s intervals — hence the cache below.
const OAUTH_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const USAGE_UA: &str = "claude-code/1.0.128";
const USAGE_CACHE_SECS: u64 = 170;

/// The two named top-level windows we give product names to; every other named
/// window the endpoint returns is surfaced generically as
/// [`UsageWindowKind::Opaque`].
const SESSION_WINDOW_KEY: &str = "five_hour";
const WEEKLY_WINDOW_KEY: &str = "seven_day";

/// The semantic kind of a rate-limit window. The endpoint returns a handful of
/// named windows (the 5-hour `five_hour` session, the 7-day `seven_day` weekly
/// all-models cap) plus per-model caps under `limits[]`; we classify each so the
/// meter can label it and the auto-resume scheduler can match the window a CLI
/// named. A named window we don't recognize passes through as `Opaque` — surfaced
/// honestly, never dropped.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageWindowKind {
    Session,
    Weekly,
    Model,
    Opaque,
}

impl UsageWindowKind {
    /// The one authoritative mapping from a top-level window key to its kind.
    fn from_window_key(key: &str) -> Self {
        match key {
            SESSION_WINDOW_KEY => Self::Session,
            WEEKLY_WINDOW_KEY => Self::Weekly,
            _ => Self::Opaque,
        }
    }

    /// Stable render + match order: session, then the weekly all-models cap, then
    /// per-model caps, then anything unrecognized.
    fn order(self) -> u8 {
        match self {
            Self::Session => 0,
            Self::Weekly => 1,
            Self::Model => 2,
            Self::Opaque => 3,
        }
    }
}

/// One rate-limit window surfaced from the account response: a stable `key`, its
/// semantic `kind`, a human `label`, the 0..100 `utilization` percent, and an
/// ISO-8601 reset time when known. The generic shape the meter renders and the
/// auto-resume scheduler matches against — one entry per window the endpoint
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

/// Stable identities for the underlying billing account behind each adapter, so
/// the frontend can dedupe agents that share one subscription (Codex + opencode
/// both bill the same `ChatGPT` account) — one identity per account, never per
/// agent.
const BILLING_ACCOUNT_ANTHROPIC: &str = "anthropic";
const BILLING_ACCOUNT_CHATGPT: &str = "chatgpt";
const BILLING_ACCOUNT_GITHUB: &str = "github";
const BILLING_ACCOUNT_GOOGLE: &str = "google";
const BILLING_ACCOUNT_CURSOR: &str = "cursor";

/// Live account usage mirrored from the OAuth endpoint: every rate-limit window it
/// returns (the session + weekly windows, any per-model caps, and any future
/// windows), plus the plan label.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsage {
    windows: Vec<UsageWindow>,
    plan: String,
    source: String,
    /// Stable identity of the underlying billing account (one of the
    /// `BILLING_ACCOUNT_*` ids), so two agents on one subscription are never
    /// counted twice. `None` when the adapter can't name the account.
    account: Option<String>,
}

/// Live account usage windows, mirroring claude.ai. `None` when offline, `curl` is
/// unavailable, or the token is missing/expired (Claude Code refreshes it on its
/// next run). Cached ~3 min to respect the endpoint's per-token limit.
#[tauri::command]
pub fn usage_account() -> Option<AccountUsage> {
    account_usage_for(UsageAgent::Claude)
}

/// Live account usage windows for a specific agent (`claude`, `codex`) — what the
/// per-agent meter renders. `None` for an agent we have no usage adapter for, or
/// when its token / network isn't available.
#[tauri::command]
pub fn usage_account_agent(agent: String) -> Option<AccountUsage> {
    account_usage_for(UsageAgent::from_id(&agent)?)
}

/// Live account usage for `agent`, cached ~3 min per agent to respect each
/// endpoint's per-token limit. One cache map, keyed by agent — the single source
/// for both the no-arg Claude command and the per-agent one above.
fn account_usage_for(agent: UsageAgent) -> Option<AccountUsage> {
    static CACHE: OnceLock<Mutex<HashMap<UsageAgent, (Instant, AccountUsage)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Ok(guard) = cache.lock() {
        if let Some((fetched_at, cached)) = guard.get(&agent) {
            if fetched_at.elapsed() < Duration::from_secs(USAGE_CACHE_SECS) {
                return Some(cached.clone());
            }
        }
    }

    let fresh = match agent {
        UsageAgent::Claude => fetch_claude_account_usage(),
        UsageAgent::Codex => fetch_codex_account_usage(),
        UsageAgent::Copilot => fetch_copilot_account_usage(),
        UsageAgent::Antigravity => fetch_antigravity_account_usage(),
        UsageAgent::Cursor => fetch_cursor_account_usage(),
    }?;
    if let Ok(mut guard) = cache.lock() {
        guard.insert(agent, (Instant::now(), fresh.clone()));
    }
    Some(fresh)
}

fn fetch_claude_account_usage() -> Option<AccountUsage> {
    let creds = home_dir()?.join(".claude").join(".credentials.json");
    let raw = std::fs::read_to_string(&creds).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let oauth = json.get("claudeAiOauth")?;
    let token = oauth
        .get("accessToken")
        .and_then(serde_json::Value::as_str)?;

    // Don't spend a request on a token we already know is stale.
    if let Some(expires_at) = oauth.get("expiresAt").and_then(serde_json::Value::as_u64) {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis())
            .unwrap_or(0);
        if u128::from(expires_at) <= now_ms {
            return None;
        }
    }

    let plan = oauth
        .get("subscriptionType")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| "Claude".to_string(), |tier| format!("Claude {tier}"));

    let body = curl_oauth_usage(token)?;
    let response: serde_json::Value = serde_json::from_str(&body).ok()?;

    let windows = collect_windows(&response);
    // A non-usage body (e.g. a 401 JSON error) carries no windows.
    if windows.is_empty() {
        return None;
    }

    Some(AccountUsage {
        windows,
        plan,
        source: "oauth:api.anthropic.com".to_string(),
        account: Some(BILLING_ACCOUNT_ANTHROPIC.to_string()),
    })
}

/// Every rate-limit window the account response carries, in a stable order: the
/// named top-level windows (`five_hour`, `seven_day`, and any others the endpoint
/// adds) followed by the per-model caps under `limits[]`. Whatever the API returns
/// — nothing is invented.
fn collect_windows(response: &serde_json::Value) -> Vec<UsageWindow> {
    let mut windows: Vec<UsageWindow> = response
        .as_object()
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| named_window(key, value))
                .collect()
        })
        .unwrap_or_default();
    windows.extend(model_windows(response));
    windows.sort_by_key(|window| window.kind.order());
    windows
}

/// A top-level entry as a window when it is window-shaped — an object carrying a
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
        resets_at: value
            .get("resets_at")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    })
}

/// The per-model caps under `limits[]` — each row scoped to a named model
/// (`scope.model.display_name`), which both identifies it as a per-model window
/// and skips the unscoped session/weekly rows the array may also carry. Every
/// window is surfaced as-is — the frontend (`usage-groups.ts`) shows them all,
/// including any still at 0%.
fn model_windows(response: &serde_json::Value) -> Vec<UsageWindow> {
    response
        .get("limits")
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let utilization = row.get("percent").and_then(serde_json::Value::as_f64)?;
                    let name = row
                        .pointer("/scope/model/display_name")
                        .and_then(serde_json::Value::as_str)?;
                    Some(UsageWindow {
                        key: name.to_string(),
                        kind: UsageWindowKind::Model,
                        label: name.to_string(),
                        utilization,
                        resets_at: row
                            .get("resets_at")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A human label for a named window, sentence-cased from its `snake_case` key
/// (`seven_day` → "Seven day") — the fallback the frontend shows for a window
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
/// character set — non-empty, ASCII alphanumerics plus `.`, `_`, `-` — and
/// reject everything else.
fn is_safe_header_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// GET the usage endpoint via `curl`, feeding every header (incl. the bearer
/// token) through a `--config` file on stdin so the token never appears in the
/// process arguments. `--fail` turns an HTTP error status into a non-zero exit.
///
/// The token is interpolated into a quoted `header = "…"` config directive, so we
/// reject anything outside the OAuth-token character set — see
/// [`is_safe_header_token`].
fn curl_oauth_usage(token: &str) -> Option<String> {
    if !is_safe_header_token(token) {
        return None;
    }

    let config = format!(
        "silent\nshow-error\nfail\nmax-time = 12\nurl = \"{OAUTH_USAGE_URL}\"\n\
         header = \"Authorization: Bearer {token}\"\n\
         header = \"anthropic-beta: oauth-2025-04-20\"\n\
         header = \"User-Agent: {USAGE_UA}\"\n\
         header = \"Content-Type: application/json\"\n"
    );
    curl_get_json(&config)
}

/// Run a prepared curl `--config` document (which already carries the `url` and
/// every `header`, including the bearer token) by feeding it on stdin — so the
/// token never appears in the process arguments. The shared transport behind both
/// vendors' usage calls; each builds its own header set above.
fn curl_get_json(config: &str) -> Option<String> {
    let mut child = crate::util::command("curl")
        .arg("--config")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Write the config, then drop stdin so curl sees EOF and runs the request.
    child.stdin.take()?.write_all(config.as_bytes()).ok()?;

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

// ---------------------------------------------------------------------------
// Codex live account usage — the ChatGPT rate-limit windows the Codex CLI reads
// from `/backend-api/wham/usage` with the locally-stored ChatGPT OAuth token.
// ---------------------------------------------------------------------------

/// The `ChatGPT` backend endpoint the Codex CLI (`codex-rs/backend-client`) polls
/// for rate limits — it sends the `ChatGPT` `Bearer` token and the account id, and
/// answers whether or not any quota is spent, so it's safe at the same ~180s
/// cadence as the Claude endpoint.
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
/// A Codex-CLI-shaped User-Agent, matching what `get_codex_user_agent()` sends.
const CODEX_USAGE_UA: &str = "codex_cli_rs/0.55.0";

/// Codex's two account-wide rate-limit window durations, used to classify each
/// window the endpoint returns into the same session/weekly kinds the meter and
/// the auto-resume scheduler understand. Any other duration passes through as
/// [`UsageWindowKind::Opaque`].
const CODEX_SESSION_WINDOW_SECONDS: i64 = 5 * 60 * 60;
const CODEX_WEEKLY_WINDOW_SECONDS: i64 = 7 * 24 * 60 * 60;

fn fetch_codex_account_usage() -> Option<AccountUsage> {
    let auth = home_dir()?.join(".codex").join("auth.json");
    let raw = std::fs::read_to_string(&auth).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let tokens = json.get("tokens")?;
    let token = tokens
        .get("access_token")
        .and_then(serde_json::Value::as_str)?;
    let account_id = tokens
        .get("account_id")
        .and_then(serde_json::Value::as_str)?;

    let body = curl_codex_usage(token, account_id)?;
    let response: serde_json::Value = serde_json::from_str(&body).ok()?;

    let windows = collect_codex_windows(&response);
    // A non-usage body (e.g. a 401 JSON error) carries no windows.
    if windows.is_empty() {
        return None;
    }

    let plan = response
        .get("plan_type")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| "Codex".to_string(), |plan| format!("Codex {plan}"));

    Some(AccountUsage {
        windows,
        plan,
        source: "oauth:chatgpt.com".to_string(),
        account: Some(BILLING_ACCOUNT_CHATGPT.to_string()),
    })
}

/// Every rate-limit window the Codex usage response carries: the account-wide
/// `rate_limit` primary/secondary caps (classified by duration into session /
/// weekly / opaque), then any per-feature `additional_rate_limits`, each surfaced
/// opaquely under its own feature name. Ordered like the Claude windows.
fn collect_codex_windows(response: &serde_json::Value) -> Vec<UsageWindow> {
    let mut windows: Vec<UsageWindow> = Vec::new();
    if let Some(rate_limit) = response.get("rate_limit") {
        windows.extend(codex_rate_limit_windows(rate_limit, None));
    }
    if let Some(additional) = response
        .get("additional_rate_limits")
        .and_then(serde_json::Value::as_array)
    {
        for entry in additional {
            let feature = entry
                .get("limit_name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("codex");
            if let Some(rate_limit) = entry.get("rate_limit") {
                windows.extend(codex_rate_limit_windows(rate_limit, Some(feature)));
            }
        }
    }
    windows.sort_by_key(|window| window.kind.order());
    windows
}

/// The `primary_window` / `secondary_window` of one `rate_limit` object as usage
/// windows. `feature` is `None` for the account-wide caps (classified by their
/// duration) and `Some(name)` for a named metered feature (surfaced opaquely,
/// so a 5-hour code-review cap isn't mislabeled the account "Session").
fn codex_rate_limit_windows(
    rate_limit: &serde_json::Value,
    feature: Option<&str>,
) -> Vec<UsageWindow> {
    ["primary_window", "secondary_window"]
        .into_iter()
        .filter_map(|slot| {
            rate_limit
                .get(slot)
                .and_then(|window| codex_window(window, feature))
        })
        .collect()
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
    if seconds > 0 && seconds % 86_400 == 0 {
        let days = seconds / 86_400;
        return if days == 7 {
            "Weekly".to_string()
        } else {
            format!("{days}-day")
        };
    }
    if seconds > 0 && seconds % 3_600 == 0 {
        return format!("{}-hour", seconds / 3_600);
    }
    if seconds > 0 && seconds % 60 == 0 {
        return format!("{}-minute", seconds / 60);
    }
    format!("{seconds}-second")
}

/// A Unix timestamp (seconds since the epoch) as an ISO-8601 UTC string
/// (`YYYY-MM-DDTHH:MM:SSZ`). Codex reports window resets as epoch seconds; the
/// frontend countdown parses ISO strings (like the Claude windows), so we
/// normalize here. Pure `std` arithmetic — Howard Hinnant's `civil_from_days`,
/// no date crate.
fn unix_seconds_to_iso8601(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
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

/// GET the Codex usage endpoint via `curl`, feeding the bearer token and account
/// id through a `--config` file on stdin so neither appears in the process
/// arguments. Both are interpolated into quoted `header = "…"` directives, so we
/// reject anything outside the token character set — see [`is_safe_header_token`].
fn curl_codex_usage(token: &str, account_id: &str) -> Option<String> {
    if !is_safe_header_token(token) || !is_safe_header_token(account_id) {
        return None;
    }

    let config = format!(
        "silent\nshow-error\nfail\nmax-time = 12\nurl = \"{CODEX_USAGE_URL}\"\n\
         header = \"Authorization: Bearer {token}\"\n\
         header = \"ChatGPT-Account-Id: {account_id}\"\n\
         header = \"User-Agent: {CODEX_USAGE_UA}\"\n"
    );
    curl_get_json(&config)
}

// ---------------------------------------------------------------------------
// Copilot usage. GitHub's Copilot CLI signs in with a GitHub OAuth token and
// meters "premium requests" against a monthly entitlement. The editor Copilot
// clients read that entitlement from the internal `copilot_internal/user`
// endpoint (the same one VS Code polls to render its usage meter) — an
// entitlement/status read that spends no premium request, so it's safe at the
// same ~180s cadence as the Claude/Codex endpoints. We reach it with the token
// the CLI's precedence order exposes: a `GITHUB_TOKEN`/`GH_TOKEN` env override,
// then the editor OAuth store (`apps.json`/`hosts.json`). The CLI's own default
// store is the OS keychain, which no std-only read can open — so when only the
// keychain holds the token we fall back to a bare "Copilot" plan label.
// ---------------------------------------------------------------------------

/// The internal entitlement endpoint the editor Copilot clients read the user's
/// plan and premium-request quota from. Undocumented but stable, and a status
/// read (not a chat/completion), so polling it costs no premium request.
const COPILOT_USAGE_URL: &str = "https://api.github.com/copilot_internal/user";
/// A Copilot-Chat-shaped `User-Agent`; the endpoint gates on an editor-flavored
/// client identifying itself, like the Claude/Codex endpoints do.
const COPILOT_USAGE_UA: &str = "GitHubCopilotChat/0.26.7";
/// The `Editor-Version` the Copilot endpoints expect from an editor client.
const COPILOT_EDITOR_VERSION: &str = "vscode/1.99.0";
/// Where the fallback plan label comes from when only a local token (no live
/// numbers) is available.
const COPILOT_LOCAL_SOURCE: &str = "local:github-copilot";

/// Copilot usage for the meter — the live premium-request window when the
/// entitlement endpoint answers, else the honest plan label with no numbers.
fn copilot_usage() -> Option<Usage> {
    Some(usage_from_account(account_usage_for(UsageAgent::Copilot)?))
}

/// The Copilot account: live entitlement windows when the endpoint answers,
/// otherwise a bare plan label confirming a local token exists. `None` only when
/// no token is reachable at all (keychain-only login, no env override, no editor
/// OAuth store) — an honest "—".
fn fetch_copilot_account_usage() -> Option<AccountUsage> {
    let token = copilot_token()?;
    if let Some(account) = copilot_live_usage(&token) {
        return Some(account);
    }
    Some(AccountUsage {
        windows: Vec::new(),
        plan: "Copilot".to_string(),
        source: COPILOT_LOCAL_SOURCE.to_string(),
        account: Some(BILLING_ACCOUNT_GITHUB.to_string()),
    })
}

/// Live Copilot entitlement, or `None` when the endpoint is unreachable or the
/// body isn't a usage response (e.g. a 401 error blob carries neither
/// `copilot_plan` nor any quota snapshot).
fn copilot_live_usage(token: &str) -> Option<AccountUsage> {
    let body = curl_copilot_usage(token)?;
    let response: serde_json::Value = serde_json::from_str(&body).ok()?;

    let has_plan = response.get("copilot_plan").is_some();
    let windows = collect_copilot_windows(&response);
    if windows.is_empty() && !has_plan {
        return None;
    }

    let plan = response
        .get("copilot_plan")
        .and_then(serde_json::Value::as_str)
        .map_or_else(
            || "Copilot".to_string(),
            |plan| format!("Copilot {}", plan.replace('_', " ")),
        );
    Some(AccountUsage {
        windows,
        plan,
        source: "oauth:api.github.com".to_string(),
        account: Some(BILLING_ACCOUNT_GITHUB.to_string()),
    })
}

/// The premium-request / chat / completions quota snapshots the Copilot
/// entitlement response carries under `quota_snapshots`, one window each. A
/// snapshot flagged `unlimited` carries no meaningful percentage, so it's
/// skipped; every metered snapshot becomes an opaque window (a monthly
/// entitlement, not one of the session/weekly time windows), sharing the
/// account-wide `quota_reset_date`. Nothing is invented — a snapshot with no
/// derivable percentage falls away.
fn collect_copilot_windows(response: &serde_json::Value) -> Vec<UsageWindow> {
    let resets_at = response
        .get("quota_reset_date")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    response
        .get("quota_snapshots")
        .and_then(serde_json::Value::as_object)
        .map(|snapshots| {
            snapshots
                .iter()
                .filter_map(|(name, snapshot)| copilot_window(name, snapshot, resets_at.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// One Copilot quota snapshot as a [`UsageWindow`], or `None` when it's an
/// unlimited bucket or carries no derivable percent-used. Prefers the endpoint's
/// own `percent_remaining`; otherwise derives it from `remaining`/`entitlement`.
fn copilot_window(
    name: &str,
    snapshot: &serde_json::Value,
    resets_at: Option<String>,
) -> Option<UsageWindow> {
    if snapshot
        .get("unlimited")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
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
        return Some(100.0 - remaining);
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
    Some((1.0 - remaining / entitlement) * 100.0)
}

/// The GitHub token to reach the Copilot entitlement endpoint with, in the
/// precedence the CLI itself uses: the `COPILOT_GITHUB_TOKEN`/`GH_TOKEN`/
/// `GITHUB_TOKEN` env overrides, then the editor OAuth store on disk. The CLI's
/// own default store is the OS keychain, which no dependency-free read can open —
/// so a keychain-only login yields `None` here (and the plan-label fallback).
fn copilot_token() -> Option<String> {
    for variable in ["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(token) = std::env::var(variable) {
            if !token.is_empty() {
                return Some(token);
            }
        }
    }
    copilot_token_from_store()
}

/// The Copilot OAuth token from the editor store — `apps.json` (and the newer
/// `hosts.json`), each an object keyed by host whose values carry an
/// `oauth_token`. The first token found wins. On Windows the store lives under
/// `%USERPROFILE%\AppData\Local\github-copilot`, elsewhere `~/.config/github-copilot`.
fn copilot_token_from_store() -> Option<String> {
    let home = home_dir()?;
    let directory = if cfg!(windows) {
        home.join("AppData/Local/github-copilot")
    } else {
        home.join(".config/github-copilot")
    };
    for file in ["apps.json", "hosts.json"] {
        let Ok(raw) = std::fs::read_to_string(directory.join(file)) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let Some(hosts) = json.as_object() else {
            continue;
        };
        for host in hosts.values() {
            if let Some(token) = host.get("oauth_token").and_then(serde_json::Value::as_str) {
                if !token.is_empty() {
                    return Some(token.to_string());
                }
            }
        }
    }
    None
}

/// GET the Copilot entitlement endpoint via `curl`, feeding the token through a
/// `--config` file on stdin so it never appears in the process arguments. The
/// endpoint uses the `token` (not `Bearer`) authorization scheme and expects an
/// editor `User-Agent` + `Editor-Version`. Rejects anything outside the token
/// character set — see [`is_safe_header_token`].
fn curl_copilot_usage(token: &str) -> Option<String> {
    if !is_safe_header_token(token) {
        return None;
    }

    let config = format!(
        "silent\nshow-error\nfail\nmax-time = 12\nurl = \"{COPILOT_USAGE_URL}\"\n\
         header = \"Authorization: token {token}\"\n\
         header = \"User-Agent: {COPILOT_USAGE_UA}\"\n\
         header = \"Editor-Version: {COPILOT_EDITOR_VERSION}\"\n\
         header = \"Accept: application/json\"\n"
    );
    curl_get_json(&config)
}

// ---------------------------------------------------------------------------
// Antigravity usage (tier-label only). Google's Antigravity CLI stores a Google
// OAuth token under `~/.gemini/antigravity-cli/antigravity-oauth-token`. Its
// remaining-quota does surface from a private `cloudcode-pa.googleapis.com`
// `v1internal:fetchAvailableModels` call, but that endpoint is undocumented,
// reverse-engineered, POST-only, and known to report headroom while generation
// is already rate-limited (a number that would mislead the meter) — so we do NOT
// poll it. We confirm the local login exists and surface just the plan label.
// ---------------------------------------------------------------------------

/// Antigravity usage — the plan label when a local Google login exists, else "—".
fn antigravity_usage() -> Option<Usage> {
    Some(usage_from_account(account_usage_for(
        UsageAgent::Antigravity,
    )?))
}

/// Confirm the Antigravity OAuth token exists on disk and surface a bare plan
/// label with no numbers (no quota-free usage endpoint we trust). `None` when no
/// login is present — an honest "—".
fn fetch_antigravity_account_usage() -> Option<AccountUsage> {
    let token_file = home_dir()?
        .join(".gemini")
        .join("antigravity-cli")
        .join("antigravity-oauth-token");
    let raw = std::fs::read_to_string(&token_file).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    json.pointer("/token/access_token")
        .and_then(serde_json::Value::as_str)?;

    Some(AccountUsage {
        windows: Vec::new(),
        plan: "Antigravity".to_string(),
        source: "local:.gemini/antigravity-cli/antigravity-oauth-token".to_string(),
        account: Some(BILLING_ACCOUNT_GOOGLE.to_string()),
    })
}

// ---------------------------------------------------------------------------
// Cursor usage (tier-label only). The Cursor CLI (`cursor-agent`) stores a JWT
// under a version-dependent path (`~/.config/cursor/auth.json`,
// `~/.cursor/auth.json`, …). Cursor's usage does surface from `cursor.com/api/
// usage`, but only via a reverse-engineered `WorkosCursorSessionToken` *cookie*
// (userId::JWT) that a `CURSOR_API_KEY` can't even build, on an undocumented,
// version-fragile endpoint — too unreliable to poll. We confirm the local login
// exists and surface just the plan label.
// ---------------------------------------------------------------------------

/// Cursor usage — the plan label when a local login exists, else "—".
fn cursor_usage() -> Option<Usage> {
    Some(usage_from_account(account_usage_for(UsageAgent::Cursor)?))
}

/// Confirm a Cursor login exists on disk (or a `CURSOR_API_KEY` override) and
/// surface a bare plan label with no numbers. `None` when no login is present.
fn fetch_cursor_account_usage() -> Option<AccountUsage> {
    if !cursor_login_present() {
        return None;
    }
    Some(AccountUsage {
        windows: Vec::new(),
        plan: "Cursor".to_string(),
        source: "local:cursor".to_string(),
        account: Some(BILLING_ACCOUNT_CURSOR.to_string()),
    })
}

/// Whether a Cursor login is present: the documented `CURSOR_API_KEY` override,
/// or a stored session in one of the CLI's version-dependent auth files (a JWT
/// under `accessToken`/`access_token`, or the `authInfo` marker the CLI writes to
/// `cli-config.json`). We probe a candidate list because the layout drifts
/// between `cursor-agent` versions; a wrong guess simply yields "not present".
fn cursor_login_present() -> bool {
    if std::env::var("CURSOR_API_KEY").is_ok_and(|key| !key.is_empty()) {
        return true;
    }
    let Some(home) = home_dir() else {
        return false;
    };
    let candidates = [
        home.join(".config/cursor/auth.json"),
        home.join(".cursor/auth.json"),
        home.join(".config/cursor-agent/auth.json"),
        home.join("AppData/Roaming/cursor-agent/auth.json"),
        home.join(".cursor/cli-config.json"),
    ];
    candidates.iter().any(|path| auth_file_has_login(path))
}

/// Whether an auth file names a stored login — a non-empty `accessToken`/
/// `access_token` JWT, or the `authInfo` block the CLI writes once signed in.
fn auth_file_has_login(path: &std::path::Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
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
        collect_codex_windows, collect_copilot_windows, collect_windows, humanize_key,
        humanize_window_seconds, is_safe_header_token, unix_seconds_to_iso8601, usage_from_account,
        AccountUsage, UsageAgent, UsageWindow, UsageWindowKind, AGENT_ID_CODEX, AGENT_ID_OPENCODE,
        BILLING_ACCOUNT_GOOGLE,
    };

    #[test]
    fn opencode_resolves_to_the_codex_adapter() {
        // One ChatGPT subscription, one source of truth: both agent ids read the
        // same adapter, so both surface the same account.
        assert!(matches!(
            UsageAgent::from_id(AGENT_ID_OPENCODE),
            Some(UsageAgent::Codex)
        ));
        assert!(matches!(
            UsageAgent::from_id(AGENT_ID_CODEX),
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
        // its own name — never mislabeled the account "Session".
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

        // Only the metered snapshot surfaces — the unlimited buckets carry no
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
        // A 401 error blob is not a usage response — no snapshots, no windows.
        let error_body = serde_json::json!({ "message": "Bad credentials" });

        assert!(collect_copilot_windows(&error_body).is_empty());
    }

    #[test]
    fn usage_from_account_prefers_the_weekly_window() {
        let account = AccountUsage {
            plan: "Copilot business".to_string(),
            source: "oauth:api.github.com".to_string(),
            account: None,
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

        let usage = usage_from_account(account);

        // The weekly cap is the headline even when another window is hotter.
        assert_eq!(usage.used_pct, Some(40.0));
        assert_eq!(usage.resets_at.as_deref(), Some("2026-08-01"));
        assert_eq!(usage.label, "Copilot business");
    }

    #[test]
    fn usage_from_account_falls_back_to_the_most_consumed_window() {
        let account = AccountUsage {
            plan: "Copilot".to_string(),
            source: "oauth:api.github.com".to_string(),
            account: None,
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

        assert_eq!(usage_from_account(account).used_pct, Some(88.0));
    }

    #[test]
    fn usage_from_account_leaves_a_tier_label_account_without_numbers() {
        // The Antigravity / Cursor shape: a plan label, no windows — an honest
        // "we know your login, not your usage".
        let account = AccountUsage {
            plan: "Antigravity".to_string(),
            source: "local:.gemini/antigravity-cli/antigravity-oauth-token".to_string(),
            windows: Vec::new(),
            account: Some(BILLING_ACCOUNT_GOOGLE.to_string()),
        };

        let usage = usage_from_account(account);

        assert_eq!(usage.used_pct, None);
        assert_eq!(usage.resets_at, None);
        assert_eq!(usage.label, "Antigravity");
    }
}
