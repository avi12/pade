
use super::{is_grant_id, offered_resets, LimitReset, LimitResetOutcome, ResetKind, ResetStatus};

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
            kind: ResetKind::Full,
            has_usage_to_clear: true,
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
fn a_weekly_reset_and_a_fable_reset_are_offered_apart_as_claude_names_them() {
    // The real shape, probed live 2026-11-11: a monthly weekly reset that
    // also clears Fable's week, beside Fable resets the server types "full"
    // but which give the week back.
    let offers = offered_resets(status(&serde_json::json!({
        "eligible": true,
        "grants": [
            {
                "id": "full-reset-2611",
                "label": "One full limit reset (November 2026)",
                "resets_left": 1,
                "ends_at": "2026-12-10T22:00:00+00:00",
                "clears": ["seven_day", "seven_day_overage_included"],
                "usable_now": true,
                "percent_used": { "seven_day": 63, "seven_day_overage_included": 0 },
                "type": "weekly",
                "gives_back_weekly": false
            },
            {
                "id": "launch-2610-fable",
                "label": "New model launch: one Fable reset",
                "resets_left": 1,
                "ends_at": "2027-03-09T07:59:00+00:00",
                "clears": ["seven_day_overage_included"],
                "usable_now": true,
                "percent_used": { "seven_day_overage_included": 0 },
                "type": "full",
                "gives_back_weekly": true
            }
        ],
        "next_grant_id": "full-reset-2611"
    })));

    let summary: Vec<(ResetKind, bool)> = offers
        .iter()
        .map(|offer| (offer.kind, offer.has_usage_to_clear))
        .collect();
    assert_eq!(
        summary,
        vec![(ResetKind::Weekly, true), (ResetKind::Fable, false)]
    );
}

#[test]
fn an_unknown_type_falls_back_to_the_windows_it_clears() {
    let offers = offered_resets(status(&serde_json::json!({
        "eligible": true,
        "grants": [{
            "id": "session",
            "label": "session",
            "resets_left": 1,
            "clears": ["five_hour"],
            "usable_now": true,
            "type": "something_new"
        }],
        "next_grant_id": "session"
    })));

    assert_eq!(offers[0].kind, ResetKind::Session);
    assert!(offers[0].has_usage_to_clear);
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
