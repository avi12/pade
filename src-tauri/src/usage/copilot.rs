//! Copilot usage adapter: the premium-request entitlement read from GitHub's internal endpoint.

use super::claude::{humanize_key, string_field};
use super::readings::{
    AccountUsage, BillingAccount, NoReadingReason, RequestFailure, UsageWindow, UsageWindowKind,
    FULL_PERCENTAGE,
};
use super::sign_in::parse_usage_body;
use super::transport::{
    curl_json, is_safe_header_token, CurlRequest, AUTHORIZATION_HEADER, USER_AGENT_HEADER,
};
use super::{plan_label, COPILOT_OAUTH_SOURCE, COPILOT_PLAN, USAGE_REQUEST_TIMEOUT_SECONDS};
use crate::util::{home_directory, read_json};

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
pub(super) fn fetch_copilot_account_usage() -> Result<AccountUsage, NoReadingReason> {
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
pub(super) fn collect_copilot_windows(response: &serde_json::Value) -> Vec<UsageWindow> {
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
