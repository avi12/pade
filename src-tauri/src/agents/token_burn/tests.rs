use super::*;
use std::io::Cursor;

const NOW: &str = "2026-10-30T12:00:00.000Z";
const EARLIER: &str = "2026-10-30T11:50:00.000Z";
const LONG_AGO: &str = "2026-10-30T10:00:00.000Z";
const WINDOW_START: &str = "2026-10-30T11:30:00.000Z";

fn window_start() -> u64 {
    timestamp_milliseconds(WINDOW_START).expect("a valid timestamp")
}

fn reply(timestamp: &str, model: &str, message_id: &str, request_id: &str) -> String {
    format!(
        r#"{{"type":"assistant","timestamp":"{timestamp}","requestId":"{request_id}","message":{{"id":"{message_id}","model":"{model}","usage":{{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":300,"cache_creation_input_tokens":40,"cache_creation":{{"ephemeral_5m_input_tokens":30,"ephemeral_1h_input_tokens":10}}}}}}}}"#
    )
}

fn user(timestamp: &str) -> String {
    format!(
        r#"{{"type":"user","timestamp":"{timestamp}","message":{{"role":"user","content":"next"}}}}"#
    )
}

fn totals(lines: &[String]) -> Vec<ModelTokenUsage> {
    Tally::default().count(replies_since(
        Cursor::new(lines.join("\n")),
        window_start(),
        READ_LIMITS,
    ))
}

fn usage(model: &str, replies: u64) -> ModelTokenUsage {
    ModelTokenUsage {
        model: model.to_string(),
        input_tokens: 10 * replies,
        output_tokens: 20 * replies,
        cache_read_tokens: 300 * replies,
        cache_write_five_minute_tokens: 30 * replies,
        cache_write_one_hour_tokens: 10 * replies,
    }
}

#[test]
fn the_lines_of_one_reply_count_once() {
    let lines = [
        reply(EARLIER, "claude-opus-5-5", "message-1", "request-1"),
        reply(EARLIER, "claude-opus-5-5", "message-1", "request-1"),
        reply(EARLIER, "claude-opus-5-5", "message-1", "request-1"),
        reply(NOW, "claude-opus-5-5", "message-2", "request-2"),
    ];
    assert_eq!(totals(&lines), vec![usage("claude-opus-5-5", 2)]);
}

#[test]
fn a_reply_counts_the_final_output_its_last_line_holds() {
    let streaming = reply(EARLIER, "claude-opus-5-5", "message-1", "request-1")
        .replace(r#""output_tokens":20"#, r#""output_tokens":3"#);
    let lines = [
        streaming,
        reply(EARLIER, "claude-opus-5-5", "message-1", "request-1"),
    ];
    assert_eq!(totals(&lines), vec![usage("claude-opus-5-5", 1)]);
}

#[test]
fn a_reply_without_both_ids_counts_as_is() {
    let lines = [
        reply(NOW, "claude-sonnet-5-5", "", "request-1").replace(r#""id":"","#, ""),
        reply(NOW, "claude-sonnet-5-5", "", "request-1").replace(r#""id":"","#, ""),
    ];
    assert_eq!(totals(&lines), vec![usage("claude-sonnet-5-5", 2)]);
}

#[test]
fn entries_before_the_window_are_left_out_and_end_the_read() {
    let lines = [
        reply(LONG_AGO, "claude-haiku-5-5", "message-0", "request-0"),
        user(LONG_AGO),
        reply(EARLIER, "claude-opus-5-5", "message-1", "request-1"),
        user(NOW),
    ];
    assert_eq!(totals(&lines), vec![usage("claude-opus-5-5", 1)]);
}

#[test]
fn malformed_partial_and_synthetic_lines_are_skipped() {
    let partial = reply(NOW, "claude-opus-5-5", "message-9", "request-9");
    let lines = [
        reply(EARLIER, "claude-opus-5-5", "message-1", "request-1"),
        "not json at all".to_string(),
        reply(EARLIER, SYNTHETIC_MODEL, "message-2", "request-2"),
        partial[..partial.len() / 2].to_string(),
    ];
    assert_eq!(totals(&lines), vec![usage("claude-opus-5-5", 1)]);
}

#[test]
fn a_cache_write_with_no_lifetime_split_lives_five_minutes() {
    let line = reply(NOW, "claude-opus-5-5", "message-1", "request-1").replace(
        r#","cache_creation":{"ephemeral_5m_input_tokens":30,"ephemeral_1h_input_tokens":10}"#,
        "",
    );
    let found = totals(&[line]);
    assert_eq!(found[0].cache_write_five_minute_tokens, 40);
    assert_eq!(found[0].cache_write_one_hour_tokens, 0);
}

#[test]
fn lines_read_backwards_across_tiny_chunks_come_out_whole_newest_first() {
    let text = "first line\r\nsecond \u{05e9}\u{05dc}\u{05d5}\u{05dd}\n\nthird";
    let limits = ReadLimits {
        chunk_bytes: 3,
        maximum_line_bytes: 1024,
    };
    let lines: Vec<String> = ReverseLines::new(Cursor::new(text), limits)
        .expect("a seekable reader")
        .collect();
    assert_eq!(
        lines,
        [
            "third",
            "second \u{05e9}\u{05dc}\u{05d5}\u{05dd}",
            "first line"
        ]
    );
}

#[test]
fn a_line_longer_than_the_limit_is_dropped_and_its_neighbours_kept() {
    let text = format!("before\n{}\nafter", "x".repeat(50));
    let limits = ReadLimits {
        chunk_bytes: 4,
        maximum_line_bytes: 10,
    };
    let lines: Vec<String> = ReverseLines::new(Cursor::new(text), limits)
        .expect("a seekable reader")
        .collect();
    assert_eq!(lines, ["after", "before"]);
}

#[test]
fn a_session_counts_each_subagents_transcript_apart_from_its_own() {
    let folder = std::env::temp_dir().join(format!("pade-token-burn-{}", std::process::id()));
    let subagents = folder.join("conversation").join(SUBAGENTS_DIRECTORY);
    std::fs::create_dir_all(&subagents).expect("a scratch folder");
    let log = folder.join("conversation.jsonl");
    std::fs::write(
        &log,
        reply(NOW, "claude-opus-5-5", "message-1", "request-1"),
    )
    .expect("the session's transcript");
    std::fs::write(
        subagents.join("agent-a.jsonl"),
        [
            reply(EARLIER, "claude-haiku-5-5", "message-2", "request-2"),
            reply(NOW, "claude-haiku-5-5", "message-3", "request-3"),
        ]
        .join("\n"),
    )
    .expect("a subagent's transcript");
    std::fs::write(
        subagents.join("agent-a.meta.json"),
        r#"{"agentType":"general-purpose","description":"Find the bug"}"#,
    )
    .expect("its metadata");
    // A repeat of the session's own reply counts once, where it came first.
    std::fs::write(
        subagents.join("agent-b.jsonl"),
        reply(NOW, "claude-opus-5-5", "message-1", "request-1"),
    )
    .expect("a repeating transcript");

    // The scratch files are written now, not on the fixtures' date, so the
    // window opens at the epoch to keep the write-time filter out of it.
    let found = session_usage(&log, 0);
    let untouched_since_the_future = session_usage(&log, u64::MAX);
    std::fs::remove_dir_all(&folder).expect("cleanup");
    assert_eq!(untouched_since_the_future, SessionTokenUsage::default());
    assert_eq!(found.own, vec![usage("claude-opus-5-5", 1)]);
    assert_eq!(found.subagents.len(), 1);
    let subagent = &found.subagents[0];
    assert_eq!(subagent.id, "a");
    assert_eq!(subagent.description.as_deref(), Some("Find the bug"));
    assert_eq!(subagent.usage, vec![usage("claude-haiku-5-5", 2)]);
}

#[test]
fn an_agent_without_a_readable_log_measures_nothing() {
    assert_eq!(
        session_usage(&PathBuf::from("no/such/transcript.jsonl"), 0),
        SessionTokenUsage::default()
    );
}
