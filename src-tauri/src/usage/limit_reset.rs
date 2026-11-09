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

/// The resets the account can spend now, one per kind - empty when it has
/// none (or Claude isn't signed in, or the endpoint can't be reached).
#[tauri::command]
pub async fn limit_reset_offers() -> Vec<LimitReset> {
    tauri::async_runtime::spawn_blocking(fetch_offers)
        .await
        .unwrap_or_default()
}

/// Spend `grant_id` to reset the account's usage limits.
#[tauri::command]
pub async fn limit_reset_claim(grant_id: String) -> Result<LimitResetOutcome, String> {
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
}

/// The limits a grant clears, order-free, so two grants of one kind compare
/// equal however the server lists them.
fn kind_of(grant: &Grant) -> Vec<&str> {
    let mut clears: Vec<&str> = grant.clears.iter().map(String::as_str).collect();
    clears.sort_unstable();
    clears
}

/// One offer per kind of reset the account can spend now - a kind being the
/// set of limits its grants clear. One launch can hand out several grants of
/// a kind (a base grant plus tenure bonuses), and claude.ai counts them
/// together ("Fable reset · 3"); different kinds (a Fable reset beside an
/// all-models one) stay apart so the user picks which to spend.
///
/// Each offer spends the grant that runs out first in its kind - the server's
/// `next_grant_id` when it is one of them - and the server's next kind leads.
fn offered_resets(status: ResetStatus) -> Vec<LimitReset> {
    if !status.is_eligible {
        return Vec::new();
    }

    // One pass: each spendable grant joins its kind's group, its kind worked
    // out once. Groups keep the server's order (a handful of grants, so a
    // linear lookup beats hashing).
    let mut groups: Vec<(Vec<&str>, Vec<&Grant>)> = Vec::new();
    for grant in status.grants.iter().filter(|grant| grant.is_spendable()) {
        let kind = kind_of(grant);
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
        .filter_map(|(_, grants)| offer_for_kind(grants, next_grant_id))
        .collect()
}

fn is_grant(grant: &Grant, grant_id: Option<&str>) -> bool {
    Some(grant.id.as_str()) == grant_id
}

/// One kind's offer: every reset its grants hold, the earliest expiry, and
/// the grant a claim spends - the server's next when it is of this kind, or
/// else the one that runs out first.
fn offer_for_kind(grants: &[&Grant], next_grant_id: Option<&str>) -> Option<LimitReset> {
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
        clears: spent_first.clears.clone(),
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
    use super::{is_grant_id, offered_resets, LimitReset, LimitResetOutcome, ResetStatus};

    fn status(json: &serde_json::Value) -> ResetStatus {
        serde_json::from_value(json.clone()).expect("a status")
    }

    #[test]
    fn offers_the_next_grant_while_it_is_usable() {
        // The real shape, probed live from a Max account.
        let offers = offered_resets(status(&serde_json::json!({
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
            offers,
            vec![LimitReset {
                grant_id: "opus55-launch-promax-20260921".to_string(),
                label: "Claude Opus 5.5 launch: one usage-limit reset for Pro and Max".to_string(),
                resets_left: 1,
                ends_at: Some("2026-10-22T16:00:00+00:00".to_string()),
                clears: vec![
                    "five_hour".to_string(),
                    "seven_day".to_string(),
                    "seven_day_overage_included".to_string()
                ],
            }]
        );
    }

    fn grant(id: &str, resets_left: u32, ends_at: &str, clears: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "label": id,
            "resets_left": resets_left,
            "ends_at": ends_at,
            "clears": clears,
            "usable_now": true
        })
    }

    const FABLE: &[&str] = &["seven_day_overage_included"];
    const ALL_MODELS: &[&str] = &["five_hour", "seven_day"];

    #[test]
    fn counts_every_held_grant_of_the_same_kind() {
        // The real shape, probed live: a launch grant plus two tenure bonuses,
        // all clearing Fable's week, beside a spent one.
        let expiry = "2027-03-09T09:59:00+02:00";
        let offers = offered_resets(status(&serde_json::json!({
            "eligible": true,
            "grants": [
                grant("launch-2610-fable", 1, expiry, FABLE),
                grant("launch-2610-fable-12mo", 1, expiry, FABLE),
                grant("launch-2610-fable-6mo", 1, expiry, FABLE),
                grant("launch-2610-fable-spent", 0, expiry, FABLE)
            ],
            "next_grant_id": "launch-2610-fable"
        })));

        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].grant_id, "launch-2610-fable");
        assert_eq!(offers[0].resets_left, 3);
    }

    #[test]
    fn offers_each_kind_apart_with_the_servers_next_kind_first() {
        let offers = offered_resets(status(&serde_json::json!({
            "eligible": true,
            "grants": [
                grant("all-late", 1, "2027-05-01T00:00:00+00:00", &["seven_day", "five_hour"]),
                grant("fable", 1, "2027-03-09T00:00:00+00:00", FABLE),
                grant("all-early", 1, "2027-04-01T00:00:00+00:00", ALL_MODELS)
            ],
            "next_grant_id": "fable"
        })));

        let summary: Vec<(&str, u32, Option<&str>)> = offers
            .iter()
            .map(|offer| {
                (
                    offer.grant_id.as_str(),
                    offer.resets_left,
                    offer.ends_at.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("fable", 1, Some("2027-03-09T00:00:00+00:00")),
                // Clears listed in any order are one kind; the earliest-expiring
                // grant of it is spent first.
                ("all-early", 2, Some("2027-04-01T00:00:00+00:00"))
            ]
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

        assert_eq!(offered_resets(ineligible), Vec::new());
        assert_eq!(offered_resets(spent), Vec::new());
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
