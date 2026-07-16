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
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::util::{encode_project, home_dir};

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

/// GET the usage endpoint via `curl`, feeding every header (incl. the bearer
/// token) through a `--config` file on stdin so the token never appears in the
/// process arguments. `--fail` turns an HTTP error status into a non-zero exit.
///
/// The token is interpolated into a quoted `header = "…"` config directive, so we
/// reject anything outside the OAuth-token character set — a stray quote or
/// newline could otherwise close the string and inject an arbitrary directive.
fn curl_oauth_usage(token: &str) -> Option<String> {
    let is_safe_token = !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if !is_safe_token {
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

// ---------------------------------------------------------------------------
// Context-window fill — the one exact "percent" we can source locally.
// ---------------------------------------------------------------------------

/// The active session's context-window state, sourced locally.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsage {
    /// Fill of the context window as a percent (0..100).
    pct: f64,
    /// The model in play, best-effort (e.g. "claude-sonnet-4-6"); empty if unknown.
    model: String,
    /// Tokens of context in play right now (input + cache), when parseable.
    used_tokens: Option<u64>,
    /// The model's context window in tokens, when parseable.
    limit_tokens: Option<u64>,
    /// User + assistant message count in the transcript, when parseable.
    messages: Option<u64>,
    /// Epoch-ms of the session's first timestamped entry, when parseable.
    started_at: Option<u64>,
}

/// Context-window fill for the most recent Claude session in `cwd`, read from its
/// local transcript — no network, no CLI. `None` when there is no readable session
/// log yet.
///
/// Claude Code writes a JSONL transcript per session under
/// `~/.claude/projects/<encoded-cwd>/`; each assistant message carries a
/// `message.usage` with exact token counts. The context in play is the input side
/// (`input_tokens` + cache creation + cache read); we divide it by the model's
/// window (1M when the model advertises it, else 200k) for a real percentage.
#[tauri::command]
pub fn usage_session(cwd: String) -> Option<SessionUsage> {
    let dir = home_dir()?
        .join(".claude")
        .join("projects")
        .join(encode_project(&cwd));
    let log = latest_jsonl(&dir)?;
    let raw = std::fs::read_to_string(&log).ok()?;

    // Session-wide stats parsed from the whole transcript (best-effort, never
    // fabricated): the message count and the first entry's start timestamp.
    let messages = count_messages(&raw);
    let started_at = first_timestamp_ms(&raw);

    // The newest assistant message with usage reflects the current context.
    for line in raw.lines().rev() {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(usage) = json.pointer("/message/usage") else {
            continue;
        };
        let field = |key: &str| {
            usage
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        };
        let used = field("input_tokens")
            + field("cache_creation_input_tokens")
            + field("cache_read_input_tokens");
        if used == 0 {
            continue;
        }

        let model = json
            .pointer("/message/model")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let window: u64 = if model.to_lowercase().contains("1m") {
            1_000_000
        } else {
            200_000
        };
        // Token counts are far below f64's exact-integer range, so no precision loss.
        #[allow(clippy::cast_precision_loss)]
        let pct = (used as f64 / window as f64) * 100.0;
        return Some(SessionUsage {
            pct: pct.min(100.0),
            model: model.to_string(),
            used_tokens: Some(used),
            limit_tokens: Some(window),
            messages,
            started_at,
        });
    }

    None
}

/// Count the user + assistant messages in a JSONL transcript. `None` when the
/// transcript has no recognizable message entries at all.
fn count_messages(raw: &str) -> Option<u64> {
    let count = raw
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|json| {
            matches!(
                json.get("type").and_then(serde_json::Value::as_str),
                Some("user" | "assistant")
            )
        })
        .count();
    (count > 0).then_some(count.try_into().unwrap_or(u64::MAX))
}

/// Epoch-ms of the first entry that carries an ISO-8601 `timestamp`. `None` when
/// none is present or parseable — we never fabricate a start time.
fn first_timestamp_ms(raw: &str) -> Option<u64> {
    raw.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|json| {
            json.get("timestamp")
                .and_then(serde_json::Value::as_str)
                .and_then(iso8601_to_epoch_ms)
        })
}

/// Parse a fixed-shape UTC ISO-8601 timestamp (`YYYY-MM-DDTHH:MM:SS[.mmm]Z`) to
/// epoch milliseconds, dependency-free. `None` on any shape we don't recognize —
/// the caller then leaves `startedAt` unset rather than guessing.
fn iso8601_to_epoch_ms(ts: &str) -> Option<u64> {
    let date_time = ts.strip_suffix('Z')?;
    let (date, rest) = date_time.split_once('T')?;
    let mut d = date.split('-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;

    // Time is HH:MM:SS with an optional fractional part.
    let (hms, millis) = match rest.split_once('.') {
        Some((hms, fraction)) => {
            // Interpret the fraction as milliseconds: keep the first three digits,
            // right-padding shorter fractions so ".5" is 500ms (not 5ms).
            let mut fraction_digits = fraction.get(..3).unwrap_or(fraction).to_string();
            while fraction_digits.len() < 3 {
                fraction_digits.push('0');
            }
            let milliseconds: u64 = fraction_digits.parse().ok()?;
            (hms, milliseconds)
        }
        None => (rest, 0),
    };
    let mut t = hms.split(':');
    let hour: u64 = t.next()?.parse().ok()?;
    let minute: u64 = t.next()?.parse().ok()?;
    let second: u64 = t.next()?.parse().ok()?;

    let days = days_from_civil(year, month, day)?;
    let secs = days * 86_400 + hour * 3_600 + minute * 60 + second;
    Some(secs * 1_000 + millis)
}

/// Days since the Unix epoch (1970-01-01) for a proleptic-Gregorian date, via
/// Howard Hinnant's `days_from_civil` algorithm. `None` for pre-epoch dates,
/// which can't occur in a live transcript.
fn days_from_civil(year: i64, month: u32, day: u32) -> Option<u64> {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(month);
    let d = i64::from(day);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days).ok()
}

/// The most-recently-modified `*.jsonl` in `dir` — the active session, if any.
fn latest_jsonl(dir: &Path) -> Option<PathBuf> {
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let is_newer = match &newest {
            Some((newest_time, _)) => modified > *newest_time,
            None => true,
        };
        if is_newer {
            newest = Some((modified, path));
        }
    }
    newest.map(|(_, path)| path)
}
