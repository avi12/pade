//! An agent session's context-window size, resolved without its startup banner.
//!
//! An agent states its window only once — in the banner it paints at startup
//! (`Opus 4.8 (1M context)`). On a long, re-attached session that line has
//! scrolled out of the replayable PTY history, so the frontend can no longer
//! parse it and the context gauge reads "measuring…" forever (and the early
//! auto-handoff never arms). This module recovers it from two durable sources
//! instead: the MODEL the session runs, read off disk, and that model's context
//! window, looked up in the live models.dev catalog. Nothing about model→window
//! is hardcoded — the size is fetched (and refreshed) from models.dev.
//!
//! Only Claude is resolved here on purpose. Codex records its model too, but its
//! only on-screen token counters come from tool output, so handing it a window
//! would re-arm the false handoff fixed in `context.svelte.ts` (a stray
//! tool-output token ÷ a real window). It needs a trustworthy-counter fix first.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::agents::ID_CLAUDE;
use crate::util::home_dir;

/// The live model catalog. Public, unauthenticated JSON: provider → model id →
/// `limit.context` (the window in tokens).
const MODELS_CATALOG_URL: &str = "https://models.dev/api.json";
/// Refresh the catalog at most this often, so a session that re-attaches many
/// times doesn't refetch 3 MB each time.
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

/// The catalog flattened to model id → context window, cached for the run. A
/// model resold by several providers collapses to the largest reported window —
/// its true size, not a reseller's tighter cap.
static CATALOG: Mutex<Option<(Instant, HashMap<String, u64>)>> = Mutex::new(None);

/// The context window (tokens) the session's model advertises, or `None` when
/// the model can't be read or the catalog can't be reached. Off the UI thread:
/// it reads a file and makes a network call.
#[tauri::command]
pub async fn agent_context_window(command: String, conversation_id: Option<String>) -> Option<u64> {
    tauri::async_runtime::spawn_blocking(move || {
        let model = session_model(&command, conversation_id.as_deref())?;
        window_for_model(&model)
    })
    .await
    .ok()
    .flatten()
}

/// The model id a session runs, read off disk so it survives a re-attach. Claude
/// records it per-conversation in its session log; see the module note on why
/// only Claude is resolved.
fn session_model(command: &str, conversation_id: Option<&str>) -> Option<String> {
    if command == ID_CLAUDE {
        return claude_session_model(conversation_id?);
    }

    None
}

/// The model recorded in Claude's session log for `conversation_id`. The log is
/// `~/.claude/projects/<encoded-cwd>/<conversation-id>.jsonl`; the id is unique,
/// so the project directory is matched by scanning rather than by re-deriving
/// Claude's path encoding.
fn claude_session_model(conversation_id: &str) -> Option<String> {
    let projects = home_dir()?.join(".claude").join("projects");
    let log_name = format!("{conversation_id}.jsonl");
    for project in std::fs::read_dir(projects).ok()?.flatten() {
        let log = project.path().join(&log_name);
        if log.is_file() {
            return first_model_field(&log);
        }
    }

    None
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
    use super::{model_field, CatalogProvider};
    use std::collections::HashMap;

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
