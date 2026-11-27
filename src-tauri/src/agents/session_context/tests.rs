use crate::agents::session_context::{
    addresses_in, is_conversation_entry, model_field, newest_goal_in, newest_occupied_in,
    newest_turn_in, CatalogProvider, TurnProfile,
};
use std::collections::HashMap;

const LARGE_CACHE_READ_TOKENS: u64 = 472_681;
const LARGE_CACHE_WRITE_TOKENS: u64 = 840;
const LARGE_OUTPUT_TOKENS: u64 = 447;
const LATER_INPUT_TOKENS: u64 = 5;
const SMALL_CACHE_READ_TOKENS: u64 = 100_000;
const SMALL_CACHE_WRITE_TOKENS: u64 = 500;
const SMALL_OUTPUT_TOKENS: u64 = 900;
const MEDIUM_CACHE_READ_TOKENS: u64 = 300_000;
const MEDIUM_CACHE_WRITE_TOKENS: u64 = 700;
const MEDIUM_OUTPUT_TOKENS: u64 = 300;
const COMPLETE_CACHE_READ_TOKENS: u64 = 2_000;
const COMPLETE_CACHE_WRITE_TOKENS: u64 = 10;
const COMPLETE_OUTPUT_TOKENS: u64 = 30;

fn assistant_line(input: u64, cache_read: u64, cache_write: u64, output: u64) -> String {
    format!(
        r#"{{"type":"assistant","message":{{"model":"claude-opus-5","usage":{{"input_tokens":{input},"cache_read_input_tokens":{cache_read},"cache_creation_input_tokens":{cache_write},"output_tokens":{output}}}}}}}"#
    )
}

#[test]
fn reads_the_model_from_a_session_line() {
    let line = r#"{"type":"assistant","message":{"model":"claude-opus-4-8","role":"assistant"}}"#;
    assert_eq!(model_field(line).as_deref(), Some("claude-opus-4-8"));
}

#[test]
fn a_line_without_a_model_field_yields_none() {
    assert_eq!(model_field(r#"{"type":"summary","summary":"hi"}"#), None);
}

#[test]
fn occupied_tokens_count_the_cached_prefix_not_just_what_streamed() {
    let lines = assistant_line(
        2,
        LARGE_CACHE_READ_TOKENS,
        LARGE_CACHE_WRITE_TOKENS,
        LARGE_OUTPUT_TOKENS,
    );
    assert_eq!(newest_occupied_in(&lines), Some(473_970));
}

#[test]
fn the_newest_turn_wins_over_the_earlier_ones() {
    let lines = [
        assistant_line(
            LATER_INPUT_TOKENS,
            SMALL_CACHE_READ_TOKENS,
            SMALL_CACHE_WRITE_TOKENS,
            SMALL_OUTPUT_TOKENS,
        ),
        assistant_line(
            2,
            MEDIUM_CACHE_READ_TOKENS,
            MEDIUM_CACHE_WRITE_TOKENS,
            MEDIUM_OUTPUT_TOKENS,
        ),
        r#"{"type":"user","message":{"role":"user","content":"next"}}"#.to_string(),
    ]
    .join("\n");
    assert_eq!(newest_occupied_in(&lines), Some(301_002));
}

#[test]
fn a_tail_starting_mid_line_skips_the_broken_first_line() {
    let complete = assistant_line(
        1,
        COMPLETE_CACHE_READ_TOKENS,
        COMPLETE_CACHE_WRITE_TOKENS,
        COMPLETE_OUTPUT_TOKENS,
    );
    let lines = format!("okens\":9999999}}}}}}\n{complete}");
    assert_eq!(newest_occupied_in(&lines), Some(2_041));
}

#[test]
fn a_log_with_no_usage_yet_yields_none() {
    let lines = r#"{"type":"user","message":{"role":"user","content":"hello"}}"#;
    assert_eq!(newest_occupied_in(lines), None);
}

#[test]
fn the_newest_turn_names_the_model_and_effort_it_ran_on() {
    let lines = [
        r#"{"type":"assistant","effort":"high","message":{"model":"claude-opus-5-5"}}"#,
        r#"{"type":"assistant","effort":"medium","message":{"model":"claude-fable-5-1"}}"#,
        r#"{"type":"user","message":{"role":"user","content":"next"}}"#,
    ]
    .join("\n");
    assert_eq!(
        newest_turn_in(&lines),
        Some(TurnProfile {
            model: "claude-fable-5-1".to_string(),
            effort: Some("medium".to_string()),
        })
    );
}

#[test]
fn a_synthetic_turn_never_reads_as_the_running_model() {
    let lines = [
        r#"{"type":"assistant","effort":"high","message":{"model":"claude-opus-5-5"}}"#,
        r#"{"type":"assistant","message":{"model":"<synthetic>"}}"#,
    ]
    .join("\n");
    assert_eq!(
        newest_turn_in(&lines).map(|turn| turn.model).as_deref(),
        Some("claude-opus-5-5")
    );
}

#[test]
fn a_turn_without_an_effort_still_names_its_model() {
    let line = r#"{"type":"assistant","message":{"model":"claude-haiku-4-5-20251001"}}"#;
    assert_eq!(newest_turn_in(line).and_then(|turn| turn.effort), None);
}

#[test]
fn parses_the_models_dev_shape_down_to_the_context_window() {
    let catalog = r#"{"anthropic":{"models":{"claude-opus-4-8":{"limit":{"context":1000000,"output":128000}}}}}"#;
    let context = serde_json::from_str::<HashMap<String, CatalogProvider>>(catalog)
        .ok()
        .and_then(|providers| {
            providers
                .get("anthropic")?
                .models
                .get("claude-opus-4-8")?
                .limit
                .as_ref()?
                .context
        });
    assert_eq!(context, Some(1_000_000));
}

#[test]
fn finds_every_address_in_log_text_without_its_wrapping() {
    let text = r#"see [PR](https://github.com/a/b/pull/7), then \"https://ci.example.com/run/9\". Done: https://x.dev."#;
    assert_eq!(
        addresses_in(text),
        [
            "https://github.com/a/b/pull/7",
            "https://ci.example.com/run/9",
            "https://x.dev"
        ]
    );
}

#[test]
fn only_the_conversations_own_entries_count() {
    assert!(is_conversation_entry(
        r#"{"type":"pr-link","prUrl":"https://github.com/a/b/pull/7"}"#
    ));
    assert!(is_conversation_entry(
        r#"{"type":"assistant","message":{"content":"https://x.dev"}}"#
    ));
    assert!(!is_conversation_entry(
        r#"{"type":"attachment","tools":"https://docs.example.com"}"#
    ));
    assert!(!is_conversation_entry(
        r#"{"type":"user","message":"no link"}"#
    ));
}

/// A user log entry recording a typed `/goal` with `arguments`, in the shape
/// Claude writes it.
fn goal_entry(arguments: &str) -> String {
    let content = format!(
        "<command-name>/goal</command-name>\n<command-message>goal</command-message>\n<command-args>{arguments}</command-args>"
    );
    serde_json::json!({"type": "user", "message": {"role": "user", "content": content}}).to_string()
}

#[test]
fn the_newest_goal_condition_is_read_from_the_users_own_entry() {
    let log = [
        goal_entry("ship the old thing"),
        goal_entry("Run the tests and verify everything works"),
    ]
    .join("\n");
    assert_eq!(
        newest_goal_in(&log).as_deref(),
        Some("Run the tests and verify everything works")
    );
}

#[test]
fn a_cleared_goal_leaves_none_to_carry_over() {
    let log = [goal_entry("ship it"), goal_entry("clear")].join("\n");
    assert_eq!(newest_goal_in(&log), None);
}

#[test]
fn a_tool_result_quoting_a_goal_command_never_counts() {
    let quoted = serde_json::json!({
        "type": "assistant",
        "message": {"content": "<command-name>/goal</command-name><command-args>fake</command-args>"}
    })
    .to_string();
    let log = [goal_entry("the real goal"), quoted].join("\n");
    assert_eq!(newest_goal_in(&log).as_deref(), Some("the real goal"));
}
