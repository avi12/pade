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

use std::path::PathBuf;

use serde::Serialize;

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

/// The user's home directory, cross-platform, without pulling in a dependency.
fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from)
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
