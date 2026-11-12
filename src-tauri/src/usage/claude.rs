//! Claude usage adapter: the stored OAuth login, the account's windows, credits and prepaid balance, and the claude.ai OAuth request every Claude endpoint shares.

use super::readings::{
    credits_state, AccountUsage, BillingAccount, CreditBalance, CreditsState, NoReadingReason,
    RequestFailure, UsageWindow, UsageWindowKind, CREDITS_KEY,
};
use super::sign_in::{parse_usage_body, read_sign_in, sign_in_field};
use super::transport::{
    curl_json, is_safe_header_token, CurlRequest, AUTHORIZATION_HEADER, USER_AGENT_HEADER,
};
use super::{
    local_source, plan_label, Usage, CLAUDE_OAUTH_SOURCE, CLAUDE_PLAN,
    USAGE_REQUEST_TIMEOUT_SECONDS,
};
use crate::util::{home_directory, now_milliseconds, read_json};
use std::path::PathBuf;

/// The local Claude Code credentials file, `~/.claude/.credentials.json`.
const CLAUDE_CREDENTIALS_RELATIVE_PATH: &str = ".claude/.credentials.json";

fn claude_credentials_path() -> Option<PathBuf> {
    Some(home_directory()?.join(CLAUDE_CREDENTIALS_RELATIVE_PATH))
}

/// Offline fallback: just the subscription tier from the local credentials, with
/// the precise numbers left `None`.
pub(super) fn claude_tier_label_usage() -> Option<Usage> {
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

/// The undocumented endpoint Claude Code's `/usage` reads.
pub(crate) fn claude_usage_url() -> String {
    format!("{CLAUDE_API}{CLAUDE_OAUTH_PATH}/usage")
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

pub(super) fn fetch_claude_account_usage() -> Result<AccountUsage, NoReadingReason> {
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

/// Claude Code's profile, `~/.claude.json`, which records the signed-in account's
/// organization.
const CLAUDE_PROFILE_RELATIVE_PATH: &str = ".claude.json";

/// Claude Code's profile, parsed once. The file also carries every project's
/// history and grows large, so a caller needing several fields reads it once
/// and asks this rather than re-reading per field.
pub(crate) struct ClaudeProfile(pub(super) serde_json::Value);

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
pub(super) fn credit_balance(response: &serde_json::Value) -> Option<CreditBalance> {
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
pub(super) fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// What usage credits have cost this month, when they are on with no monthly
/// cap: `used_credits` in minor units, scaled by the response's own
/// `decimal_places`. `None` while a cap is set - its percentage says it - and
/// for any other shape.
pub(super) fn uncapped_credits_spend(response: &serde_json::Value) -> Option<CreditBalance> {
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
pub(super) fn collect_windows(response: &serde_json::Value) -> Vec<UsageWindow> {
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
pub(super) fn humanize_key(key: &str) -> String {
    let mut label = key.replace('_', " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}

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
