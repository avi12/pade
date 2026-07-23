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

use crate::agents;
use crate::util::{command, home_dir};

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

/// The newest recorded opencode session for `cwd`, read through opencode's own
/// `db` subcommand so ADE takes no sqlite dependency for one lookup. Only a
/// session that actually exists is worth a resume flag: a blind `--continue`
/// makes opencode walk its legacy session history, which currently throws a
/// user-visible "unexpected server error" on Windows (opencode #28486) — and
/// a never-prompted session records nothing, so there is nothing to reopen.
/// opencode stores `directory` with forward slashes; compare the normalized,
/// case-folded forms.
fn opencode_session_for_cwd(cwd: &str) -> Option<String> {
    let program = agents::program("opencode")?;
    let directory = cwd.replace('\\', "/").replace('\'', "''");
    let query = format!(
        "select id from session \
         where lower(replace(directory, '\\', '/')) = lower('{directory}') \
         order by time_updated desc limit 1"
    );
    let output = command(program).args(["db", &query]).output().ok()?;
    if !output.status.success() {
        return None;
    }

    opencode_session_id(&String::from_utf8_lossy(&output.stdout))
}

/// The session id in `opencode db`'s tab-separated output — a header line
/// (`id`) followed by at most one `ses_…` row.
fn opencode_session_id(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("ses_"))
        .map(str::to_string)
}

/// The extra launch arguments that reopen the agent's recorded conversation for
/// `cwd` — `["resume", "<uuid>"]` for Codex with a matching rollout,
/// `["--session", "<ses_…>"]` for opencode with a recorded session in that
/// directory, empty when the agent has no resume mechanism ADE can drive
/// (Claude is pinned at spawn via `--session-id` instead) or nothing is
/// recorded.
#[tauri::command]
pub fn agent_resume_args(command: String, cwd: String) -> Vec<String> {
    if command == "opencode" {
        return match opencode_session_for_cwd(&cwd) {
            Some(session_id) => vec!["--session".to_string(), session_id],
            None => Vec::new(),
        };
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

    #[test]
    fn an_opencode_db_row_yields_its_session_id() {
        assert_eq!(
            super::opencode_session_id("id\nses_24440494dffekcRK8jTX3XCccC\n"),
            Some("ses_24440494dffekcRK8jTX3XCccC".to_string())
        );
    }

    #[test]
    fn an_opencode_db_result_without_a_row_yields_nothing() {
        // Header only (no session recorded), empty output, and an error blurb
        // must all read as "nothing to resume", never as a session id.
        assert_eq!(super::opencode_session_id("id\n"), None);
        assert_eq!(super::opencode_session_id(""), None);
        assert_eq!(super::opencode_session_id("Error: Unexpected error"), None);
    }
}
