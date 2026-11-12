use super::claude::{
    collect_windows, credit_balance, humanize_key, uncapped_credits_spend, ClaudeProfile,
};
use super::codex::collect_codex_windows;
use super::copilot::collect_copilot_windows;
use super::muse::{muse_account_usage, muse_failure};
use super::readings::{
    credits_state, AccountReading, AccountUsage, BillingAccount, CreditsState, NoReadingReason,
    RequestFailure, UsageWindow, UsageWindowKind,
};
use super::transport::{curl_config, http_answer, is_safe_header_token, CurlRequest};
use super::window_time::{humanize_window_seconds, unix_seconds_to_iso8601};
use super::{
    usage_from_account, AccountReadings, AccountUsageRequest, HeadlineWindow, Usage, UsageAgent,
    LAST_READING_LIMIT, USAGE_CACHE_TTL,
};
use crate::agents::{ID_CODEX, ID_OPENCODE};
use std::time::Duration;
use std::time::Instant;

const ONE_SECOND: Duration = Duration::from_secs(1);

fn reading(plan: &str) -> AccountUsage {
    AccountUsage {
        plan: plan.to_string(),
        ..AccountUsage::default()
    }
}

fn readings_answered_at(asked_at: Instant) -> AccountReadings {
    let mut readings = AccountReadings::default();
    readings.record(Ok(reading("Max")), asked_at);
    readings
}

#[test]
fn a_fresh_reading_says_nothing_is_missing() {
    let asked_at = Instant::now();
    let reading = readings_answered_at(asked_at).reading(UsageAgent::Codex, asked_at);

    assert_eq!(
        reading.account.map(|usage| usage.plan),
        Some("Max".to_string())
    );
    assert_eq!(reading.no_reading, None);
}

#[test]
fn an_expired_sign_in_names_the_cli_that_refreshes_it() {
    let asked_at = Instant::now();
    let mut readings = AccountReadings::default();
    readings.record(Err(NoReadingReason::SignInExpired), asked_at);

    let reading = readings.reading(UsageAgent::Codex, asked_at);

    assert!(reading.account.is_none());
    let no_reading = reading.no_reading.expect("a reason");
    assert_eq!(no_reading.reason, NoReadingReason::SignInExpired);
    assert_eq!(no_reading.sign_in_command, Some("codex"));
}

#[test]
fn a_failure_after_a_reading_holds_it_over_and_says_why() {
    let first_ask = Instant::now();
    let mut readings = readings_answered_at(first_ask);
    readings.record(
        Err(NoReadingReason::Unreachable),
        first_ask + USAGE_CACHE_TTL,
    );

    let reading = readings.reading(UsageAgent::Claude, first_ask + USAGE_CACHE_TTL);

    assert!(reading.account.is_some());
    assert_eq!(
        reading.no_reading.map(|no_reading| no_reading.reason),
        Some(NoReadingReason::Unreachable)
    );
}

#[test]
fn a_reading_after_a_failure_clears_the_reason() {
    let asked_at = Instant::now();
    let mut readings = AccountReadings::default();
    readings.record(Err(NoReadingReason::SignedOut), asked_at);
    readings.record(Ok(reading("Plus")), asked_at + USAGE_CACHE_TTL);

    assert_eq!(
        readings
            .reading(UsageAgent::Codex, asked_at + USAGE_CACHE_TTL)
            .no_reading,
        None
    );
}

#[test]
fn opencode_is_told_to_refresh_codex_whose_sign_in_it_reads() {
    let agent = UsageAgent::from_id(ID_OPENCODE).expect("an adapter");
    assert_eq!(
        crate::agents::command_for_id(agent.sign_in_agent()),
        Some("codex")
    );
}

#[test]
fn the_reading_wire_shape_is_what_the_frontend_parses() {
    let wire = serde_json::to_value(AccountReading::no_source()).expect("serializes");
    assert_eq!(
        wire,
        serde_json::json!({
            "account": null,
            "noReading": { "reason": "no_source", "signInCommand": null }
        })
    );

    let reasons = [
        (NoReadingReason::SignedOut, "signed_out"),
        (NoReadingReason::SignInExpired, "sign_in_expired"),
        (NoReadingReason::Unreachable, "unreachable"),
    ];
    for (reason, wire) in reasons {
        assert_eq!(serde_json::to_value(reason).expect("serializes"), wire);
    }
}

#[test]
fn a_success_status_answers_the_body() {
    assert_eq!(
        http_answer("{\"plan_type\":\"plus\"}\n200"),
        Ok("{\"plan_type\":\"plus\"}".to_string())
    );
}

#[test]
fn an_unauthorized_or_forbidden_status_is_a_rejected_sign_in() {
    // The real Codex answer to an expired token, trimmed.
    let expired = "{\"error\":{\"code\":\"token_expired\"}}\n401";
    assert_eq!(http_answer(expired), Err(RequestFailure::Rejected));
    assert_eq!(http_answer("{}\n403"), Err(RequestFailure::Rejected));
}

#[test]
fn a_refusal_or_no_status_leaves_the_vendor_unreachable() {
    assert_eq!(http_answer("{}\n429"), Err(RequestFailure::Unreachable));
    assert_eq!(http_answer("\n503"), Err(RequestFailure::Unreachable));
    assert_eq!(http_answer("\n000"), Err(RequestFailure::Unreachable));
    assert_eq!(http_answer(""), Err(RequestFailure::Unreachable));
}

/// The `muse usage --json` shape (Muse Code 1.4.13), values sanitized.
fn muse_usage_response() -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "allowance": {
            "tier": "Plus",
            "window": { "usedPercent": 42, "resetsAtMs": 1_785_074_467_000_i64 },
            "weekly": { "usedPercent": 7, "resetsAtMs": 1_785_074_467_000_i64 },
            "asOfMs": 1_785_070_000_000_i64,
            "source": "mint"
        }
    })
}

#[test]
fn muse_usage_maps_the_five_hour_block_and_the_weekly_window() {
    let account = muse_account_usage(&muse_usage_response()).expect("an account");

    assert_eq!(account.plan, "Muse Plus");
    assert_eq!(account.account, Some(BillingAccount::Meta));
    assert_eq!(account.windows.len(), 2);
    assert_eq!(account.windows[0].kind, UsageWindowKind::Session);
    assert_eq!(account.windows[0].label, "5-hour");
    assert!((account.windows[0].utilization - 42.0).abs() < f64::EPSILON);
    assert_eq!(
        account.windows[0].resets_at.as_deref(),
        Some("2026-07-26T14:01:07Z")
    );
    assert_eq!(account.windows[1].kind, UsageWindowKind::Weekly);
}

#[test]
fn a_muse_window_of_another_length_passes_through_as_opaque() {
    let response = serde_json::json!({
        "allowance": {
            "window": { "usedPercent": 105, "windowDurationMins": 180 }
        }
    });

    let account = muse_account_usage(&response).expect("an account");

    assert_eq!(account.windows[0].kind, UsageWindowKind::Opaque);
    assert_eq!(account.windows[0].label, "3-hour");
    // The server's own figure, even past 100 - never trimmed here.
    assert!((account.windows[0].utilization - 105.0).abs() < f64::EPSILON);
    assert_eq!(account.plan, "Muse");
}

#[test]
fn muse_without_an_allowance_has_no_usage_source() {
    let response = serde_json::json!({ "schemaVersion": 1, "allowance": null });

    assert_eq!(
        muse_account_usage(&response).map(|account| account.plan),
        Err(NoReadingReason::NoSource)
    );
}

#[test]
fn what_muse_usage_prints_on_failure_says_why() {
    assert_eq!(
        muse_failure("muse: not logged in: run `muse login` first"),
        NoReadingReason::SignedOut
    );
    assert_eq!(
        muse_failure("Your saved login is no longer valid. Log in again"),
        NoReadingReason::SignInExpired
    );
    assert_eq!(
        muse_failure("No subscription allowance (payment_subscription_required)"),
        NoReadingReason::NoSource
    );
    assert_eq!(
        muse_failure("muse: usage refresh rate-limited; retry in 40s"),
        NoReadingReason::Unreachable
    );
}

#[test]
fn a_rejected_request_is_an_expired_sign_in() {
    assert_eq!(
        NoReadingReason::from(RequestFailure::Rejected),
        NoReadingReason::SignInExpired
    );
    assert_eq!(
        NoReadingReason::from(RequestFailure::Unreachable),
        NoReadingReason::Unreachable
    );
}

#[test]
fn an_agent_never_asked_must_be_asked() {
    let now = Instant::now();
    let request = AccountUsageRequest::cached(UsageAgent::Claude);
    assert!(AccountReadings::default().needs_asking(request, now, now));
}

#[test]
fn a_cached_request_reuses_a_fresh_ask() {
    let asked_at = Instant::now();
    let requested_at = asked_at + USAGE_CACHE_TTL.saturating_sub(ONE_SECOND);
    let request = AccountUsageRequest::cached(UsageAgent::Claude);
    let readings = readings_answered_at(asked_at);
    assert!(!readings.needs_asking(request, requested_at, requested_at));

    let expired_at = asked_at + USAGE_CACHE_TTL;
    assert!(readings.needs_asking(request, expired_at, expired_at));
}

#[test]
fn a_forced_request_asks_unless_someone_asked_after_it_was_made() {
    let asked_at = Instant::now();
    let request = AccountUsageRequest::refresh(UsageAgent::Claude);
    let readings = readings_answered_at(asked_at);
    let made_later = asked_at + ONE_SECOND;
    assert!(readings.needs_asking(request, made_later, made_later));

    // A burst of forced refreshes waits on the one ask in flight, then reuses it.
    let made_earlier = asked_at;
    assert!(!readings.needs_asking(request, made_earlier, asked_at + ONE_SECOND));
}

#[test]
fn a_failed_ask_keeps_the_last_reading_and_holds_off_a_retry() {
    let first_ask = Instant::now();
    let mut readings = readings_answered_at(first_ask);
    let failed_ask = first_ask + USAGE_CACHE_TTL;
    readings.record(Err(NoReadingReason::Unreachable), failed_ask);

    let shortly_after = failed_ask + ONE_SECOND;
    let request = AccountUsageRequest::cached(UsageAgent::Claude);
    assert!(!readings.needs_asking(request, shortly_after, shortly_after));
    assert_eq!(
        readings.last_known(shortly_after).map(|usage| usage.plan),
        Some("Max".to_string())
    );
}

#[test]
fn a_reading_past_its_limit_is_no_longer_shown() {
    let asked_at = Instant::now();
    let mut readings = readings_answered_at(asked_at);
    readings.record(
        Err(NoReadingReason::Unreachable),
        asked_at + LAST_READING_LIMIT,
    );
    assert!(readings.last_known(asked_at + LAST_READING_LIMIT).is_none());
}

#[test]
fn a_failure_before_any_reading_answers_nothing() {
    let asked_at = Instant::now();
    let mut readings = AccountReadings::default();
    readings.record(Err(NoReadingReason::Unreachable), asked_at);
    assert!(readings.last_known(asked_at).is_none());
}

#[test]
fn opencode_resolves_to_the_codex_adapter() {
    // One ChatGPT subscription, one source of truth: both agent ids read the
    // same adapter, so both surface the same account.
    assert!(matches!(
        UsageAgent::from_id(ID_OPENCODE),
        Some(UsageAgent::Codex)
    ));
    assert!(matches!(
        UsageAgent::from_id(ID_CODEX),
        Some(UsageAgent::Codex)
    ));
}

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
fn credits_turned_on_with_room_are_available() {
    // The real shape, probed live from a Max account with credits on.
    let response = serde_json::json!({
        "extra_usage": {
            "is_enabled": true,
            "monthly_limit": 20000,
            "used_credits": 11333,
            "utilization": 56.665,
            "currency": "USD",
            "decimal_places": 2,
            "disabled_reason": null,
            "user_disabled": false,
            "spend_limit_reached": false
        }
    });

    assert_eq!(credits_state(&response), Some(CreditsState::Available));
}

#[test]
fn uncapped_credits_are_available() {
    let response = serde_json::json!({
        "extra_usage": {
            "is_enabled": true,
            "monthly_limit": null,
            "used_credits": null,
            "utilization": null
        }
    });

    assert_eq!(credits_state(&response), Some(CreditsState::Available));
}

#[test]
fn spent_credits_are_unavailable() {
    let response = serde_json::json!({
        "extra_usage": { "is_enabled": true, "monthly_limit": 5000, "utilization": 100.0 }
    });

    assert_eq!(credits_state(&response), Some(CreditsState::Unavailable));
}

#[test]
fn credits_held_back_by_the_account_are_unavailable() {
    let reached = serde_json::json!({
        "extra_usage": { "is_enabled": true, "utilization": 40.0, "spend_limit_reached": true }
    });
    let disabled = serde_json::json!({
        "extra_usage": {
            "is_enabled": true,
            "utilization": 40.0,
            "disabled_reason": "org_level_disabled_until"
        }
    });

    assert_eq!(credits_state(&reached), Some(CreditsState::Unavailable));
    assert_eq!(credits_state(&disabled), Some(CreditsState::Unavailable));
}

#[test]
fn credits_turned_off_are_off_even_with_a_stale_cap_reading() {
    let response = serde_json::json!({
        "extra_usage": { "is_enabled": false, "monthly_limit": 5000, "utilization": 12.0 }
    });

    assert_eq!(credits_state(&response), Some(CreditsState::Off));
}

#[test]
fn a_prepaid_balance_reads_in_major_units() {
    // The real `prepaid/credits` shape, probed live (trimmed of its tranches).
    let response = serde_json::json!({
        "amount": 4689,
        "currency": "USD",
        "balance": { "money": null, "credits": { "amount_minor": 4689, "exponent": 2 } },
        "balance_credits": 46,
        "auto_reload_settings": { "enabled": true }
    });

    let balance = credit_balance(&response).expect("a balance");

    assert!((balance.amount - 46.89).abs() < 1e-9);
    assert_eq!(balance.currency, "USD");
}

#[test]
fn a_money_balance_and_its_own_exponent_are_honored() {
    let response = serde_json::json!({
        "currency": "JPY",
        "balance": { "money": { "amount_minor": 1200, "exponent": 0 }, "credits": null }
    });

    let balance = credit_balance(&response).expect("a balance");

    assert!((balance.amount - 1200.0).abs() < 1e-9);
}

#[test]
fn a_body_without_a_balance_has_none() {
    let error_body = serde_json::json!({ "error": { "message": "not found" } });
    let no_amount =
        serde_json::json!({ "currency": "USD", "balance": { "money": null, "credits": null } });

    assert_eq!(credit_balance(&error_body), None);
    assert_eq!(credit_balance(&no_amount), None);
}

#[test]
fn uncapped_credits_report_their_spend() {
    // The real shape, probed live from a Max account with no monthly cap.
    let response = serde_json::json!({
        "extra_usage": {
            "is_enabled": true,
            "monthly_limit": null,
            "used_credits": 25918.0,
            "utilization": null,
            "currency": "USD",
            "decimal_places": 2,
            "disabled_reason": null,
            "spend_limit_reached": false
        }
    });

    let spend = uncapped_credits_spend(&response).expect("a spend");
    assert!((spend.amount - 259.18).abs() < 1e-9);
    assert_eq!(spend.currency, "USD");
}

#[test]
fn capped_or_disabled_credits_report_no_spend() {
    let capped = serde_json::json!({
        "extra_usage": {
            "is_enabled": true,
            "monthly_limit": 20000,
            "used_credits": 11333,
            "currency": "USD",
            "decimal_places": 2
        }
    });
    let disabled = serde_json::json!({
        "extra_usage": {
            "is_enabled": false,
            "monthly_limit": null,
            "used_credits": 0,
            "currency": "USD",
            "decimal_places": 2
        }
    });

    assert_eq!(uncapped_credits_spend(&capped), None);
    assert_eq!(uncapped_credits_spend(&disabled), None);
}

#[test]
fn the_extra_usage_cap_is_the_credits_window() {
    let response = serde_json::json!({
        "extra_usage": { "is_enabled": true, "utilization": 56.665 },
        "five_hour": { "utilization": 100.0 }
    });

    let windows = collect_windows(&response);

    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].kind, UsageWindowKind::Session);
    assert_eq!(windows[1].kind, UsageWindowKind::Credits);
}

#[test]
fn a_response_without_credits_has_no_credits_state() {
    let without_key = serde_json::json!({ "five_hour": { "utilization": 40.0 } });
    let null_credits = serde_json::json!({ "extra_usage": null });

    assert_eq!(credits_state(&without_key), None);
    assert_eq!(credits_state(&null_credits), None);
}

#[test]
fn a_non_usage_body_yields_no_windows() {
    let error_body = serde_json::json!({ "error": { "message": "unauthorized" } });

    assert_eq!(collect_windows(&error_body), Vec::<UsageWindow>::new());
}

#[test]
fn collect_codex_windows_maps_a_weekly_primary_window() {
    // The real `/backend-api/wham/usage` shape captured from a ChatGPT account:
    // a single 7-day primary window, no secondary, no additional limits.
    let response = serde_json::json!({
        "plan_type": "plus",
        "rate_limit": {
            "primary_window": {
                "used_percent": 5,
                "limit_window_seconds": 604_800,
                "reset_after_seconds": 577_913,
                "reset_at": 1_785_074_467_i64
            },
            "secondary_window": serde_json::Value::Null
        },
        "additional_rate_limits": serde_json::Value::Null
    });

    let windows = collect_codex_windows(&response);

    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].kind, UsageWindowKind::Weekly);
    assert!((windows[0].utilization - 5.0).abs() < f64::EPSILON);
    assert_eq!(windows[0].label, "Weekly");
    // Epoch seconds normalized to the ISO string the frontend countdown parses.
    assert_eq!(
        windows[0].resets_at.as_deref(),
        Some("2026-07-26T14:01:07Z")
    );
}

#[test]
fn collect_codex_windows_classifies_session_before_weekly() {
    let response = serde_json::json!({
        "rate_limit": {
            "primary_window": { "used_percent": 20, "limit_window_seconds": 18_000 },
            "secondary_window": { "used_percent": 60, "limit_window_seconds": 604_800 }
        }
    });

    let windows = collect_codex_windows(&response);

    assert_eq!(windows.len(), 2);
    // Session (5-hour) sorts ahead of the weekly cap, matching the Claude order.
    assert_eq!(windows[0].kind, UsageWindowKind::Session);
    assert!((windows[0].utilization - 20.0).abs() < f64::EPSILON);
    assert_eq!(windows[1].kind, UsageWindowKind::Weekly);
    assert!((windows[1].utilization - 60.0).abs() < f64::EPSILON);
}

#[test]
fn collect_codex_windows_surfaces_additional_limits_opaquely() {
    let response = serde_json::json!({
        "rate_limit": {
            "primary_window": { "used_percent": 10, "limit_window_seconds": 604_800 }
        },
        "additional_rate_limits": [{
            "limit_name": "code_review",
            "rate_limit": {
                "primary_window": { "used_percent": 33, "limit_window_seconds": 18_000 }
            }
        }]
    });

    let windows = collect_codex_windows(&response);

    assert_eq!(windows.len(), 2);
    // The account weekly cap, then the named feature surfaced opaquely under
    // its own name - never mislabeled the account "Session".
    assert_eq!(windows[0].kind, UsageWindowKind::Weekly);
    assert_eq!(windows[1].kind, UsageWindowKind::Opaque);
    assert_eq!(windows[1].label, "Code review");
}

#[test]
fn a_codex_body_without_rate_limits_yields_no_windows() {
    let empty = serde_json::json!({ "plan_type": "plus", "rate_limit": serde_json::Value::Null });

    assert_eq!(collect_codex_windows(&empty), Vec::<UsageWindow>::new());
}

#[test]
fn unix_seconds_to_iso8601_matches_known_epochs() {
    assert_eq!(unix_seconds_to_iso8601(0), "1970-01-01T00:00:00Z");
    assert_eq!(
        unix_seconds_to_iso8601(1_704_067_200),
        "2024-01-01T00:00:00Z"
    );
    assert_eq!(
        unix_seconds_to_iso8601(1_785_074_467),
        "2026-07-26T14:01:07Z"
    );
}

#[test]
fn humanize_window_seconds_labels_common_durations() {
    assert_eq!(humanize_window_seconds(18_000), "5-hour");
    assert_eq!(humanize_window_seconds(604_800), "Weekly");
    assert_eq!(humanize_window_seconds(86_400), "1-day");
    assert_eq!(humanize_window_seconds(3_600), "1-hour");
}

#[test]
fn one_profile_read_answers_both_the_organization_and_the_version() {
    let profile = ClaudeProfile(serde_json::json!({
        "oauthAccount": { "organizationUuid": "ef60a6fb-ea18-46d3-a019-46012137d46e" },
        "lastReleaseNotesSeen": "2.1.227",
        "projects": {}
    }));

    assert_eq!(
        profile.organization(),
        Some("ef60a6fb-ea18-46d3-a019-46012137d46e")
    );
    assert_eq!(profile.cli_version(), Some("2.1.227"));
    assert_eq!(ClaudeProfile(serde_json::json!({})).organization(), None);
}

#[test]
fn the_wire_names_stay_what_the_frontend_reads() {
    let usage = serde_json::to_value(Usage {
        used_percentage: Some(12.5),
        label: "Codex".to_string(),
        resets_at: None,
        source: "local:.codex/auth.json".to_string(),
    })
    .expect("usage serializes");
    assert_eq!(usage["usedPct"], serde_json::json!(12.5));

    let accounts = [
        (BillingAccount::Anthropic, "anthropic"),
        (BillingAccount::Chatgpt, "chatgpt"),
        (BillingAccount::Github, "github"),
        (BillingAccount::Google, "google"),
        (BillingAccount::Cursor, "cursor"),
        (BillingAccount::Meta, "meta"),
    ];
    for (account, wire) in accounts {
        assert_eq!(serde_json::to_value(account).expect("serializes"), wire);
    }
}

#[test]
fn a_curl_config_carries_every_header_and_turns_a_body_into_a_quoted_post() {
    let config = curl_config(&CurlRequest {
        url: "https://example.com/reset",
        timeout_seconds: 20,
        headers: &[("Authorization", "Bearer abc"), ("User-Agent", "pade")],
        json_body: Some(r#"{"id":"a\b"}"#),
    });

    assert_eq!(
        config,
        "silent\nshow-error\nwrite-out = \"\\n%{http_code}\"\nmax-time = 20\n\
             url = \"https://example.com/reset\"\n\
             header = \"Authorization: Bearer abc\"\n\
             header = \"User-Agent: pade\"\n\
             request = \"POST\"\ndata = \"{\\\"id\\\":\\\"a\\\\b\\\"}\"\n"
    );
}

#[test]
fn a_curl_config_without_a_body_is_a_get() {
    let config = curl_config(&CurlRequest {
        url: "https://example.com/usage",
        timeout_seconds: 12,
        headers: &[],
        json_body: None,
    });

    assert!(!config.contains("request ="), "{config}");
    assert!(
        config.ends_with("url = \"https://example.com/usage\"\n"),
        "{config}"
    );
}

#[test]
fn a_chatgpt_account_id_is_a_safe_header_token() {
    assert!(is_safe_header_token("ef60a6fb-ea18-46d3-a019-46012137d46e"));
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

#[test]
fn collect_copilot_windows_maps_the_metered_premium_snapshot() {
    // The `copilot_internal/user` shape: a metered premium-request snapshot
    // plus unlimited chat/completions on a paid plan, and the shared reset.
    let response = serde_json::json!({
        "copilot_plan": "individual_pro",
        "quota_reset_date": "2026-08-01",
        "quota_snapshots": {
            "chat": { "unlimited": true, "percent_remaining": 100 },
            "completions": { "unlimited": true },
            "premium_interactions": {
                "unlimited": false,
                "entitlement": 300,
                "remaining": 210,
                "percent_remaining": 70
            }
        }
    });

    let windows = collect_copilot_windows(&response);

    // Only the metered snapshot surfaces - the unlimited buckets carry no
    // meaningful percentage and are skipped.
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].kind, UsageWindowKind::Opaque);
    assert_eq!(windows[0].key, "copilot_premium_interactions");
    assert_eq!(windows[0].label, "Premium interactions");
    // 70% remaining ⇒ 30% consumed.
    assert!((windows[0].utilization - 30.0).abs() < f64::EPSILON);
    assert_eq!(windows[0].resets_at.as_deref(), Some("2026-08-01"));
}

#[test]
fn collect_copilot_windows_derives_percent_from_counts_when_absent() {
    let response = serde_json::json!({
        "copilot_plan": "business",
        "quota_snapshots": {
            "premium_interactions": { "unlimited": false, "entitlement": 400, "remaining": 100 }
        }
    });

    let windows = collect_copilot_windows(&response);

    assert_eq!(windows.len(), 1);
    // 100 of 400 remaining ⇒ 75% consumed.
    assert!((windows[0].utilization - 75.0).abs() < f64::EPSILON);
    // No account reset date ⇒ no reset surfaced, never invented.
    assert_eq!(windows[0].resets_at, None);
}

#[test]
fn a_copilot_body_without_snapshots_yields_no_windows() {
    // A 401 error blob is not a usage response - no snapshots, no windows.
    let error_body = serde_json::json!({ "message": "Bad credentials" });

    assert_eq!(
        collect_copilot_windows(&error_body),
        Vec::<UsageWindow>::new()
    );
}

#[test]
fn usage_from_account_prefers_the_weekly_window() {
    let account = AccountUsage {
        plan: "Copilot business".to_string(),
        source: "oauth:api.github.com".to_string(),
        account: None,
        credits: None,
        credit_balance: None,
        uncapped_spend: None,
        windows: vec![
            UsageWindow {
                key: "a".to_string(),
                kind: UsageWindowKind::Opaque,
                label: "A".to_string(),
                utilization: 90.0,
                resets_at: None,
            },
            UsageWindow {
                key: "b".to_string(),
                kind: UsageWindowKind::Weekly,
                label: "B".to_string(),
                utilization: 40.0,
                resets_at: Some("2026-08-01".to_string()),
            },
        ],
    };

    let usage = usage_from_account(account, HeadlineWindow::WeeklyOrMostConsumed);

    // The weekly cap is the headline even when another window is hotter.
    assert_eq!(usage.used_percentage, Some(40.0));
    assert_eq!(usage.resets_at.as_deref(), Some("2026-08-01"));
    assert_eq!(usage.label, "Copilot business");
}

#[test]
fn usage_from_account_falls_back_to_the_most_consumed_window() {
    let account = AccountUsage {
        plan: "Copilot".to_string(),
        source: "oauth:api.github.com".to_string(),
        account: None,
        credits: None,
        credit_balance: None,
        uncapped_spend: None,
        windows: vec![
            UsageWindow {
                key: "a".to_string(),
                kind: UsageWindowKind::Opaque,
                label: "A".to_string(),
                utilization: 12.0,
                resets_at: None,
            },
            UsageWindow {
                key: "b".to_string(),
                kind: UsageWindowKind::Opaque,
                label: "B".to_string(),
                utilization: 88.0,
                resets_at: None,
            },
        ],
    };

    assert_eq!(
        usage_from_account(account, HeadlineWindow::WeeklyOrMostConsumed).used_percentage,
        Some(88.0)
    );
}

#[test]
fn weekly_only_ignores_a_non_weekly_window() {
    // Claude's strategy: with no weekly cap present, the meter shows no number
    // rather than falling back to the most-consumed window.
    let account = AccountUsage {
        plan: "Claude Pro".to_string(),
        source: "oauth:api.anthropic.com".to_string(),
        account: None,
        credits: None,
        credit_balance: None,
        uncapped_spend: None,
        windows: vec![UsageWindow {
            key: "five_hour".to_string(),
            kind: UsageWindowKind::Session,
            label: "Session".to_string(),
            utilization: 55.0,
            resets_at: Some("2026-08-01".to_string()),
        }],
    };

    let usage = usage_from_account(account, HeadlineWindow::WeeklyOnly);

    assert_eq!(usage.used_percentage, None);
    assert_eq!(usage.resets_at, None);
    assert_eq!(usage.label, "Claude Pro");
}

#[test]
fn usage_from_account_leaves_a_tier_label_account_without_numbers() {
    // The Antigravity / Cursor shape: a plan label, no windows - an honest
    // "we know your login, not your usage".
    let account = AccountUsage {
        plan: "Antigravity".to_string(),
        source: "local:.gemini/antigravity-cli/antigravity-oauth-token".to_string(),
        windows: Vec::new(),
        account: Some(BillingAccount::Google),
        credits: None,
        credit_balance: None,
        uncapped_spend: None,
    };

    let usage = usage_from_account(account, HeadlineWindow::WeeklyOrMostConsumed);

    assert_eq!(usage.used_percentage, None);
    assert_eq!(usage.resets_at, None);
    assert_eq!(usage.label, "Antigravity");
}
