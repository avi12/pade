//! An agent session's context fill, read off disk instead of scraped off the
//! terminal.
//!
//! The terminal is not a reliable source for this. Claude Code only renders its
//! context indicator once a session is within 20k tokens of the compaction
//! threshold — roughly the last 3% of the window — so for the whole life of a
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
//! counters come from tool output — handing it a window would re-arm the false
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
use crate::util::{home_dir, newest_in_tail};

/// The live model catalog. Public, unauthenticated JSON: provider → model id →
/// `limit.context` (the window in tokens).
const MODELS_CATALOG_URL: &str = "https://models.dev/api.json";
/// Refresh the catalog at most this often, so a session that re-reads its
/// context on every turn doesn't refetch 3 MB each time.
const CATALOG_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// One provider block of the models.dev catalog — only the field we read.
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

/// One line of an agent session log — only the shape the token accounting and
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
/// a `_tokens` suffix this struct drops — the type already says what they count.
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
    /// request re-sends — the uncached input, the cached prefix it reads back,
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

    let projects = home_dir()?.join(".claude").join("projects");
    let log_name = format!("{}.jsonl", conversation_id?);
    for project in std::fs::read_dir(projects).ok()?.flatten() {
        let log = project.path().join(&log_name);
        if log.is_file() {
            return Some(log);
        }
    }

    None
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

/// The first `"model":"…"` value in a JSON-lines log — Claude stamps it on every
/// assistant turn, so an early line carries it and the scan stops there instead
/// of reading a multi-megabyte transcript whole.
fn first_model_field(log: &Path) -> Option<String> {
    let reader = BufReader::new(File::open(log).ok()?);
    for line in reader.lines().map_while(Result::ok) {
        if let Some(model) = model_field(&line) {
            return Some(model);
        }
    }

    None
}

/// The value of a `"model":"…"` pair in one line of JSON, without a full parse.
fn model_field(line: &str) -> Option<String> {
    const MARKER: &str = "\"model\":\"";
    let start = line.find(MARKER)? + MARKER.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// The catalog flattened to model id → context window, cached for the run. A
/// model resold by several providers collapses to the largest reported window —
/// its true size, not a reseller's tighter cap.
static CATALOG: Mutex<Option<(Instant, HashMap<String, u64>)>> = Mutex::new(None);

/// The window models.dev reports for `model`, refreshing the catalog when the
/// cached copy has aged past the TTL.
fn window_for_model(model: &str) -> Option<u64> {
    let mut cache = CATALOG.lock().ok()?;
    let fresh =
        matches!(cache.as_ref(), Some((fetched_at, _)) if fetched_at.elapsed() < CATALOG_TTL);
    if !fresh {
        *cache = Some((Instant::now(), fetch_catalog()?));
    }

    cache.as_ref()?.1.get(model).copied()
}

/// Fetch and flatten the models.dev catalog to model id → largest context window.
fn fetch_catalog() -> Option<HashMap<String, u64>> {
    let body = ureq::get(MODELS_CATALOG_URL)
        .call()
        .ok()?
        .into_string()
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
    use super::{model_field, newest_occupied_in, newest_turn_in, CatalogProvider, TurnProfile};
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
}
