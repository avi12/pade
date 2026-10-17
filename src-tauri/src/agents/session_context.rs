//! An agent session's context fill, read off disk instead of scraped off the
//! terminal.
//!
//! The terminal is not a reliable source for this. Claude Code only renders its
//! context indicator once a session is within 20k tokens of the compaction
//! threshold - roughly the last 3% of the window - so for the whole life of a
//! normal session there is simply no percentage on screen to parse, and the
//! frontend's byte/token fallbacks answer with something unrelated to the real
//! fill. The agent's own session log carries the truth on every turn: the
//! `usage` block of the newest assistant message is exactly what the next
//! request will re-send.
//!
//! So this module answers both halves of the fraction from durable sources:
//! the USED tokens from that log, and the WINDOW from the model the same log
//! records, sized by the live models.dev catalog. Nothing about model→window is
//! hardcoded.
//!
//! Only Claude is resolved here on purpose. Codex records its model too, but it
//! writes no per-turn token accounting this can read, and its only on-screen
//! counters come from tool output - handing it a window would re-arm the false
//! handoff fixed in `context.svelte.ts` (a stray tool-output token ÷ a real
//! window). It needs its own trustworthy source first.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::agents::ID_CLAUDE;
use crate::util::{home_directory, json_string_field, newest_in_tail};

/// The live model catalog. Public, unauthenticated JSON: provider → model id →
/// `limit.context` (the window in tokens).
const MODELS_CATALOG_URL: &str = "https://models.dev/api.json";
/// Refresh the catalog at most this often, so a session that re-reads its
/// context on every turn doesn't refetch 3 MB each time.
const CATALOG_TTL: Duration = Duration::from_hours(6);

/// One provider block of the models.dev catalog - only the field we read.
#[derive(Debug, Deserialize)]
struct CatalogProvider {
    models: HashMap<String, CatalogModel>,
}

#[derive(Debug, Deserialize)]
struct CatalogModel {
    limit: Option<CatalogLimit>,
}

#[derive(Debug, Deserialize)]
struct CatalogLimit {
    context: Option<u64>,
}

/// One line of an agent session log - only the shape the token accounting and
/// the turn's model and effort live in. Every other field is ignored.
#[derive(Debug, Deserialize)]
struct LogLine {
    message: Option<LogMessage>,
    /// The reasoning effort the session runs at, stamped beside every turn.
    effort: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LogMessage {
    model: Option<String>,
    usage: Option<TokenUsage>,
}

/// Claude's placeholder model on turns it writes itself (an API error, an
/// interrupt) - not a model anything ran on.
const SYNTHETIC_MODEL: &str = "<synthetic>";

/// The token accounting an assistant turn records. Every count is optional
/// because the same log carries other message shapes. The wire names all carry
/// a `_tokens` suffix this struct drops - the type already says what they count.
#[derive(Debug, Deserialize)]
struct TokenUsage {
    #[serde(rename = "input_tokens")]
    input: Option<u64>,
    #[serde(rename = "output_tokens")]
    output: Option<u64>,
    #[serde(rename = "cache_read_input_tokens")]
    cache_read: Option<u64>,
    #[serde(rename = "cache_creation_input_tokens")]
    cache_written: Option<u64>,
}

impl TokenUsage {
    /// What the conversation occupies after this turn: everything the next
    /// request re-sends - the uncached input, the cached prefix it reads back,
    /// the prefix it just wrote, and the reply it just produced. Cache reads
    /// dominate a long session, which is why a counter of streamed tokens alone
    /// under-reports the fill so badly.
    fn occupied(&self) -> Option<u64> {
        let input = self.input?;
        Some(
            input
                + self.output.unwrap_or_default()
                + self.cache_read.unwrap_or_default()
                + self.cache_written.unwrap_or_default(),
        )
    }
}

/// A session's context fill as the agent itself accounts for it. Each half is
/// optional: a session whose first turn hasn't been written yet has a model but
/// no usage, and a catalog that can't be reached leaves the window unknown
/// while the used count is still good.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext {
    used_tokens: Option<u64>,
    window_tokens: Option<u64>,
}

/// What a turn ran on, as the log records it.
#[derive(Debug, PartialEq, Eq)]
pub struct TurnProfile {
    pub model: String,
    pub effort: Option<String>,
}

/// The session's context fill, or `None` when the agent keeps no readable log
/// for it. Off the UI thread: it reads a file and may make a network call.
#[tauri::command]
pub async fn agent_session_context(
    command: String,
    conversation_id: Option<String>,
) -> Option<SessionContext> {
    tauri::async_runtime::spawn_blocking(move || {
        let log = claude_session_log(&command, conversation_id.as_deref())?;
        Some(SessionContext {
            used_tokens: newest_in_tail(&log, newest_occupied_in),
            window_tokens: first_model_field(&log).and_then(|model| window_for_model(&model)),
        })
    })
    .await
    .ok()
    .flatten()
}

/// The model and effort Claude's own log says the newest real turn of
/// `conversation_id` ran on - the fallback for a session launched before PADE
/// attached its hooks (see `running_model`).
pub fn logged_claude_turn(conversation_id: &str) -> Option<TurnProfile> {
    let log = claude_session_log(ID_CLAUDE, Some(conversation_id))?;
    newest_in_tail(&log, newest_turn_in)
}

/// The log entry kinds that belong to the conversation itself - what was said,
/// what the tools returned, and the links Claude recorded (`pr-link`). The
/// rest of a log (tool schemas, system attachments, settings) is the app's
/// own furniture and full of addresses no one worked with.
const CONVERSATION_ENTRY_KINDS: &[&str] = &["user", "assistant", "pr-link"];
/// How many of a project's most recent other session logs on the same branch
/// add their links - a resumed session continues in a fresh log that doesn't
/// carry the earlier ones.
const EARLIER_LOGS_SEARCHED: usize = 30;
/// Links kept per log, newest last.
const LINKS_PER_LOG: usize = 40;
/// What marks a log entry's branch.
const GIT_BRANCH_FIELD: &str = "gitBranch";
const WEB_ADDRESS_START: &str = "https://";
/// Where an address embedded in log text ends: whitespace, or a character
/// JSON, Markdown or a sentence wraps it in.
const ADDRESS_ENDS: &[char] = &[
    '"', '\'', '\\', '`', '<', '>', '(', ')', '[', ']', '{', '}', '|', ',',
];
/// Punctuation a sentence leaves stuck to the end of an address.
const TRAILING_PUNCTUATION: &[char] = &['.', ':', ';', '!', '?'];

/// One log entry's kind, read without parsing the rest of it.
#[derive(Deserialize)]
struct EntryKind {
    #[serde(rename = "type")]
    kind: String,
}

/// The web addresses a conversation has worked with, newest first - links it
/// opened, created or was shown, from its own log and the earlier logs of the
/// same project on the same branch. Read off Claude's own logs, so nothing is
/// looked up; whatever kind of address it is, a pull request, a CI run, a
/// deploy, it is simply an address the session knows.
pub(crate) fn session_links(conversation_id: &str) -> Vec<String> {
    let Some(log) = claude_session_log(ID_CLAUDE, Some(conversation_id)) else {
        return Vec::new();
    };
    let mut logs = vec![log.clone()];
    logs.extend(earlier_logs_on_branch(&log));

    let mut links: Vec<String> = Vec::new();
    for each_log in &logs {
        for link in links_of(each_log).into_iter().rev() {
            if !links.contains(&link) {
                links.push(link);
            }
        }
    }
    links
}

/// The project's other logs on `log`'s branch, newest first.
fn earlier_logs_on_branch(log: &Path) -> Vec<PathBuf> {
    let (Some(branch), Some(folder)) = (branch_of(log), log.parent()) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut earlier: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path != log
                && path
                    .extension()
                    .is_some_and(|extension| extension == "jsonl")
        })
        .filter_map(|path| Some((std::fs::metadata(&path).ok()?.modified().ok()?, path)))
        .collect();
    earlier.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    earlier
        .into_iter()
        .take(EARLIER_LOGS_SEARCHED)
        .map(|(_, path)| path)
        .filter(|path| branch_of(path).as_deref() == Some(branch.as_str()))
        .collect()
}

/// The branch a log's newest entry was written on.
fn branch_of(log: &Path) -> Option<String> {
    newest_in_tail(log, |tail| {
        tail.lines()
            .rev()
            .find_map(|line| json_string_field(line, GIT_BRANCH_FIELD))
            .filter(|branch| !branch.is_empty())
    })
}

/// Each log's links, kept until the log changes - a link can sit megabytes
/// back, so a log is read through once per change, not once per summary.
type ReadLinks = (std::time::SystemTime, Vec<String>);
static LOG_LINKS: std::sync::LazyLock<Mutex<HashMap<PathBuf, ReadLinks>>> =
    std::sync::LazyLock::new(Default::default);

/// The distinct addresses in a log's conversation entries, oldest first, the
/// last [`LINKS_PER_LOG`] of them.
fn links_of(log: &Path) -> Vec<String> {
    let Some(modified) = std::fs::metadata(log)
        .and_then(|metadata| metadata.modified())
        .ok()
    else {
        return Vec::new();
    };
    let cached = LOG_LINKS.lock().ok().and_then(|links| {
        let (read_at, links) = links.get(log)?;
        (*read_at == modified).then(|| links.clone())
    });
    if let Some(links) = cached {
        return links;
    }

    let mut links: Vec<String> = Vec::new();
    if let Ok(file) = File::open(log) {
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if !is_conversation_entry(&line) {
                continue;
            }
            for address in addresses_in(&line) {
                links.retain(|known| known != address);
                links.push(address.to_string());
            }
        }
    }
    let kept = links.split_off(links.len().saturating_sub(LINKS_PER_LOG));
    if let Ok(mut cache) = LOG_LINKS.lock() {
        cache.insert(log.to_path_buf(), (modified, kept.clone()));
    }
    kept
}

/// Whether a log line is one of the conversation's own entries. A line with no
/// address at all is skipped before it is parsed.
fn is_conversation_entry(line: &str) -> bool {
    line.contains(WEB_ADDRESS_START)
        && serde_json::from_str::<EntryKind>(line)
            .is_ok_and(|entry| CONVERSATION_ENTRY_KINDS.contains(&entry.kind.as_str()))
}

/// Every web address written in `text`, in order.
fn addresses_in(text: &str) -> Vec<&str> {
    text.match_indices(WEB_ADDRESS_START)
        .filter_map(|(start, _)| {
            let rest = &text[start..];
            let end = rest
                .find(|character: char| {
                    character.is_whitespace() || ADDRESS_ENDS.contains(&character)
                })
                .unwrap_or(rest.len());
            let address = rest[..end].trim_end_matches(TRAILING_PUNCTUATION);
            (address.len() > WEB_ADDRESS_START.len()).then_some(address)
        })
        .collect()
}

/// Where Claude keeps `conversation_id`'s transcript, when it has one yet.
pub fn claude_transcript(conversation_id: &str) -> Option<PathBuf> {
    claude_session_log(ID_CLAUDE, Some(conversation_id))
}

/// The session log Claude keeps for `conversation_id`, at
/// `~/.claude/projects/<encoded-cwd>/<conversation-id>.jsonl`. The id is unique,
/// so the project directory is matched by scanning rather than by re-deriving
/// Claude's path encoding. The one place this file is located; see the module
/// note on why only Claude is resolved.
fn claude_session_log(command: &str, conversation_id: Option<&str>) -> Option<PathBuf> {
    if command != ID_CLAUDE {
        return None;
    }

    let projects = home_directory()?.join(".claude").join("projects");
    let log_name = format!("{}.jsonl", conversation_id?);
    std::fs::read_dir(projects)
        .ok()?
        .flatten()
        .map(|project| project.path().join(&log_name))
        .find(|log| log.is_file())
}

/// The newest complete usage total in a chunk of JSON lines.
fn newest_occupied_in(lines: &str) -> Option<u64> {
    lines.lines().rev().find_map(|line| {
        serde_json::from_str::<LogLine>(line)
            .ok()?
            .message?
            .usage?
            .occupied()
    })
}

/// The model and effort of the newest real turn in a chunk of JSON lines - the
/// newest, not the first, so a mid-session `/model` or `/effort` switch shows.
fn newest_turn_in(lines: &str) -> Option<TurnProfile> {
    lines.lines().rev().find_map(turn_from_line)
}

/// The turn one log line records, unless it has no model or Claude wrote it
/// itself.
fn turn_from_line(line: &str) -> Option<TurnProfile> {
    let parsed = serde_json::from_str::<LogLine>(line).ok()?;
    let model = parsed.message?.model?;
    let is_real_turn = model != SYNTHETIC_MODEL;
    is_real_turn.then_some(TurnProfile {
        model,
        effort: parsed.effort,
    })
}

/// The first `"model":"…"` value in a JSON-lines log - Claude stamps it on every
/// assistant turn, so an early line carries it and the scan stops there instead
/// of reading a multi-megabyte transcript whole.
fn first_model_field(log: &Path) -> Option<String> {
    BufReader::new(File::open(log).ok()?)
        .lines()
        .map_while(Result::ok)
        .find_map(|line| model_field(&line))
}

/// The value of a `"model":"…"` pair in one line of JSON, without a full parse.
fn model_field(line: &str) -> Option<String> {
    json_string_field(line, MODEL_FIELD)
}

const MODEL_FIELD: &str = "model";

/// The catalog flattened to model id → context window, cached for the run. A
/// model resold by several providers collapses to the largest reported window -
/// its true size, not a reseller's tighter cap.
static CATALOG: Mutex<Option<(Instant, HashMap<String, u64>)>> = Mutex::new(None);

/// The window models.dev reports for `model`, refreshing the catalog when the
/// cached copy has aged past the TTL.
fn window_for_model(model: &str) -> Option<u64> {
    {
        let cache = CATALOG.lock().ok()?;
        if let Some((fetched_at, windows)) = cache.as_ref() {
            let is_fresh = fetched_at.elapsed() < CATALOG_TTL;
            if is_fresh {
                return windows.get(model).copied();
            }
        }
    }

    // The fetch is a multi-megabyte network call; it runs with the cache
    // unlocked so a slow models.dev never queues every other session's read
    // behind it. Two sessions racing past a stale cache may both fetch - the
    // later write simply wins, and both answers are equally fresh.
    let windows = fetch_catalog()?;
    let window = windows.get(model).copied();
    *CATALOG.lock().ok()? = Some((Instant::now(), windows));
    window
}

/// Fetch and flatten the models.dev catalog to model id → largest context window.
fn fetch_catalog() -> Option<HashMap<String, u64>> {
    let body = ureq::get(MODELS_CATALOG_URL)
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    let providers: HashMap<String, CatalogProvider> = serde_json::from_str(&body).ok()?;

    let mut windows: HashMap<String, u64> = HashMap::new();
    for provider in providers.values() {
        for (model, entry) in &provider.models {
            let Some(context) = entry.limit.as_ref().and_then(|limit| limit.context) else {
                continue;
            };
            let slot = windows.entry(model.clone()).or_default();
            *slot = (*slot).max(context);
        }
    }

    (!windows.is_empty()).then_some(windows)
}

#[cfg(test)]
mod tests {
    use super::{
        addresses_in, is_conversation_entry, model_field, newest_occupied_in, newest_turn_in,
        CatalogProvider, TurnProfile,
    };
    use std::collections::HashMap;

    fn assistant_line(input: u64, cache_read: u64, cache_write: u64, output: u64) -> String {
        format!(
            r#"{{"type":"assistant","message":{{"model":"claude-opus-5","usage":{{"input_tokens":{input},"cache_read_input_tokens":{cache_read},"cache_creation_input_tokens":{cache_write},"output_tokens":{output}}}}}}}"#
        )
    }

    #[test]
    fn reads_the_model_from_a_session_line() {
        let line =
            r#"{"type":"assistant","message":{"model":"claude-opus-4-8","role":"assistant"}}"#;
        assert_eq!(model_field(line).as_deref(), Some("claude-opus-4-8"));
    }

    #[test]
    fn a_line_without_a_model_field_yields_none() {
        assert_eq!(model_field(r#"{"type":"summary","summary":"hi"}"#), None);
    }

    #[test]
    fn occupied_tokens_count_the_cached_prefix_not_just_what_streamed() {
        let lines = assistant_line(2, 472_681, 840, 447);
        assert_eq!(newest_occupied_in(&lines), Some(473_970));
    }

    #[test]
    fn the_newest_turn_wins_over_the_earlier_ones() {
        let lines = [
            assistant_line(5, 100_000, 500, 900),
            assistant_line(2, 300_000, 700, 300),
            r#"{"type":"user","message":{"role":"user","content":"next"}}"#.to_string(),
        ]
        .join("\n");
        assert_eq!(newest_occupied_in(&lines), Some(301_002));
    }

    #[test]
    fn a_tail_starting_mid_line_skips_the_broken_first_line() {
        let complete = assistant_line(1, 2_000, 10, 30);
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
}
