//! Codex usage adapter: the `ChatGPT` rate-limit windows read from the Codex CLI's own backend endpoint.

use super::claude::humanize_key;
use super::readings::{
    AccountUsage, BillingAccount, NoReadingReason, RequestFailure, UsageWindow, UsageWindowKind,
};
use super::sign_in::{parse_usage_body, read_sign_in, sign_in_field};
use super::transport::{
    curl_json, is_safe_header_token, CurlRequest, AUTHORIZATION_HEADER, USER_AGENT_HEADER,
};
use super::window_time::{
    humanize_window_seconds, unix_seconds_to_iso8601, window_kind_for_duration,
};
use super::{
    local_source, plan_label, Usage, CODEX_OAUTH_SOURCE, CODEX_PLAN, USAGE_REQUEST_TIMEOUT_SECONDS,
};
use crate::util::read_json;
use std::path::PathBuf;

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
pub(super) fn codex_tier_label_usage() -> Option<Usage> {
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

/// The feature name a Codex per-feature limit falls back to when it names none.
const CODEX_UNNAMED_FEATURE: &str = "codex";

pub(super) fn fetch_codex_account_usage() -> Result<AccountUsage, NoReadingReason> {
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
pub(super) fn collect_codex_windows(response: &serde_json::Value) -> Vec<UsageWindow> {
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
            window_kind_for_duration(seconds),
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
