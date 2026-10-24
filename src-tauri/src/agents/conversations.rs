//! Every recorded conversation of a project, across the agents that keep one
//! on disk - the list each CLI's own `/resume` picker shows, read straight off
//! their files so the sidebar can offer it without a terminal:
//!
//!   * Claude Code - `~/.claude/projects/<encoded project>/<id>.jsonl`, titled
//!     by the newest `custom-title` (a `/rename`) or `ai-title` record, which
//!     Claude re-appends near the end, else by the first typed prompt;
//!   * Codex - interactive rollouts whose `session_meta` names the project as
//!     their `cwd`, titled from `session_index.jsonl`;
//!   * Copilot - `~/.copilot/session-state/<id>/workspace.yaml`, by its `cwd`
//!     and `summary`.
//!
//! Claude and Copilot reopen a conversation through the id PADE pins at spawn
//! (`session_id_flag`), Codex through `codex resume <id>`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

use serde::Serialize;

use crate::agents::resume::{first_line, interactive_session_id, rollout_files};
use crate::agents::running_model::{copilot_session_root, session_resume_arguments};
use crate::agents::{codex_home, ID_CLAUDE, ID_CODEX, ID_COPILOT};
use crate::util::{encode_project, home_directory, milliseconds_since_unix_epoch, same_path};

/// The most conversations listed for one project, newest first - `/resume`
/// pages its own list the same way.
const MAXIMUM_CONVERSATIONS: usize = 50;
/// How many of the newest files of each agent are inspected, so a long history
/// never turns one listing into a full scan.
const NEWEST_FILES_INSPECTED: usize = 256;
/// How much of a Claude transcript's end is read for its title records.
const TRANSCRIPT_TAIL_BYTES: u64 = 64 * 1024;
/// How much of a Claude transcript's start is read for its first prompt.
const TRANSCRIPT_HEAD_BYTES: u64 = 256 * 1024;
const TRANSCRIPT_EXTENSION: &str = "jsonl";
const CUSTOM_TITLE_RECORD: &str = "custom-title";
const AI_TITLE_RECORD: &str = "ai-title";
const USER_RECORD: &str = "user";
/// Claude's `entrypoint` for a headless `claude -p` run (PADE's own naming
/// asks it this way), which `/resume` doesn't offer either.
const HEADLESS_ENTRYPOINT_PREFIX: &str = "sdk";
const CODEX_SESSION_INDEX: &str = "session_index.jsonl";
const COPILOT_WORKSPACE_FILE: &str = "workspace.yaml";
const COPILOT_CWD_KEY: &str = "cwd:";
const COPILOT_SUMMARY_KEY: &str = "summary:";

/// One recorded conversation, as the sidebar lists and reopens it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedConversation {
    agent_id: &'static str,
    /// The agent's own id for the conversation.
    conversation_id: String,
    title: Option<String>,
    /// Milliseconds since the Unix epoch.
    updated_at: u64,
    /// The launch arguments that reopen it, for an agent that isn't pinned by
    /// `conversation_id` at spawn.
    resume_arguments: Vec<String>,
}

/// `project`'s recorded conversations, newest first.
// `async` plus `spawn_blocking`: it reads many files, which must stay off the
// main thread.
#[tauri::command]
pub async fn agent_conversations(project: String) -> Vec<RecordedConversation> {
    tauri::async_runtime::spawn_blocking(move || project_conversations(&project))
        .await
        .unwrap_or_default()
}

fn project_conversations(project: &str) -> Vec<RecordedConversation> {
    let mut conversations = claude_conversations(project);
    conversations.extend(codex_conversations(project));
    conversations.extend(copilot_conversations(project));
    conversations.sort_by_key(|conversation| std::cmp::Reverse(conversation.updated_at));
    conversations.truncate(MAXIMUM_CONVERSATIONS);
    conversations
}

/// The newest entries first, bounded to the ones worth inspecting.
fn newest<T>(mut entries: Vec<(SystemTime, T)>) -> impl Iterator<Item = (SystemTime, T)> {
    entries.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    entries.into_iter().take(NEWEST_FILES_INSPECTED)
}

fn modified_time(path: &Path) -> Option<SystemTime> {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
}

fn claude_conversations(project: &str) -> Vec<RecordedConversation> {
    let Some(home) = home_directory() else {
        return Vec::new();
    };
    let directory = home
        .join(".claude")
        .join("projects")
        .join(encode_project(project));
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let transcripts = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == TRANSCRIPT_EXTENSION)
        })
        .filter_map(|path| Some((modified_time(&path)?, path)))
        .collect();

    newest(transcripts)
        .filter_map(|(modified, path)| {
            let conversation_id = path.file_stem()?.to_str()?.to_string();
            let first_prompt = first_typed_prompt(&path)?;
            Some(RecordedConversation {
                agent_id: ID_CLAUDE,
                conversation_id,
                title: transcript_title(&path).or(Some(first_prompt)),
                updated_at: milliseconds_since_unix_epoch(modified),
                resume_arguments: Vec::new(),
            })
        })
        .collect()
}

/// The JSON records of a transcript's lines, skipping any that don't parse (a
/// line cut by a bounded read).
fn records(reader: impl BufRead) -> impl Iterator<Item = serde_json::Value> {
    reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
}

fn record_type(record: &serde_json::Value) -> Option<&str> {
    record.get("type")?.as_str()
}

/// The newest `/rename` title, else Claude's own generated one.
fn transcript_title(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(
        length.saturating_sub(TRANSCRIPT_TAIL_BYTES),
    ))
    .ok()?;
    let mut custom_title = None;
    let mut ai_title = None;
    for record in records(BufReader::new(file)) {
        match record_type(&record) {
            Some(CUSTOM_TITLE_RECORD) => custom_title = string_field(&record, "customTitle"),
            Some(AI_TITLE_RECORD) => ai_title = string_field(&record, "aiTitle"),
            _ => {}
        }
    }
    custom_title.or(ai_title)
}

fn string_field(record: &serde_json::Value, key: &str) -> Option<String> {
    let value = record.get(key)?.as_str()?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// The first prompt typed into the conversation. `None` for a transcript with
/// none, or one a headless run recorded - neither is a conversation to resume.
fn first_typed_prompt(path: &Path) -> Option<String> {
    let head = File::open(path).ok()?.take(TRANSCRIPT_HEAD_BYTES);
    let first_user_record =
        records(BufReader::new(head)).find(|record| record_type(record) == Some(USER_RECORD))?;
    let is_headless = first_user_record
        .get("entrypoint")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|entrypoint| entrypoint.starts_with(HEADLESS_ENTRYPOINT_PREFIX));
    if is_headless {
        return None;
    }

    let content = first_user_record.get("message")?.get("content")?;
    let text = content.as_str().map(str::to_string).or_else(|| {
        content
            .as_array()?
            .iter()
            .find_map(|part| part.get("text")?.as_str().map(str::to_string))
    })?;
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn codex_conversations(project: &str) -> Vec<RecordedConversation> {
    let Some(home) = codex_home() else {
        return Vec::new();
    };
    let titles = codex_titles(&home.join(CODEX_SESSION_INDEX));
    newest(rollout_files(&home.join("sessions")))
        .filter_map(|(modified, path)| {
            let session_id = interactive_session_id(&first_line(&path)?, project)?;
            Some(RecordedConversation {
                agent_id: ID_CODEX,
                title: titles.get(&session_id).cloned(),
                updated_at: milliseconds_since_unix_epoch(modified),
                resume_arguments: session_resume_arguments(ID_CODEX, session_id.clone())
                    .unwrap_or_default(),
                conversation_id: session_id,
            })
        })
        .collect()
}

/// Codex's thread names by session id; a later line renames an earlier one.
fn codex_titles(index: &Path) -> HashMap<String, String> {
    let Ok(file) = File::open(index) else {
        return HashMap::new();
    };
    records(BufReader::new(file))
        .filter_map(|record| {
            Some((
                string_field(&record, "id")?,
                string_field(&record, "thread_name")?,
            ))
        })
        .collect()
}

fn copilot_conversations(project: &str) -> Vec<RecordedConversation> {
    let Some(Ok(entries)) = copilot_session_root().map(std::fs::read_dir) else {
        return Vec::new();
    };
    let workspaces = entries
        .flatten()
        .map(|entry| entry.path().join(COPILOT_WORKSPACE_FILE))
        .filter_map(|path| Some((modified_time(&path)?, path)))
        .collect();

    newest(workspaces)
        .filter_map(|(modified, path)| {
            let workspace = std::fs::read_to_string(&path).ok()?;
            let cwd = yaml_value(&workspace, COPILOT_CWD_KEY)?;
            if !same_path(&cwd, project) {
                return None;
            }

            Some(RecordedConversation {
                agent_id: ID_COPILOT,
                conversation_id: path.parent()?.file_name()?.to_str()?.to_string(),
                title: yaml_value(&workspace, COPILOT_SUMMARY_KEY),
                updated_at: milliseconds_since_unix_epoch(modified),
                resume_arguments: Vec::new(),
            })
        })
        .collect()
}

/// A top-level scalar of Copilot's flat `workspace.yaml`, its quotes dropped.
fn yaml_value(yaml: &str, key: &str) -> Option<String> {
    let value = yaml
        .lines()
        .find_map(|line| line.strip_prefix(key))?
        .trim()
        .trim_matches('"');
    (!value.is_empty()).then(|| value.replace("\\\\", "\\"))
}

#[cfg(test)]
mod tests {
    use super::{first_typed_prompt, transcript_title, yaml_value};

    fn scratch_transcript(name: &str, lines: &[&str]) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("pade-conversations-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("scratch");
        let path = directory.join("conversation.jsonl");
        std::fs::write(&path, lines.join("\n")).expect("write");
        path
    }

    const PROMPT: &str =
        r#"{"type":"user","entrypoint":"cli","message":{"role":"user","content":"fix the build"}}"#;

    #[test]
    fn a_rename_wins_over_the_generated_title() {
        let path = scratch_transcript(
            "rename",
            &[
                PROMPT,
                r#"{"type":"ai-title","aiTitle":"Build fix"}"#,
                r#"{"type":"custom-title","customTitle":"My build"}"#,
            ],
        );
        assert_eq!(transcript_title(&path).as_deref(), Some("My build"));
    }

    #[test]
    fn an_untitled_transcript_falls_back_to_its_first_prompt() {
        let path = scratch_transcript("untitled", &[PROMPT]);
        assert_eq!(transcript_title(&path), None);
        assert_eq!(first_typed_prompt(&path).as_deref(), Some("fix the build"));
    }

    #[test]
    fn a_headless_run_is_not_a_conversation() {
        let path = scratch_transcript(
            "headless",
            &[r#"{"type":"user","entrypoint":"sdk-cli","message":{"content":"name this"}}"#],
        );
        assert_eq!(first_typed_prompt(&path), None);
    }

    #[test]
    fn copilot_workspace_values_lose_their_quotes_and_escapes() {
        let yaml = "id: 1\ncwd: C:\\work\\app\nsummary: \"chat C:\\\\work\\\\app\"\n";
        assert_eq!(yaml_value(yaml, "cwd:").as_deref(), Some(r"C:\work\app"));
        assert_eq!(
            yaml_value(yaml, "summary:").as_deref(),
            Some(r"chat C:\work\app")
        );
        assert_eq!(yaml_value(yaml, "branch:"), None);
    }
}
