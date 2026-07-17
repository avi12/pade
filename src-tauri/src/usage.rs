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
    let Some(account) = account_usage() else {
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

/// Live account usage mirrored from the OAuth endpoint: every rate-limit window it
/// returns (the session + weekly windows, any per-model caps, and any future
/// windows), plus the plan label.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsage {
    windows: Vec<UsageWindow>,
    plan: String,
    source: String,
}

/// Live account usage windows, mirroring claude.ai. `None` when offline, `curl` is
/// unavailable, or the token is missing/expired (Claude Code refreshes it on its
/// next run). Cached ~3 min to respect the endpoint's per-token limit.
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
/// and skips the unscoped session/weekly rows the array may also carry. Display
/// filtering of empty windows is the frontend's job (see `usageGroups.ts`).
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
    use super::{collect_windows, humanize_key, is_safe_header_token, UsageWindowKind};

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
}
