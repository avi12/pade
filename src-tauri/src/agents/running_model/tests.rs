use super::state::fold;
use super::{
    days_since_epoch, launch_arguments, newer, timestamp_milliseconds, ModelChoice, Saved,
    SessionState,
};

fn choice(model: Option<&str>, effort: Option<&str>) -> ModelChoice {
    ModelChoice {
        model: model.map(str::to_string),
        effort: effort.map(str::to_string),
    }
}

fn report(json: &str) -> serde_json::Value {
    serde_json::from_str(json).expect("a valid report")
}

#[test]
fn each_agent_gets_its_own_launch_flags_for_a_choice() {
    let picked = choice(Some("claude-sonnet-5-5"), Some("low"));
    assert_eq!(
        launch_arguments("claude", &picked),
        ["--model", "claude-sonnet-5-5", "--effort", "low"]
    );
    assert_eq!(
        launch_arguments("codex", &choice(Some("gpt-5.6-terra"), Some("high"))),
        [
            "-c",
            "model=\"gpt-5.6-terra\"",
            "-c",
            "model_reasoning_effort=\"high\""
        ]
    );
    assert_eq!(
        launch_arguments(
            "opencode",
            &choice(Some("openai/gpt-5.6-sol"), Some("high"))
        ),
        ["-m", "openai/gpt-5.6-sol"]
    );
    assert_eq!(launch_arguments("aider", &picked), Vec::<String>::new());
}

#[test]
fn only_plain_tokens_make_a_well_formed_choice() {
    assert!(choice(Some("openai/gpt-5.6-sol"), Some("xhigh")).is_well_formed());
    assert!(choice(Some("opus[1m]"), None).is_well_formed());
    assert!(!choice(Some("opus; rm -rf"), None).is_well_formed());
    assert!(!choice(None, Some("")).is_well_formed());
}

#[test]
fn a_report_records_the_agents_own_session_id() {
    let state = fold(
        SessionState::default(),
        &serde_json::json!({ "session_id": "01a12d15", "model": "gpt-5.6-terra" }),
        10,
    );
    assert_eq!(state.agent_session.as_deref(), Some("01a12d15"));
}

#[test]
fn a_start_report_names_the_model_and_transcript() {
    let state = fold(
        SessionState::default(),
        &report(
            r#"{"hook_event_name":"SessionStart","model":"gpt-5.6-terra","transcript_path":"C:\\x.jsonl"}"#,
        ),
        10,
    );
    assert_eq!(state.model.as_deref(), Some("gpt-5.6-terra"));
    assert_eq!(state.model_at, 10);
    assert_eq!(state.transcript.as_deref(), Some("C:\\x.jsonl"));
}

#[test]
fn a_model_switch_reports_the_new_model() {
    let state = fold(
        SessionState::default(),
        &report(r#"{"from_model":"claude-opus-5","to_model":"claude-sonnet-5-5"}"#),
        20,
    );
    assert_eq!(state.model.as_deref(), Some("claude-sonnet-5-5"));
}

#[test]
fn re_reporting_the_same_model_keeps_its_original_time() {
    let launched = SessionState {
        model: Some("claude-opus-5-5".to_string()),
        model_at: 10,
        ..SessionState::default()
    };
    let state = fold(launched, &report(r#"{"model":"claude-opus-5-5"}"#), 50);
    assert_eq!(state.model_at, 10);
}

#[test]
fn a_turn_report_sets_the_effort_and_keeps_the_model() {
    let started = SessionState {
        model: Some("claude-opus-5".to_string()),
        model_at: 10,
        ..SessionState::default()
    };
    let state = fold(started, &report(r#"{"effort":{"level":"low"}}"#), 30);
    assert_eq!(state.model.as_deref(), Some("claude-opus-5"));
    assert_eq!(state.effort.as_deref(), Some("low"));
    assert_eq!(state.effort_at, 30);
}

#[test]
fn with_nothing_reported_the_saved_value_speaks() {
    let saved = Saved {
        value: "high".to_string(),
        saved_at: 5,
    };
    assert_eq!(newer(None, Some(&saved), 0).as_deref(), Some("high"));
}

#[test]
fn a_save_after_the_last_report_wins() {
    let saved = Saved {
        value: "low".to_string(),
        saved_at: 25,
    };
    assert_eq!(
        newer(Some(("high", 20)), Some(&saved), 0).as_deref(),
        Some("low")
    );
}

#[test]
fn a_report_after_the_save_wins() {
    let saved = Saved {
        value: "low".to_string(),
        saved_at: 25,
    };
    assert_eq!(
        newer(Some(("medium", 30)), Some(&saved), 0).as_deref(),
        Some("medium")
    );
}

#[test]
fn a_model_switch_dates_the_saved_level_to_the_switch() {
    let saved = Saved {
        value: "low".to_string(),
        saved_at: 5,
    };
    assert_eq!(
        newer(Some(("high", 30)), Some(&saved), 40).as_deref(),
        Some("low")
    );
}

#[test]
fn reads_an_agent_timestamp() {
    assert_eq!(
        timestamp_milliseconds("1970-01-01T00:00:01.500Z"),
        Some(1_500)
    );
    assert_eq!(
        timestamp_milliseconds("2026-10-11T22:28:21.681Z"),
        Some(1_791_757_701_681)
    );
    assert_eq!(
        timestamp_milliseconds("2000-03-01T00:00:00Z"),
        Some(951_868_800_000)
    );
    assert_eq!(timestamp_milliseconds("not a time"), None);
}

#[test]
fn counts_days_from_the_epoch_across_leap_days() {
    assert_eq!(days_since_epoch(1970, 1, 1), 0);
    assert_eq!(days_since_epoch(2000, 2, 29), 11_016);
    assert_eq!(days_since_epoch(2000, 3, 1), 11_017);
}
