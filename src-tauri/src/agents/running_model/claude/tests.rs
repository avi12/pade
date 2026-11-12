use super::catalog::model_option;
use super::switch::{hold_open, revert_switch_keys};
use super::{
    effort_for_model, merge_settings, newest_effort_set, newest_ultracode_entry, ULTRACODE_ENTER,
    ULTRACODE_EXIT,
};

#[cfg(windows)]
#[test]
fn a_held_permission_outlasts_a_hook_that_deletes_it() {
    let path = std::env::temp_dir().join(format!(
        "pade-switch-permission-{}.switch-permission",
        std::process::id()
    ));
    let held = hold_open(&path).expect("held permission");
    assert!(
        std::fs::remove_file(&path).is_err(),
        "an older sink's delete must fail"
    );
    assert!(path.exists());
    drop(held);
    std::fs::remove_file(&path).expect("deletable once released");
}

#[test]
fn the_newest_effort_the_transcript_logs_is_the_sessions_own() {
    let set = |level: &str, at: &str| {
        format!(
            r#"{{"type":"user","message":{{"role":"user","content":"<local-command-stdout>Set effort level to {level} (saved as your default for new sessions): Deeper</local-command-stdout>"}},"timestamp":"{at}"}}"#
        )
    };
    let cancelled = r#"{"type":"user","message":{"role":"user","content":"<local-command-stdout>Cancelled</local-command-stdout>"},"timestamp":"2026-10-15T02:28:32.508Z"}"#;
    let lines = [
        set("low", "2026-10-15T02:20:00.000Z"),
        set("xhigh", "2026-10-15T02:27:42.600Z"),
        cancelled.to_string(),
    ]
    .join("\n");

    let (level, set_at) = newest_effort_set(&lines).expect("a logged level");
    assert_eq!(level, "xhigh");
    assert_eq!(set_at, 1_792_031_262_600);
    assert_eq!(newest_effort_set(cancelled), None);
}

#[test]
fn the_newest_ultracode_entry_decides_whether_it_is_on() {
    let enter =
        r#"{"type":"attachment","attachment":{"type":"ultra_effort_enter","reminderType":"full"}}"#;
    let exit = r#"{"type":"attachment","attachment":{"type":"ultra_effort_exit"}}"#;
    let prompt = r#"{"type":"user","message":{"role":"user","content":"go"}}"#;
    assert_eq!(
        newest_ultracode_entry(&[enter, prompt].join("\n")),
        Some(ULTRACODE_ENTER)
    );
    assert_eq!(
        newest_ultracode_entry(&[enter, exit, prompt].join("\n")),
        Some(ULTRACODE_EXIT)
    );
    assert_eq!(newest_ultracode_entry(prompt), None);
}

#[test]
fn a_prompt_merely_mentioning_ultracode_is_not_an_entry() {
    let mention =
        r#"{"type":"user","message":{"role":"user","content":"what is ultra_effort_enter?"}}"#;
    assert_eq!(newest_ultracode_entry(mention), None);
}

#[test]
fn a_switch_is_reverted_key_by_key_and_other_edits_survive() {
    let before = serde_json::json!({
        "theme": "dark",
        "model": "claude-opus-5",
        "modelSettings": { "claude-opus-5": { "effortLevel": "high" } }
    });
    let mut after = serde_json::json!({
        "theme": "light",
        "model": "claude-sonnet-5-5",
        "modelSettings": {
            "claude-opus-5": { "effortLevel": "high" },
            "claude-sonnet-5-5": { "effortLevel": "low" }
        }
    });
    revert_switch_keys(&before, &mut after);
    assert_eq!(after["model"], "claude-opus-5");
    assert_eq!(after["modelSettings"], before["modelSettings"]);
    assert_eq!(after["theme"], "light");
}

#[test]
fn a_key_absent_before_the_switch_is_removed_again() {
    let before = serde_json::json!({ "theme": "dark" });
    let mut after = serde_json::json!({ "theme": "dark", "model": "sonnet" });
    revert_switch_keys(&before, &mut after);
    assert_eq!(after, before);
}

#[test]
fn reads_a_catalog_model_with_its_labelled_effort_ladder() {
    let model = serde_json::json!({
        "id": "claude-opus-5-5",
        "name": "Opus 5.5",
        "thinking": { "type": "effort", "effort_options": [
            { "id": "low", "name": "Low" },
            { "id": "medium", "name": "Medium", "badge": { "message": "Recommended" } },
            { "id": "xhigh", "name": "Extra" }
        ] }
    });
    let option = model_option(&model).expect("an option");
    let labels: Vec<_> = option
        .efforts
        .iter()
        .map(|effort| effort.name.as_str())
        .collect();
    assert_eq!(option.name, "Opus 5.5");
    assert_eq!(labels, ["Low", "Medium", "Extra"]);
    assert_eq!(option.recommended_effort.as_deref(), Some("medium"));
}

#[test]
fn a_models_own_level_outranks_the_global_one() {
    let settings = serde_json::json!({
        "effortLevel": "xhigh",
        "modelSettings": { "claude-opus-5": { "effortLevel": "high" } }
    });
    assert_eq!(
        effort_for_model(&settings, None, "claude-opus-5").as_deref(),
        Some("high")
    );
    assert_eq!(
        effort_for_model(&settings, None, "claude-fable-5-1").as_deref(),
        Some("xhigh")
    );
}

#[test]
fn a_catalog_model_without_its_own_level_runs_at_the_catalog_default() {
    let settings = serde_json::json!({
        "effortLevel": "xhigh",
        "modelSettings": { "claude-opus-5-5": { "effortLevel": "high" } }
    });
    let catalog = serde_json::json!({ "catalog": { "config": { "models": [
        { "id": "claude-opus-5-5", "runtime": { "default_effort": "medium" } },
        { "id": "claude-sonnet-5-5", "runtime": { "default_effort": "medium" } }
    ] } } });
    let effort = |model: &str| effort_for_model(&settings, Some(&catalog), model);
    assert_eq!(effort("claude-sonnet-5-5").as_deref(), Some("medium"));
    assert_eq!(effort("claude-sonnet-5-5[1m]").as_deref(), Some("medium"));
    assert_eq!(effort("claude-opus-5-5").as_deref(), Some("high"));
    assert_eq!(effort("claude-opus-4-8").as_deref(), Some("xhigh"));
}

#[test]
fn a_context_variant_shares_its_models_level() {
    let settings = serde_json::json!({
        "effortLevel": "xhigh",
        "modelSettings": { "claude-opus-5-5": { "effortLevel": "medium" } }
    });
    assert_eq!(
        effort_for_model(&settings, None, "claude-opus-5-5[1m]").as_deref(),
        Some("medium")
    );
}

#[test]
fn hooks_join_the_existing_settings_layer() {
    let mut hooks = serde_json::Map::new();
    hooks.insert("hooks".to_string(), serde_json::json!({}));
    let merged = merge_settings(r#"{"theme":"custom:pade"}"#, hooks);
    let value: serde_json::Value = serde_json::from_str(&merged).expect("valid JSON");
    assert_eq!(value["theme"], "custom:pade");
    assert!(value.get("hooks").is_some());
}
