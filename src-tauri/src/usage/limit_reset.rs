//! Claude's free usage-limit resets: grants the account is given (a launch
//! promotion, a monthly full reset) that each clear some of its limits once, on
//! demand. claude.ai shows them as "Limit resets → Reset for free", one button
//! per kind - a weekly reset apart from a Fable one - greyed out while the
//! limits a kind clears hold no use; this offers the same in PADE's usage meter.
//!
//! Both calls are the ones Claude Code itself makes. The offer is the `cedar_ember`
//! block of the OAuth usage endpoint - which answers only a caller that names
//! itself as a current Claude Code (any other client is an ineligible
//! "surface"), so the request carries the installed version. The claim is a POST
//! naming the grant and a fresh request id, so a retried claim is never counted
//! twice. Spending a grant cannot be undone; the frontend confirms first.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Deserializer, Serialize};

use super::claude::{
    claude_access_token, claude_usage_url, curl_claude_oauth, ClaudeOauthRequest, ClaudeProfile,
    CLAUDE_API,
};
use super::transport::is_safe_header_token;
use super::USAGE_CACHE_TTL;

/// The reset program's name, both as the usage query flag and in the claim.
const RESET_PROGRAM: &str = "cedar_ember";
/// How long an offer or claim request may take before curl gives up.
const RESET_REQUEST_TIMEOUT_SECONDS: u32 = 20;
/// The longest grant id Claude Code itself accepts.
const MAXIMUM_GRANT_ID_LENGTH: usize = 40;
/// The 5-hour window's key in a grant's `clears`.
const SESSION_WINDOW: &str = "five_hour";

/// Which limits a reset frees up, named as Claude Code names it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ResetKind {
    Session,
    Weekly,
    /// Clears the included model's week and gives its use back to the weekly
    /// limit.
    Fable,
    Full,
}

/// One kind of reset the account can spend now.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LimitReset {
    /// The grant a claim of this kind spends.
    grant_id: String,
    /// That grant's own description ("New model launch: one Fable reset").
    label: String,
    resets_left: u32,
    /// When the first of them expires, ISO-8601.
    ends_at: Option<String>,
    kind: ResetKind,
    /// Some limit it clears has use on it now. claude.ai greys out a reset
    /// that would clear nothing.
    has_usage_to_clear: bool,
}

/// What a claim did, as the server reports it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all(serialize = "camelCase"))]
pub(crate) struct LimitResetOutcome {
    /// Whether the limits were actually reset.
    #[serde(rename = "reset")]
    is_reset: bool,
    /// The server's verdict (`reset`, `already_used`, `ineligible`, …).
    result: String,
    /// Why it did not reset, when it says.
    reason: Option<String>,
}

#[derive(Deserialize)]
struct ResetStatus {
    #[serde(rename = "eligible")]
    is_eligible: bool,
    next_grant_id: Option<String>,
    #[serde(default)]
    grants: Vec<Grant>,
}

#[derive(Deserialize)]
struct Grant {
    id: String,
    label: String,
    resets_left: u32,
    ends_at: Option<String>,
    #[serde(default)]
    clears: Vec<String>,
    #[serde(rename = "usable_now")]
    is_usable_now: bool,
    /// The kind the server names, when it names one this knows.
    #[serde(rename = "type", default, deserialize_with = "known_kind")]
    declared_kind: Option<ResetKind>,
    /// Spending it also takes the week's included-model use off the weekly
    /// limit - which makes it a Fable reset, whatever its `type`.
    #[serde(rename = "gives_back_weekly", default)]
    is_giving_back_weekly: bool,
    /// How much of each window it clears is used now, 0-100.
    #[serde(default)]
    percent_used: HashMap<String, u32>,
}

/// A `type` this build doesn't know reads as none, as Claude Code reads it,
/// rather than failing the whole status block.
fn known_kind<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<ResetKind>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(ResetKind::deserialize(value).ok())
}

/// The resets the account can spend now, one per kind - empty when it has
/// none (or Claude isn't signed in, or the endpoint can't be reached).
#[tauri::command]
pub(crate) async fn limit_reset_offers() -> Vec<LimitReset> {
    tauri::async_runtime::spawn_blocking(fetch_offers)
        .await
        .unwrap_or_default()
}

/// Spend `grant_id` to reset the account's usage limits.
#[tauri::command]
pub(crate) async fn limit_reset_claim(grant_id: String) -> Result<LimitResetOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || claim(&grant_id))
        .await
        .map_err(|error| error.to_string())?
}

/// The last offers the endpoint answered, and when. Cached as long as a meter
/// reading ([`USAGE_CACHE_TTL`]): the offers come off the same rate-limited
/// usage endpoint, and asking on every reading as well got the endpoint
/// refusing both.
static OFFER_CACHE: Mutex<Option<(Instant, Vec<LimitReset>)>> = Mutex::new(None);

/// The offers, from the cache while it is fresh. A failed request (offline, or
/// the endpoint refusing a burst) keeps the last answer rather than hiding
/// resets the account still holds.
fn fetch_offers() -> Vec<LimitReset> {
    let cached = OFFER_CACHE.lock().ok().and_then(|guard| guard.clone());
    if let Some((read_at, offers)) = &cached {
        if read_at.elapsed() < USAGE_CACHE_TTL {
            return offers.clone();
        }
    }

    let Ok(offers) = request_offers() else {
        return cached.map(|(_, offers)| offers).unwrap_or_default();
    };
    if let Ok(mut guard) = OFFER_CACHE.lock() {
        *guard = Some((Instant::now(), offers.clone()));
    }
    offers
}

/// Forget the cached offer, so the next read asks again - after a claim.
fn forget_offer() {
    if let Ok(mut guard) = OFFER_CACHE.lock() {
        *guard = None;
    }
}

/// The offer request got no answer: offline, signed out, or refused.
#[derive(Debug)]
struct OfferUnanswered;

/// Ask the endpoint for the offers - empty when it answered that there is
/// nothing to spend.
fn request_offers() -> Result<Vec<LimitReset>, OfferUnanswered> {
    let url = format!("{}?{RESET_PROGRAM}=1&skip_spend=1", claude_usage_url());
    let body = curl_claude_oauth(&ClaudeOauthRequest {
        url: &url,
        token: &claude_access_token().ok_or(OfferUnanswered)?,
        user_agent: &ClaudeProfile::read()
            .as_ref()
            .and_then(claude_cli_user_agent)
            .ok_or(OfferUnanswered)?,
        organization: None,
        json_body: None,
        timeout_seconds: RESET_REQUEST_TIMEOUT_SECONDS,
    })
    .map_err(|_| OfferUnanswered)?;
    let response: serde_json::Value = serde_json::from_str(&body).map_err(|_| OfferUnanswered)?;
    Ok(response
        .get(RESET_PROGRAM)
        .and_then(|status| ResetStatus::deserialize(status).ok())
        .map(offered_resets)
        .unwrap_or_default())
}

fn claim(grant_id: &str) -> Result<LimitResetOutcome, String> {
    if !is_grant_id(grant_id) {
        return Err(format!("not a reset grant id: {grant_id}"));
    }

    let token = claude_access_token().ok_or("Claude is not signed in")?;
    if !is_safe_header_token(&token) {
        return Err("unsafe request".to_string());
    }
    let profile = ClaudeProfile::read();
    let organization = profile
        .as_ref()
        .and_then(ClaudeProfile::organization)
        .filter(|organization| is_safe_header_token(organization))
        .ok_or("no Claude organization")?;
    let user_agent = profile
        .as_ref()
        .and_then(claude_cli_user_agent)
        .ok_or("Claude Code's version is unknown")?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let body = serde_json::json!({
        "program": RESET_PROGRAM,
        "grant_id": grant_id,
        "request_id": request_id,
    })
    .to_string();
    let url = format!("{CLAUDE_API}/organizations/{organization}/reset_rate_limits");
    let response = curl_claude_oauth(&ClaudeOauthRequest {
        url: &url,
        token: &token,
        user_agent: &user_agent,
        organization: None,
        json_body: Some(&body),
        timeout_seconds: RESET_REQUEST_TIMEOUT_SECONDS,
    })
    .map_err(|_| "the reset request failed")?;
    forget_offer();
    serde_json::from_str(&response).map_err(|error| error.to_string())
}

impl Grant {
    fn is_spendable(&self) -> bool {
        self.is_usable_now && self.resets_left > 0
    }

    /// Its kind, decided as Claude Code decides it: giving the week back makes
    /// it a Fable reset, then the server's own `type`, and only then the
    /// windows it clears.
    fn kind(&self) -> ResetKind {
        if self.is_giving_back_weekly {
            return ResetKind::Fable;
        }

        if let Some(kind) = self.declared_kind {
            return kind;
        }

        let clears_session = self.clears.iter().any(|window| window == SESSION_WINDOW);
        let clears_longer = self.clears.iter().any(|window| window != SESSION_WINDOW);
        match (clears_session, clears_longer) {
            (true, false) => ResetKind::Session,
            (false, true) => ResetKind::Weekly,
            _ => ResetKind::Full,
        }
    }

    /// Some window it clears holds use now. A window the server gives no
    /// figure for counts as used, so an unknown never greys a reset out.
    fn has_usage_to_clear(&self) -> bool {
        self.clears.is_empty()
            || self.clears.iter().any(|window| {
                self.percent_used
                    .get(window)
                    .is_none_or(|percent| *percent > 0)
            })
    }
}

/// One offer per kind of reset the account can spend now. One launch can
/// hand out several grants of a kind (a base grant plus tenure bonuses), and
/// claude.ai counts them together ("Fable reset · 3"); different kinds (a
/// weekly reset beside a Fable one) stay apart, each spent on its own.
///
/// Each offer spends the grant that runs out first in its kind - the server's
/// `next_grant_id` when it is one of them - and the server's next kind leads.
fn offered_resets(status: ResetStatus) -> Vec<LimitReset> {
    if !status.is_eligible {
        return Vec::new();
    }

    // One pass: each spendable grant joins its kind's group. Groups keep the
    // server's order (a handful of grants, so a linear lookup beats hashing).
    let mut groups: Vec<(ResetKind, Vec<&Grant>)> = Vec::new();
    for grant in status.grants.iter().filter(|grant| grant.is_spendable()) {
        let kind = grant.kind();
        if let Some((_, grants)) = groups.iter_mut().find(|(existing, _)| *existing == kind) {
            grants.push(grant);
        } else {
            groups.push((kind, vec![grant]));
        }
    }

    let next_grant_id = status.next_grant_id.as_deref();
    groups.sort_by_key(|(_, grants)| !grants.iter().any(|grant| is_grant(grant, next_grant_id)));
    groups
        .iter()
        .filter_map(|(kind, grants)| offer_for_kind(*kind, grants, next_grant_id))
        .collect()
}

fn is_grant(grant: &Grant, grant_id: Option<&str>) -> bool {
    Some(grant.id.as_str()) == grant_id
}

/// One kind's offer: every reset its grants hold, the earliest expiry, and
/// the grant a claim spends - the server's next when it is of this kind, or
/// else the one that runs out first.
fn offer_for_kind(
    kind: ResetKind,
    grants: &[&Grant],
    next_grant_id: Option<&str>,
) -> Option<LimitReset> {
    let first_expiring = grants
        .iter()
        .min_by_key(|grant| (grant.ends_at.is_none(), grant.ends_at.as_deref()))?;
    let spent_first = grants
        .iter()
        .find(|grant| is_grant(grant, next_grant_id))
        .unwrap_or(first_expiring);
    Some(LimitReset {
        grant_id: spent_first.id.clone(),
        label: spent_first.label.clone(),
        resets_left: grants.iter().map(|grant| grant.resets_left).sum(),
        ends_at: first_expiring.ends_at.clone(),
        kind,
        has_usage_to_clear: spent_first.has_usage_to_clear(),
    })
}

/// The shape Claude Code itself accepts for a grant id - also what keeps it
/// safe inside the request body.
fn is_grant_id(grant_id: &str) -> bool {
    let has_valid_length = (1..=MAXIMUM_GRANT_ID_LENGTH).contains(&grant_id.len());
    has_valid_length
        && grant_id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

/// How Claude Code names itself, with the installed version.
fn claude_cli_user_agent(profile: &ClaudeProfile) -> Option<String> {
    let version = profile.cli_version()?;
    let is_version = !version.is_empty()
        && version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.');
    is_version.then(|| format!("claude-cli/{version} (external, cli)"))
}
#[cfg(test)]
mod tests;
