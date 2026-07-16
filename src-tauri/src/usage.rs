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

use std::io::Write;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::util::home_dir;

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
    match agent.as_str() {
        "claude" => claude_usage(),
        _ => None,
    }
}

/// Claude usage for the meter. Prefers the live account windows (the real weekly
/// %, mirroring claude.ai) and falls back to the honest subscription-tier label
/// when the network / token isn't available.
fn claude_usage() -> Option<Usage> {
    if let Some(account) = account_usage() {
        let (used_pct, resets_at) = account.seven_day.as_ref().map_or((None, None), |week| {
            (Some(week.utilization), week.resets_at.clone())
        });
        return Some(Usage {
            used_pct,
            label: account.plan,
            resets_at,
            source: account.source,
        });
    }
    tier_label_usage()
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
// Live account usage — the same 5h + weekly windows claude.ai / `claude /usage`
// show, read from the OAuth endpoint using the local Claude Code token.
// ---------------------------------------------------------------------------

/// The undocumented endpoint Claude Code's `/usage` reads. It requires the
/// `claude-code/<version>` User-Agent (any other UA lands in an aggressively
/// rate-limited bucket) and is safe at ~180s intervals — hence the cache below.
const OAUTH_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const USAGE_UA: &str = "claude-code/1.0.128";
const USAGE_CACHE_SECS: u64 = 170;

/// One rate-limit window (the 5-hour session or the 7-day weekly): a 0..100
/// utilization percent and an ISO-8601 reset time — the shape claude.ai reports.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    utilization: f64,
    resets_at: Option<String>,
}

/// A per-model weekly window (e.g. Opus / Fable have their own weekly caps on
/// some plans). Only the ones actually in use are surfaced — see the `> 0` filter.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    name: String,
    utilization: f64,
    resets_at: Option<String>,
}

/// Live account usage mirrored from the OAuth endpoint: the 5-hour session
/// window, the 7-day weekly window, any non-zero per-model weekly windows, and
/// the plan label.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsage {
    five_hour: Option<UsageWindow>,
    seven_day: Option<UsageWindow>,
    models: Vec<ModelUsage>,
    plan: String,
    source: String,
}

/// Live 5-hour + 7-day usage windows, mirroring claude.ai. `None` when offline,
/// `curl` is unavailable, or the token is missing/expired (Claude Code refreshes
/// it on its next run). Cached ~3 min to respect the endpoint's per-token limit.
#[tauri::command]
pub fn usage_account() -> Option<AccountUsage> {
    account_usage()
}

fn account_usage() -> Option<AccountUsage> {
    static CACHE: OnceLock<Mutex<Option<(Instant, AccountUsage)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));

    if let Ok(guard) = cache.lock() {
        if let Some((fetched_at, cached)) = guard.as_ref() {
            if fetched_at.elapsed() < Duration::from_secs(USAGE_CACHE_SECS) {
                return Some(cached.clone());
            }
        }
    }

    let fresh = fetch_account_usage()?;
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((Instant::now(), fresh.clone()));
    }
    Some(fresh)
}

fn fetch_account_usage() -> Option<AccountUsage> {
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
    let resp: serde_json::Value = serde_json::from_str(&body).ok()?;
    let window = |key: &str| -> Option<UsageWindow> {
        let raw_window = resp.get(key)?;
        Some(UsageWindow {
            utilization: raw_window
                .get("utilization")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0),
            resets_at: raw_window
                .get("resets_at")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        })
    };

    let five_hour = window("five_hour");
    let seven_day = window("seven_day");
    // A non-usage body (e.g. a 401 JSON error) carries neither window.
    if five_hour.is_none() && seven_day.is_none() {
        return None;
    }

    // Per-model weekly caps live in `limits[]` as `weekly_scoped` rows; keep only
    // the ones with a named model that are actually in use (> 0%).
    let models = resp
        .get("limits")
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let percent = row.get("percent").and_then(serde_json::Value::as_f64)?;
                    if percent <= 0.0 {
                        return None;
                    }
                    let name = row
                        .pointer("/scope/model/display_name")
                        .and_then(serde_json::Value::as_str)?;
                    Some(ModelUsage {
                        name: name.to_string(),
                        utilization: percent,
                        resets_at: row
                            .get("resets_at")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Some(AccountUsage {
        five_hour,
        seven_day,
        models,
        plan,
        source: "oauth:api.anthropic.com".to_string(),
    })
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

#[cfg(test)]
mod tests {
    use super::is_safe_header_token;

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
}
