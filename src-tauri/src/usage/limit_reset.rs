//! Claude's free usage-limit resets: a grant the account is given (a launch
//! promotion, say) that clears its session and weekly limits once, on demand.
//! claude.ai shows it as "Limit resets → Reset for free"; this offers the same
//! button in PADE's usage meter.
//!
//! Both calls are the ones Claude Code itself makes. The offer is the `cedar_ember`
//! block of the OAuth usage endpoint - which answers only a caller that names
//! itself as a current Claude Code (any other client is an ineligible
//! "surface"), so the request carries the installed version. The claim is a POST
//! naming the grant and a fresh request id, so a retried claim is never counted
//! twice. Spending a grant cannot be undone; the frontend confirms first.

use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::{
    claude_access_token, claude_usage_url, curl_claude_oauth, is_safe_header_token,
    ClaudeOauthRequest, ClaudeProfile, CLAUDE_API, USAGE_CACHE_TTL,
};

/// The reset program's name, both as the usage query flag and in the claim.
const RESET_PROGRAM: &str = "cedar_ember";
/// How long an offer or claim request may take before curl gives up.
const RESET_REQUEST_TIMEOUT_SECONDS: u32 = 20;
/// The longest grant id Claude Code itself accepts.
const MAXIMUM_GRANT_ID_LENGTH: usize = 40;

/// The reset an account can spend now: the grant the server names as next.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LimitReset {
    grant_id: String,
    /// The grant's own description ("Claude Opus 5.5 launch: one usage-limit
    /// reset for Pro and Max").
    label: String,
    resets_left: u32,
    /// When the grant expires, ISO-8601.
    ends_at: Option<String>,
    /// The windows a reset clears (`five_hour`, `seven_day`, …).
    clears: Vec<String>,
}

/// What a claim did, as the server reports it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct LimitResetOutcome {
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
}

/// The reset the account can spend now, or `None` when it has none (or Claude
/// isn't signed in, or the endpoint can't be reached).
#[tauri::command]
pub async fn limit_reset_offer() -> Option<LimitReset> {
    tauri::async_runtime::spawn_blocking(fetch_offer)
        .await
        .ok()
        .flatten()
}

/// Spend `grant_id` to reset the account's usage limits.
#[tauri::command]
pub async fn limit_reset_claim(grant_id: String) -> Result<LimitResetOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || claim(&grant_id))
        .await
        .map_err(|error| error.to_string())?
}

/// The last offer the endpoint answered, and when. Cached as long as a meter
/// reading ([`USAGE_CACHE_TTL`]): the offer comes off the same rate-limited
/// usage endpoint, and asking on every reading as well got the endpoint
/// refusing both.
static OFFER_CACHE: Mutex<Option<(Instant, Option<LimitReset>)>> = Mutex::new(None);

/// The offer, from the cache while it is fresh. A failed request (offline, or
/// the endpoint refusing a burst) keeps the last answer rather than hiding an
/// offer the account still holds.
fn fetch_offer() -> Option<LimitReset> {
    let cached = OFFER_CACHE.lock().ok().and_then(|guard| guard.clone());
    if let Some((read_at, offer)) = &cached {
        if read_at.elapsed() < USAGE_CACHE_TTL {
            return offer.clone();
        }
    }

    let Ok(offer) = request_offer() else {
        return cached.and_then(|(_, offer)| offer);
    };
    if let Ok(mut guard) = OFFER_CACHE.lock() {
        *guard = Some((Instant::now(), offer.clone()));
    }
    offer
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

/// Ask the endpoint for the offer - `Ok(None)` when it answered that there is
/// nothing to spend.
fn request_offer() -> Result<Option<LimitReset>, OfferUnanswered> {
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
    .ok_or(OfferUnanswered)?;
    let response: serde_json::Value = serde_json::from_str(&body).map_err(|_| OfferUnanswered)?;
    Ok(response
        .get(RESET_PROGRAM)
        .and_then(|status| ResetStatus::deserialize(status).ok())
        .and_then(offered_reset))
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
    .ok_or("the reset request failed")?;
    forget_offer();
    serde_json::from_str(&response).map_err(|error| error.to_string())
}

/// The grant the server names as next, while the account may spend it now.
fn offered_reset(status: ResetStatus) -> Option<LimitReset> {
    if !status.is_eligible {
        return None;
    }

    let next_grant_id = status.next_grant_id?;
    let grant = status
        .grants
        .into_iter()
        .find(|grant| grant.id == next_grant_id)?;
    let is_spendable = grant.is_usable_now && grant.resets_left > 0;
    is_spendable.then_some(LimitReset {
        grant_id: grant.id,
        label: grant.label,
        resets_left: grant.resets_left,
        ends_at: grant.ends_at,
        clears: grant.clears,
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
mod tests {
    use super::{is_grant_id, offered_reset, LimitReset, LimitResetOutcome, ResetStatus};

    fn status(json: &serde_json::Value) -> ResetStatus {
        serde_json::from_value(json.clone()).expect("a status")
    }

    #[test]
    fn offers_the_next_grant_while_it_is_usable() {
        // The real shape, probed live from a Max account.
        let offer = offered_reset(status(&serde_json::json!({
            "eligible": true,
            "at_limit": true,
            "grants": [{
                "id": "opus55-launch-promax-20260921",
                "label": "Claude Opus 5.5 launch: one usage-limit reset for Pro and Max",
                "resets_total": 1,
                "resets_left": 1,
                "ends_at": "2026-10-22T16:00:00+00:00",
                "clears": ["five_hour", "seven_day", "seven_day_overage_included"],
                "usable_now": true
            }],
            "next_grant_id": "opus55-launch-promax-20260921"
        })));

        assert_eq!(
            offer,
            Some(LimitReset {
                grant_id: "opus55-launch-promax-20260921".to_string(),
                label: "Claude Opus 5.5 launch: one usage-limit reset for Pro and Max".to_string(),
                resets_left: 1,
                ends_at: Some("2026-10-22T16:00:00+00:00".to_string()),
                clears: vec![
                    "five_hour".to_string(),
                    "seven_day".to_string(),
                    "seven_day_overage_included".to_string()
                ],
            })
        );
    }

    #[test]
    fn offers_nothing_to_an_ineligible_or_spent_account() {
        let ineligible = status(&serde_json::json!({
            "eligible": false,
            "ineligible_reason": "surface",
            "grants": [],
            "next_grant_id": null
        }));
        let spent = status(&serde_json::json!({
            "eligible": true,
            "grants": [{ "id": "g", "label": "g", "resets_left": 0, "usable_now": true }],
            "next_grant_id": "g"
        }));

        assert_eq!(offered_reset(ineligible), None);
        assert_eq!(offered_reset(spent), None);
    }

    #[test]
    fn a_claim_outcome_keeps_the_servers_field_names_on_both_sides() {
        let outcome: LimitResetOutcome = serde_json::from_value(serde_json::json!({
            "reset": false,
            "result": "already_used",
            "reason": "spent"
        }))
        .expect("the server's shape parses");

        let wire = serde_json::to_value(&outcome).expect("serializes");
        assert_eq!(wire["reset"], serde_json::json!(false));
        assert_eq!(wire["result"], serde_json::json!("already_used"));
    }

    #[test]
    fn a_grant_id_is_lowercase_word_characters_only() {
        assert!(is_grant_id("opus55-launch-promax-20260921"));
        assert!(!is_grant_id("bad\"id"));
        assert!(!is_grant_id(""));
    }
}
