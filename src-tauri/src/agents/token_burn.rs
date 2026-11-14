//! How many tokens a Claude session spent lately, read off its own transcripts -
//! the measured load usage pacing shares an account's slowdown by, instead of
//! guessing it from the model and the time the session spent working.
//!
//! Claude Code logs every reply's exact `usage` in the session's transcript, and
//! each subagent's replies in a transcript of its own beside it
//! (`<conversation>/subagents/agent-<id>.jsonl`), so the session's whole spend is
//! the sum over both. Each subagent's share is kept apart, under the id its
//! hooks report, so usage pacing can hold back one subagent on its own. Two
//! things keep the sum honest and cheap:
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
/// A subagent transcript is `agent-<id>.jsonl`, `<id>` being the `agent_id`
/// Claude's hooks report for the subagent's tool calls.
const SUBAGENT_TRANSCRIPT_PREFIX: &str = "agent-";
/// The metadata beside a subagent transcript: `agent-<id>.meta.json`.
const SUBAGENT_META_EXTENSION: &str = "meta.json";
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
pub(crate) struct ModelTokenUsage {
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

/// What a session spent lately, split by who spent it: the session's own
/// replies, and each subagent's - so usage pacing can hold back one subagent
/// without counting it twice in the session's share.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionTokenUsage {
    own: Vec<ModelTokenUsage>,
    subagents: Vec<SubagentTokenUsage>,
}

/// One subagent's recent spend, under the id the agent's hooks name it by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubagentTokenUsage {
    id: String,
    /// The task it was given, as its spawn described it, when recorded.
    description: Option<String>,
    /// When its transcript was last written (milliseconds since the epoch) -
    /// how recently it worked.
    written_at: u64,
    usage: Vec<ModelTokenUsage>,
}

/// The tokens `conversation_id`'s session - and every subagent it ran - spent
/// since `since` (milliseconds since the epoch), one entry per model, the
/// session's own apart from each subagent's. Empty for an agent that keeps no
/// readable log, or a session with no reply yet. Off the UI thread: it reads
/// files.
#[tauri::command]
pub(crate) async fn session_token_usage(
    command: String,
    conversation_id: Option<String>,
    since: u64,
) -> Result<SessionTokenUsage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        claude_session_log(&command, conversation_id.as_deref())
            .map(|log| session_usage(&log, since))
            .unwrap_or_default()
    })
    .await
    .map_err(|error| error.to_string())
}

/// Every reply the session's transcripts logged since `since`, each counted
/// once, summed per model - its own transcript, then each subagent's that was
/// written in the window.
fn session_usage(log: &Path, since: u64) -> SessionTokenUsage {
    let mut tally = Tally::default();
    let own = if is_written_since(log, since) {
        tally.count(transcript_replies(log, since))
    } else {
        Vec::new()
    };
    let subagents = subagent_transcripts(log)
        .into_iter()
        .filter_map(|(id, transcript)| {
            let written_at = modified_milliseconds(&transcript);
            if written_at.is_some_and(|written_at| written_at < since) {
                return None;
            }
            let usage = tally.count(transcript_replies(&transcript, since));
            (!usage.is_empty()).then(|| SubagentTokenUsage {
                description: subagent_description(&transcript),
                written_at: written_at.unwrap_or(since),
                id,
                usage,
            })
        })
        .collect();
    SessionTokenUsage { own, subagents }
}

/// Each subagent transcript in the folder named after the session's, with the
/// subagent id its name carries (`agent-<id>.jsonl`). A session that never ran
/// a subagent has no folder for them.
fn subagent_transcripts(log: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(log.with_extension("").join(SUBAGENTS_DIRECTORY)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == TRANSCRIPT_EXTENSION)
        })
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?;
            let id = stem.strip_prefix(SUBAGENT_TRANSCRIPT_PREFIX)?.to_string();
            Some((id, path))
        })
        .collect()
}

/// What the subagent was asked to do, from the metadata Claude keeps beside
/// its transcript (`agent-<id>.meta.json`): the spawn's description, else its
/// agent type.
fn subagent_description(transcript: &Path) -> Option<String> {
    let meta = crate::util::read_json(&transcript.with_extension(SUBAGENT_META_EXTENSION))?;
    ["description", "agentType"]
        .into_iter()
        .find_map(|key| meta.get(key)?.as_str().map(str::to_string))
}

/// Whether a transcript may hold an entry from `since` on. One whose write time
/// can't be read is read anyway, so a measurement is never lost to it.
fn is_written_since(transcript: &Path, since: u64) -> bool {
    modified_milliseconds(transcript).is_none_or(|modified| modified >= since)
}

/// The replies one transcript logged since `since`; none for a transcript gone
/// between listing and opening.
fn transcript_replies(transcript: &Path, since: u64) -> Vec<Reply> {
    File::open(transcript)
        .map(|file| replies_since(file, since, READ_LIMITS))
        .unwrap_or_default()
}

/// Counts replies once however many lines repeat them - across transcripts
/// too. Fed newest first, so the line it keeps is a reply's last, the one with
/// its final output count.
#[derive(Debug, Default)]
struct Tally {
    counted: HashSet<(String, String)>,
}

impl Tally {
    /// The replies not counted before, summed per model.
    fn count(&mut self, replies: Vec<Reply>) -> Vec<ModelTokenUsage> {
        let mut by_model = BTreeMap::<String, ModelTokenUsage>::new();
        for reply in replies {
            let is_repeat = reply
                .identity
                .is_some_and(|identity| !self.counted.insert(identity));
            if is_repeat {
                continue;
            }
            by_model
                .entry(reply.model)
                .or_insert_with_key(|model| ModelTokenUsage::named(model))
                .add(&reply.usage);
        }
        by_model.into_values().collect()
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
mod tests;
