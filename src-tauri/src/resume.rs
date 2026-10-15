//! Resume arguments for agents whose CLI can reopen a recorded conversation.
//!
//! Codex records every interactive session as a rollout file under its home
//! (`$CODEX_HOME`, `~/.codex` by default), at
//! `sessions/<yyyy>/<mm>/<dd>/rollout-…-<uuid>.jsonl`, whose first line is a `session_meta` carrying the session's `cwd` and how it was
//! started. ADE has no way to *choose* that id at spawn (unlike Claude's
//! `--session-id`), so when a session must respawn - a scheme flip re-theming
//! an arg-themed CLI - the newest interactive rollout for the same working
//! directory is the conversation to reopen with `codex resume <uuid>`.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::agents::{codex_home, program, ID_CODEX, ID_OPENCODE};
use crate::util::{command, json_string_field, same_path};

/// The folder under Codex's home that holds its rollout files.
const CODEX_SESSIONS_DIRECTORY: &str = "sessions";
/// The rollout file extension and the `session_meta` fields read off its first
/// line.
const ROLLOUT_EXTENSION: &str = "jsonl";
const SOURCE_FIELD: &str = "source";
const CWD_FIELD: &str = "cwd";
const SESSION_ID_FIELD: &str = "session_id";
/// The `source` a oneshot `codex exec` run records.
const EXEC_SOURCE: &str = "exec";
/// Every opencode session id starts with this.
const OPENCODE_SESSION_PREFIX: &str = "ses_";

/// How many of the newest rollout files are worth inspecting before giving up.
/// Oneshot `codex exec` runs (auto-naming) also record rollouts, so a few
/// non-interactive entries may sit above the wanted one; a bound keeps a huge
/// sessions directory from turning one lookup into a full scan.
const NEWEST_ROLLOUTS_INSPECTED: usize = 32;

/// The newest interactive Codex session recorded for `cwd`, as the session id
/// `codex resume` accepts. `None` when nothing matches - the caller then spawns
/// a fresh conversation.
fn codex_session_for_cwd(cwd: &str) -> Option<String> {
    let sessions_directory = codex_home()?.join(CODEX_SESSIONS_DIRECTORY);
    let mut rollouts = rollout_files(&sessions_directory);
    rollouts.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));

    rollouts
        .into_iter()
        .take(NEWEST_ROLLOUTS_INSPECTED)
        .find_map(|(_, path)| {
            let metadata_line = first_line(&path)?;
            interactive_session_id(&metadata_line, cwd)
        })
}

/// Every rollout `.jsonl` under the sessions tree, with its modified time.
fn rollout_files(directory: &Path) -> Vec<(std::time::SystemTime, PathBuf)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
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
            .is_some_and(|extension| extension == ROLLOUT_EXTENSION);
        if !is_rollout {
            continue;
        }

        if let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) {
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

/// Pull the session id out of one `session_meta` line - only when it records an
/// interactive session (a `codex exec` oneshot has `"source":"exec"` and cannot
/// be resumed into a TUI conversation) in the wanted working directory.
/// Field extraction over full JSON parsing: the meta line drags in the entire
/// base-instructions prompt, and two string fields don't justify a serde model
/// of Codex's private schema.
fn interactive_session_id(metadata_line: &str, wanted_cwd: &str) -> Option<String> {
    let is_oneshot =
        json_string_field(metadata_line, SOURCE_FIELD).is_some_and(|source| source == EXEC_SOURCE);
    if is_oneshot {
        return None;
    }

    let cwd = json_string_field(metadata_line, CWD_FIELD)?;
    if !same_path(&cwd.replace("\\\\", "\\"), wanted_cwd) {
        return None;
    }

    json_string_field(metadata_line, SESSION_ID_FIELD)
}

/// How long the `opencode db` lookup may take before ADE gives up and spawns a
/// fresh conversation instead. The child can stall indefinitely - a wedged
/// sqlite lock from another opencode process, or its CLI deciding to wait on
/// stdin - and a resume flag is never worth a hang.
const OPENCODE_DB_LOOKUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The newest recorded opencode session for `cwd`, read through opencode's own
/// `db` subcommand so ADE takes no sqlite dependency for one lookup. Only a
/// session that actually exists is worth a resume flag: a blind `--continue`
/// makes opencode walk its legacy session history, which currently throws a
/// user-visible "unexpected server error" on Windows (opencode #28486) - and
/// a never-prompted session records nothing, so there is nothing to reopen.
/// opencode stores `directory` with forward slashes; compare the normalized,
/// case-folded forms.
///
/// stdin is explicitly closed - with anything readable attached, `opencode db`
/// can drop into its interactive sqlite shell and wait forever - and the wait
/// is bounded: on timeout the child is killed and the session spawns fresh.
fn opencode_session_for_cwd(cwd: &str) -> Option<String> {
    let executable = program(ID_OPENCODE)?;
    let directory = cwd.replace('\\', "/").replace('\'', "''");
    let query = format!(
        "select id from session \
         where lower(replace(directory, '\\', '/')) = lower('{directory}') \
         order by time_updated desc limit 1"
    );
    let mut child = command(executable)
        .args(["db", &query])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    let (sender, receiver) = std::sync::mpsc::channel();
    let stdout = child.stdout.take()?;
    std::thread::spawn(move || {
        let mut collected = String::new();
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        while let Ok(read) = reader.read_line(&mut line) {
            if read == 0 {
                break;
            }

            collected.push_str(&line);
            line.clear();
        }

        let _ = sender.send(collected);
    });

    let output = receiver.recv_timeout(OPENCODE_DB_LOOKUP_TIMEOUT).ok();
    if output.is_none() {
        // A kill also closes the pipe, so the reader thread ends rather than
        // leaking alongside the wedged child.
        let _ = child.kill();
    }

    let _ = child.wait();
    opencode_session_id(&output?)
}

/// The session id in `opencode db`'s tab-separated output - a header line
/// (`id`) followed by at most one `ses_…` row.
fn opencode_session_id(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(OPENCODE_SESSION_PREFIX))
        .map(str::to_string)
}

/// The extra launch arguments that reopen the agent's recorded conversation -
/// `["resume", "<uuid>"]` for Codex, `["--session", "<ses_…>"]` for opencode -
/// empty when the agent has no resume mechanism ADE can drive (Claude and
/// Copilot are pinned at spawn by PADE's own id instead) or nothing is
/// recorded. The session the agent itself reported for `conversation_id` wins
/// (exact - see `running_model`); otherwise the newest one recorded in `cwd`.
// `async` so Tauri runs this on its worker pool: a synchronous command runs on
// the MAIN thread, and the opencode lookup shells out - when that child wedged
// (pre-timeout), the whole window stopped pumping messages ("Not Responding").
// The lookup itself is additionally time-bounded; async keeps even the bounded
// wait off the UI thread.
#[tauri::command]
pub async fn agent_resume_args(
    command: String,
    cwd: String,
    conversation_id: Option<String>,
) -> Vec<String> {
    let reported = conversation_id.as_deref().and_then(|conversation_id| {
        crate::running_model::resume_arguments(&command, conversation_id)
    });
    if let Some(arguments) = reported {
        return arguments;
    }

    let newest_in_cwd = match command.as_str() {
        ID_OPENCODE => opencode_session_for_cwd(&cwd),
        ID_CODEX => codex_session_for_cwd(&cwd),
        _ => None,
    };
    newest_in_cwd
        .and_then(|session_id| crate::running_model::session_resume_arguments(&command, session_id))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::interactive_session_id;
    use crate::util::json_string_field;

    const META: &str = r#"{"timestamp":"t","type":"session_meta","payload":{"session_id":"019f-abc","id":"019f-abc","cwd":"C:\\repositories\\avi\\proj","originator":"codex_exec","cli_version":"0.144.5","source":"exec"}}"#;

    #[test]
    fn a_oneshot_exec_rollout_is_never_resumed() {
        assert_eq!(
            interactive_session_id(META, "C:\\repositories\\avi\\proj"),
            None
        );
    }

    #[test]
    fn an_interactive_rollout_in_the_wanted_cwd_yields_its_session_id() {
        let interactive = META.replace("\"source\":\"exec\"", "\"source\":\"tui\"");
        assert_eq!(
            interactive_session_id(&interactive, "C:/repositories/avi/proj/"),
            Some("019f-abc".to_string())
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_wanted_cwd_matches_case_insensitively_on_windows() {
        let interactive = META.replace("\"source\":\"exec\"", "\"source\":\"tui\"");
        assert_eq!(
            interactive_session_id(&interactive, "C:/repositories/avi/PROJ"),
            Some("019f-abc".to_string())
        );
    }

    #[test]
    fn a_rollout_from_another_project_is_skipped() {
        let interactive = META.replace("\"source\":\"exec\"", "\"source\":\"tui\"");
        assert_eq!(interactive_session_id(&interactive, "C:\\elsewhere"), None);
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
    fn an_opencode_database_row_yields_its_session_id() {
        assert_eq!(
            super::opencode_session_id("id\nses_24440494dffekcRK8jTX3XCccC\n"),
            Some("ses_24440494dffekcRK8jTX3XCccC".to_string())
        );
    }

    #[test]
    fn an_opencode_database_result_without_a_row_yields_nothing() {
        // Header only (no session recorded), empty output, and an error blurb
        // must all read as "nothing to resume", never as a session id.
        assert_eq!(super::opencode_session_id("id\n"), None);
        assert_eq!(super::opencode_session_id(""), None);
        assert_eq!(super::opencode_session_id("Error: Unexpected error"), None);
    }
}
