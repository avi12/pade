//! Resume arguments for agents whose CLI can reopen a recorded conversation.
//!
//! Codex records every interactive session as a rollout file under
//! `~/.codex/sessions/<yyyy>/<mm>/<dd>/rollout-…-<uuid>.jsonl`, whose first
//! line is a `session_meta` carrying the session's `cwd` and how it was
//! started. ADE has no way to *choose* that id at spawn (unlike Claude's
//! `--session-id`), so when a session must respawn — a scheme flip re-theming
//! an arg-themed CLI — the newest interactive rollout for the same working
//! directory is the conversation to reopen with `codex resume <uuid>`.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::util::home_dir;

/// How many of the newest rollout files are worth inspecting before giving up.
/// Oneshot `codex exec` runs (auto-naming) also record rollouts, so a few
/// non-interactive entries may sit above the wanted one; a bound keeps a huge
/// sessions directory from turning one lookup into a full scan.
const NEWEST_ROLLOUTS_INSPECTED: usize = 32;

/// The newest interactive Codex session recorded for `cwd`, as the session id
/// `codex resume` accepts. `None` when nothing matches — the caller then spawns
/// a fresh conversation.
fn codex_session_for_cwd(cwd: &str) -> Option<String> {
    let sessions_dir = home_dir()?.join(".codex").join("sessions");
    let mut rollouts = rollout_files(&sessions_dir);
    rollouts.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));

    let wanted = normalized(cwd);
    rollouts
        .into_iter()
        .take(NEWEST_ROLLOUTS_INSPECTED)
        .find_map(|(_, path)| {
            let meta = first_line(&path)?;
            interactive_session_id(&meta, &wanted)
        })
}

/// Every rollout `.jsonl` under the sessions tree, with its modified time.
fn rollout_files(dir: &Path) -> Vec<(std::time::SystemTime, PathBuf)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(rollout_files(&path));
            continue;
        }

        let is_rollout = path
            .extension()
            .is_some_and(|extension| extension == "jsonl");
        if !is_rollout {
            continue;
        }

        if let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) {
            found.push((modified, path));
        }
    }

    found
}

fn first_line(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut line = String::new();
    BufReader::new(file).read_line(&mut line).ok()?;
    Some(line)
}

/// Case- and separator-insensitive path form for comparing the rollout's
/// recorded `cwd` with the session's.
fn normalized(path: &str) -> String {
    path.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// Pull the session id out of one `session_meta` line — only when it records an
/// interactive session (a `codex exec` oneshot has `"source":"exec"` and cannot
/// be resumed into a TUI conversation) in the wanted working directory.
/// Field extraction over full JSON parsing: the meta line drags in the entire
/// base-instructions prompt, and two string fields don't justify a serde model
/// of Codex's private schema.
fn interactive_session_id(meta: &str, wanted_cwd: &str) -> Option<String> {
    if json_string_field(meta, "source").is_some_and(|source| source == "exec") {
        return None;
    }

    let cwd = json_string_field(meta, "cwd")?;
    if normalized(&cwd.replace("\\\\", "\\")) != wanted_cwd {
        return None;
    }

    json_string_field(meta, "session_id")
}

/// The raw value of the first `"key":"value"` occurrence in `json`. Enough for
/// the flat string fields the meta line carries; escaped quotes inside values
/// don't occur in ids and the cwd's escaped backslashes are undone by the caller.
fn json_string_field(json: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker)? + marker.len();
    let end = json[start..].find('"')?;
    Some(json[start..start + end].to_string())
}

/// The extra launch arguments that reopen the agent's recorded conversation for
/// `cwd` — `["resume", "<uuid>"]` for Codex with a matching rollout,
/// `["--continue"]` for opencode (it keys "the last session" off the working
/// directory itself, so no id lookup is needed), empty when the agent has no
/// resume mechanism ADE can drive (Claude is pinned at spawn via `--session-id`
/// instead) or nothing is recorded.
#[tauri::command]
pub fn agent_resume_args(command: String, cwd: String) -> Vec<String> {
    if command == "opencode" {
        return vec!["--continue".to_string()];
    }

    if command != "codex" {
        return Vec::new();
    }

    match codex_session_for_cwd(&cwd) {
        Some(session_id) => vec!["resume".to_string(), session_id],
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{interactive_session_id, json_string_field, normalized};

    const META: &str = r#"{"timestamp":"t","type":"session_meta","payload":{"session_id":"019f-abc","id":"019f-abc","cwd":"C:\\repositories\\avi\\proj","originator":"codex_exec","cli_version":"0.144.5","source":"exec"}}"#;

    #[test]
    fn a_oneshot_exec_rollout_is_never_resumed() {
        assert_eq!(
            interactive_session_id(META, &normalized("C:\\repositories\\avi\\proj")),
            None
        );
    }

    #[test]
    fn an_interactive_rollout_in_the_wanted_cwd_yields_its_session_id() {
        let interactive = META.replace("\"source\":\"exec\"", "\"source\":\"tui\"");
        assert_eq!(
            interactive_session_id(&interactive, &normalized("C:/repositories/avi/PROJ")),
            Some("019f-abc".to_string())
        );
    }

    #[test]
    fn a_rollout_from_another_project_is_skipped() {
        let interactive = META.replace("\"source\":\"exec\"", "\"source\":\"tui\"");
        assert_eq!(
            interactive_session_id(&interactive, &normalized("C:\\elsewhere")),
            None
        );
    }

    #[test]
    fn string_fields_are_extracted_without_a_full_parse() {
        assert_eq!(json_string_field(META, "source"), Some("exec".to_string()));
        assert_eq!(
            json_string_field(META, "session_id"),
            Some("019f-abc".to_string())
        );
        assert_eq!(json_string_field(META, "missing"), None);
    }
}
