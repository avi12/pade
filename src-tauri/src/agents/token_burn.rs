//! How many tokens a Claude session spent lately, read off its own transcripts -
//! the measured load usage pacing shares an account's slowdown by, instead of
//! guessing it from the model and the time the session spent working.
//!
//! Claude Code logs every reply's exact `usage` in the session's transcript, and
//! each subagent's replies in a transcript of its own beside it
//! (`<conversation>/subagents/agent-<id>.jsonl`), so the session's whole spend is
//! the sum over both. Two things keep that sum honest and cheap:
//!
//!   * Claude writes one line per content block of a reply, each repeating the
//!     reply's usage, so a reply counts once by its message id and request id
//!     together (as `ccusage` does). The repeats are not identical, though: the
//!     input and cache counts are, but `output_tokens` grows block by block and
//!     only the reply's last line holds its final count (measured on this
//!     machine: two in three replies). The backwards read meets that last line
//!     first, so counting the first line seen keeps the final count - where a
//!     forward read's first line would keep an early, far smaller one.
//!   * The window is recent and a transcript grows all session long, so each
//!     file is read backwards from its end, a chunk at a time, and the read stops
//!     at the first entry older than the window - the cost is the window, never
//!     the whole file. A subagent transcript untouched since the window began is
//!     not opened at all.
//!
//! Only Claude keeps such a log (see `session_context`); any other agent
//! measures nothing, which the caller reads as "estimate instead".

use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::conversations::TRANSCRIPT_EXTENSION;
use super::running_model::{modified_milliseconds, timestamp_milliseconds};
use super::session_context::{claude_session_log, LogLine, TokenUsage, SYNTHETIC_MODEL};

/// The folder beside a transcript (named by its conversation) that holds its
/// subagents' transcripts.
const SUBAGENTS_DIRECTORY: &str = "subagents";
const NEWLINE: u8 = b'\n';
const CARRIAGE_RETURN: u8 = b'\r';

/// How a transcript is read backwards: a chunk at a time, and never holding
/// more than one line's worth of bytes beyond that.
#[derive(Debug, Clone, Copy)]
struct ReadLimits {
    chunk_bytes: u64,
    /// A line longer than this (a huge pasted image, a giant tool result) is
    /// dropped unread - it carries no reply's usage worth the memory.
    maximum_line_bytes: usize,
}

const READ_LIMITS: ReadLimits = ReadLimits {
    chunk_bytes: 64 * 1024,
    maximum_line_bytes: 16 * 1024 * 1024,
};

/// One model's share of a session's recent spend, by token type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTokenUsage {
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_five_minute_tokens: u64,
    cache_write_one_hour_tokens: u64,
}

impl ModelTokenUsage {
    fn named(model: &str) -> Self {
        Self {
            model: model.to_string(),
            ..Self::default()
        }
    }

    fn add(&mut self, usage: &TokenUsage) {
        let (five_minute, one_hour) = usage.cache_writes();
        self.input_tokens += usage.input.unwrap_or_default();
        self.output_tokens += usage.output.unwrap_or_default();
        self.cache_read_tokens += usage.cache_read.unwrap_or_default();
        self.cache_write_five_minute_tokens += five_minute;
        self.cache_write_one_hour_tokens += one_hour;
    }
}

/// The tokens `conversation_id`'s session - and every subagent it ran - spent
/// since `since` (milliseconds since the epoch), one entry per model. Empty for
/// an agent that keeps no readable log, or a session with no reply yet. Off the
/// UI thread: it reads files.
#[tauri::command]
pub async fn session_token_usage(
    command: String,
    conversation_id: Option<String>,
    since: u64,
) -> Result<Vec<ModelTokenUsage>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        claude_session_log(&command, conversation_id.as_deref())
            .map(|log| usage_since(&transcripts_of(&log, since), since))
            .unwrap_or_default()
    })
    .await
    .map_err(|error| error.to_string())
}

/// A session's transcripts written to since `since`: its own, and its
/// subagents' in the folder named after it.
fn transcripts_of(log: &Path, since: u64) -> Vec<PathBuf> {
    let mut transcripts = vec![log.to_path_buf()];
    let subagents = log.with_extension("").join(SUBAGENTS_DIRECTORY);
    // A session that never ran a subagent has no folder for them.
    if let Ok(entries) = std::fs::read_dir(subagents) {
        transcripts.extend(entries.flatten().map(|entry| entry.path()).filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == TRANSCRIPT_EXTENSION)
        }));
    }
    transcripts.retain(|transcript| is_written_since(transcript, since));
    transcripts
}

/// Whether a transcript may hold an entry from `since` on. One whose write time
/// can't be read is read anyway, so a measurement is never lost to it.
fn is_written_since(transcript: &Path, since: u64) -> bool {
    modified_milliseconds(transcript).is_none_or(|modified| modified >= since)
}

/// Every reply the transcripts logged since `since`, each counted once, summed
/// per model.
fn usage_since(transcripts: &[PathBuf], since: u64) -> Vec<ModelTokenUsage> {
    let mut tally = Tally::default();
    for transcript in transcripts {
        // A transcript gone between listing and opening holds nothing to count.
        let Ok(file) = File::open(transcript) else {
            continue;
        };
        tally.count(replies_since(file, since, READ_LIMITS));
    }
    tally.into_usage()
}

/// Replies summed per model, each reply once however many lines repeat it -
/// across transcripts too. Fed newest first, so the line it keeps is a reply's
/// last, the one with its final output count.
#[derive(Debug, Default)]
struct Tally {
    counted: HashSet<(String, String)>,
    by_model: BTreeMap<String, ModelTokenUsage>,
}

impl Tally {
    fn count(&mut self, replies: Vec<Reply>) {
        for reply in replies {
            let is_repeat = reply
                .identity
                .is_some_and(|identity| !self.counted.insert(identity));
            if is_repeat {
                continue;
            }
            self.by_model
                .entry(reply.model)
                .or_insert_with_key(|model| ModelTokenUsage::named(model))
                .add(&reply.usage);
        }
    }

    fn into_usage(self) -> Vec<ModelTokenUsage> {
        self.by_model.into_values().collect()
    }
}

/// One reply's usage, as one transcript line records it.
#[derive(Debug)]
struct Reply {
    /// Its message id and request id - every line of the same reply repeats
    /// both. `None` when either is missing, and then the line counts as is.
    identity: Option<(String, String)>,
    model: String,
    usage: TokenUsage,
}

impl Reply {
    /// The reply a log line records, unless it records none or Claude wrote it
    /// itself.
    fn from_line(line: LogLine) -> Option<Self> {
        let message = line.message?;
        let usage = message.usage?;
        let model = message.model.filter(|model| model != SYNTHETIC_MODEL)?;
        Some(Self {
            identity: message.id.zip(line.request_id),
            model,
            usage,
        })
    }
}

/// The replies one transcript logged since `since`, newest first. The read
/// runs backwards and stops at the first entry written before `since`.
fn replies_since(reader: impl Read + Seek, since: u64, limits: ReadLimits) -> Vec<Reply> {
    let Ok(lines) = ReverseLines::new(reader, limits) else {
        return Vec::new();
    };
    let mut replies = Vec::new();
    for text in lines {
        // A line still being written, or one that isn't a log entry, records
        // no reply and says nothing about where the window starts.
        let Ok(line) = serde_json::from_str::<LogLine>(&text) else {
            continue;
        };
        let written_at = line.timestamp.as_deref().and_then(timestamp_milliseconds);
        if written_at.is_some_and(|written_at| written_at < since) {
            break;
        }
        replies.extend(Reply::from_line(line));
    }
    replies
}

/// A file's lines, newest first, read backwards a chunk at a time - so a
/// caller that stops early never reads the rest of the file.
struct ReverseLines<R> {
    reader: R,
    limits: ReadLimits,
    /// Where the part of the file not read yet ends.
    unread_end: u64,
    /// The complete lines of the chunks read so far, oldest first, handed out
    /// from the back.
    ready: Vec<String>,
    /// The end of a line whose start lies in a chunk not read yet.
    line_end: Vec<u8>,
    /// Whether the line being assembled outgrew `maximum_line_bytes` and is
    /// being dropped until its start is found.
    is_dropping_line: bool,
}

impl<R: Read + Seek> ReverseLines<R> {
    fn new(mut reader: R, limits: ReadLimits) -> std::io::Result<Self> {
        let unread_end = reader.seek(SeekFrom::End(0))?;
        Ok(Self {
            reader,
            limits,
            unread_end,
            ready: Vec::new(),
            line_end: Vec::new(),
            is_dropping_line: false,
        })
    }

    /// Read the chunk before the part already read and split it into lines.
    /// Its first piece may be the end of a line that started further back; that
    /// piece waits for the next chunk, unless this one began the file.
    fn read_chunk(&mut self) -> std::io::Result<()> {
        let start = self.unread_end.saturating_sub(self.limits.chunk_bytes);
        let length = usize::try_from(self.unread_end - start).map_err(std::io::Error::other)?;
        let mut chunk = vec![0; length];
        self.reader.seek(SeekFrom::Start(start))?;
        self.reader.read_exact(&mut chunk)?;
        self.unread_end = start;
        chunk.append(&mut self.line_end);

        let pieces: Vec<&[u8]> = chunk.split(|byte| *byte == NEWLINE).collect();
        let Some((first, mut complete)) = pieces.split_first() else {
            return Ok(());
        };
        if self.is_dropping_line {
            // The newest piece is the dropped line's start; without a newline
            // the whole chunk still lies inside it.
            let Some((_, before_dropped)) = complete.split_last() else {
                return Ok(());
            };
            complete = before_dropped;
            self.is_dropping_line = false;
        }

        let is_file_start = self.unread_end == 0;
        let mut lines: Vec<&[u8]> = Vec::with_capacity(complete.len() + 1);
        if is_file_start {
            lines.push(first);
        } else if first.len() > self.limits.maximum_line_bytes {
            self.is_dropping_line = true;
        } else {
            self.line_end = first.to_vec();
        }
        lines.extend(complete);
        self.ready = lines
            .into_iter()
            .map(|line| line.strip_suffix(&[CARRIAGE_RETURN]).unwrap_or(line))
            .filter(|line| !line.is_empty())
            .map(|line| String::from_utf8_lossy(line).into_owned())
            .collect();
        Ok(())
    }
}

impl<R: Read + Seek> Iterator for ReverseLines<R> {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        loop {
            if let Some(line) = self.ready.pop() {
                return Some(line);
            }
            if self.unread_end == 0 {
                return None;
            }
            // A file that can no longer be read ends its lines where the read
            // failed - what came before is still a good measurement.
            if self.read_chunk().is_err() {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
        let mut tally = Tally::default();
        tally.count(replies_since(
            Cursor::new(lines.join("\n")),
            window_start(),
            READ_LIMITS,
        ));
        tally.into_usage()
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
    fn a_session_counts_its_subagents_transcripts_too() {
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
        std::fs::write(subagents.join("agent-a.meta.json"), "{}").expect("its metadata");

        // The scratch files are written now, not on the fixtures' date, so the
        // window opens at the epoch to keep the write-time filter out of it.
        let found = usage_since(&transcripts_of(&log, 0), 0);
        let untouched_since_the_future = transcripts_of(&log, u64::MAX);
        std::fs::remove_dir_all(&folder).expect("cleanup");
        crate::test_support::assert_empty(&untouched_since_the_future);
        assert_eq!(
            found,
            vec![usage("claude-haiku-5-5", 2), usage("claude-opus-5-5", 1)]
        );
    }

    #[test]
    fn an_agent_without_a_readable_log_measures_nothing() {
        assert_eq!(
            usage_since(&[PathBuf::from("no/such/transcript.jsonl")], 0),
            Vec::new()
        );
    }
}
