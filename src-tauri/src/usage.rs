//! Agent usage / quota meter.
//!
//! Surfaces how much of the active agent's quota is left, sourced WITHOUT
//! spending any quota — we never invoke the agent CLI and never hit the network.
//! We only read data the agent already persisted locally. Because no vendor
//! exposes a uniform local "remaining %", this is best-effort per agent:
//!  - When a reliable local signal exists we return what we can (e.g. the
//!    subscription tier label), leaving `used_pct`/`resets_at` `None` if the
//!    precise numbers aren't on disk.
//!  - When nothing reliable is found we return `None` and the UI shows an honest
//!    "usage unavailable" state — we never fabricate numbers.
//!
//! The fully-robust source is the vendor's authenticated site (e.g. claude.ai),
//! which needs a logged-in webview — a separate in-progress feature. This module
//! is the plumbing + local best-effort adapter that a site-backed source can
//! later slot into.

use std::path::{Path, PathBuf};

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

/// Remaining usage for `agent`, from local data only. `None` when we have no
/// reliable local signal for that agent (the UI then shows "usage —").
#[tauri::command]
pub fn usage_get(agent: String) -> Option<Usage> {
    match agent.as_str() {
        "claude" => claude_usage(),
        _ => None,
    }
}

/// Best-effort Claude Code usage from `~/.claude`, network- and CLI-free.
///
/// Claude Code does NOT persist a numeric remaining-quota / reset locally: its
/// `stats-cache.json` is only historical activity counts, and session logs hold
/// per-message token counts plus bare `rate_limit` 429 markers — none of which is
/// a quota state we can trust. The one honest local signal is the subscription
/// tier in `~/.claude/.credentials.json` (`claudeAiOauth.subscriptionType`), so
/// we surface that as a label with `used_pct`/`resets_at` left `None`. The real
/// remaining-% source is the claude.ai site (pending the design-webview feature).
fn claude_usage() -> Option<Usage> {
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
        });
    }

    None
}

/// The most-recently-modified `*.jsonl` in `dir` — the active session, if any.
fn latest_jsonl(dir: &Path) -> Option<PathBuf> {
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
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
