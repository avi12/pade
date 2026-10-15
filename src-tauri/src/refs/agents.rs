//! Each agent's memory of a project, re-pointed when the project's folder moves,
//! so its history, trust and settings follow the folder:
//!
//!   * Claude Code - transcripts and auto-memory under
//!     `~/.claude/projects/<encoded cwd>` (one folder for the workspace and one
//!     for every folder inside it a session ran in), and its per-project
//!     settings and trust in `~/.claude.json`;
//!   * Codex - its thread index (`~/.codex/state_*.sqlite`, by cwd) and project
//!     trust in `~/.codex/config.toml`;
//!   * Copilot - each session's `workspace.yaml` and the trusted folders in
//!     `~/.copilot/config.json`;
//!   * `OpenCode` - sessions, projects and workspaces in its database;
//!   * Gemini - its trusted folders and project names.
//!
//! Only entries for the moved folder or something inside it change; a sibling
//! whose name merely starts the same is left alone (see `rebase_paths_in`).
//! Measured against Claude Code 2.1.292, Codex 0.161, Copilot 1.0.34 and
//! `OpenCode` 1.18.33.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use super::{moved_path, rewrite_file_forms};
use crate::util::{encode_project, home_dir};

/// Codex's tables that record a folder: its threads and its projects' roots.
const CODEX_PATH_COLUMNS: [(&str, &str); 2] = [("threads", "cwd"), ("project_roots", "path")];
/// `OpenCode`'s tables that record a folder.
const OPENCODE_PATH_COLUMNS: [(&str, &str); 4] = [
    ("session", "directory"),
    ("project", "worktree"),
    ("project_directory", "directory"),
    ("workspace", "directory"),
];
/// How long a database write waits for an agent that is writing it too.
const DATABASE_BUSY_WAIT: Duration = Duration::from_secs(2);
/// How far into a transcript its first `cwd` is looked for.
const TRANSCRIPT_HEAD_LINES: usize = 50;

/// Re-point every agent's memory of `old` (and of the folders inside it) to
/// `new`. Best-effort per agent: one that isn't installed, or whose file is
/// locked, never stops the others.
pub(super) fn relocate_memories(old: &str, new: &str) {
    let Some(home) = home_dir() else { return };

    move_claude_transcripts(&home.join(".claude").join("projects"), old, new);
    rewrite_file_forms(&home.join(".claude.json"), old, new);

    if let Some(codex) = crate::agents::codex_home() {
        for database in codex_state_databases(&codex) {
            rebase_database_columns(&database, &CODEX_PATH_COLUMNS, old, new);
        }
        rewrite_file_forms(&codex.join("config.toml"), old, new);
    }

    let copilot = home.join(".copilot");
    rewrite_copilot_sessions(&copilot.join("session-state"), old, new);
    rewrite_file_forms(&copilot.join("config.json"), old, new);

    let opencode_database = home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    rebase_database_columns(&opencode_database, &OPENCODE_PATH_COLUMNS, old, new);

    for file in ["trustedFolders.json", "projects.json"] {
        rewrite_file_forms(&home.join(".gemini").join(file), old, new);
    }
}

/// Move each Claude project folder kept for `old` or a folder inside it to the
/// name Claude gives the new path. Which folder a project folder speaks for is
/// read off its transcripts' `cwd`, since the encoding can't be reversed
/// (`pade-effort` and `pade\effort` encode alike).
fn move_claude_transcripts(projects: &Path, old: &str, new: &str) {
    let encoded_old = encode_project(old);
    let inside_prefix = format!("{encoded_old}-");
    let Ok(entries) = std::fs::read_dir(projects) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_the_folder = name == encoded_old;
        let may_be_inside = name.starts_with(&inside_prefix);
        if !is_the_folder && !may_be_inside {
            continue;
        }
        // The workspace's own folder may hold only auto-memory, no transcript.
        let cwd = if is_the_folder {
            Some(old.to_string())
        } else {
            transcript_cwd(&entry.path())
        };
        let Some(moved_cwd) = cwd.and_then(|cwd| moved_path(&cwd, old, new)) else {
            continue;
        };
        move_into(&entry.path(), &projects.join(encode_project(&moved_cwd)));
    }
}

/// The working directory the first transcript in a Claude project folder names.
fn transcript_cwd(project: &Path) -> Option<String> {
    let entries = std::fs::read_dir(project).ok()?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .find_map(|transcript| first_cwd(&transcript))
}

/// The one field read from a transcript line. Deserializing only this skips
/// the rest of the line instead of building a whole JSON tree for it, which
/// matters because a transcript line can carry an entire tool output.
#[derive(Deserialize)]
struct TranscriptLine {
    cwd: Option<String>,
}

fn first_cwd(transcript: &Path) -> Option<String> {
    let file = std::fs::File::open(transcript).ok()?;
    BufReader::new(file)
        .lines()
        .take(TRANSCRIPT_HEAD_LINES)
        .map_while(Result::ok)
        .find_map(|line| serde_json::from_str::<TranscriptLine>(&line).ok()?.cwd)
}

/// Move `from` to `to`; when `to` already exists (the name was used before),
/// move in each entry it doesn't have yet, so no history is lost or overwritten.
fn move_into(from: &Path, to: &Path) {
    if !to.exists() {
        if let Err(error) = std::fs::rename(from, to) {
            eprintln!(
                "refs: failed to move {} → {}: {error}",
                from.display(),
                to.display()
            );
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    for entry in entries.flatten() {
        let target = to.join(entry.file_name());
        if target.exists() {
            eprintln!(
                "refs: kept {} - {} exists",
                entry.path().display(),
                target.display()
            );
            continue;
        }
        if let Err(error) = std::fs::rename(entry.path(), &target) {
            eprintln!("refs: failed to move {}: {error}", entry.path().display());
        }
    }
    // Only goes when every entry moved; otherwise what stayed is kept.
    let _ = std::fs::remove_dir(from);
}

/// Codex's state databases (`state_<version>.sqlite`), whichever versions exist.
fn codex_state_databases(codex: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(codex) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            name.starts_with("state_")
                && path
                    .extension()
                    .is_some_and(|extension| extension == "sqlite")
        })
        .collect()
}

/// Each Copilot session records its working folder in its `workspace.yaml`.
fn rewrite_copilot_sessions(session_state: &Path, old: &str, new: &str) {
    let Ok(sessions) = std::fs::read_dir(session_state) else {
        return;
    };
    for session in sessions.flatten() {
        rewrite_file_forms(&session.path().join("workspace.yaml"), old, new);
    }
}

/// Re-point every folder `columns` of `database` record under `old`.
fn rebase_database_columns(database: &Path, columns: &[(&str, &str)], old: &str, new: &str) {
    if !database.is_file() {
        return;
    }
    let result = rusqlite::Connection::open(database).and_then(|mut connection| {
        connection.busy_timeout(DATABASE_BUSY_WAIT)?;
        let transaction = connection.transaction()?;
        for (table, column) in columns {
            rebase_column(&transaction, table, column, old, new);
        }
        transaction.commit()
    });
    if let Err(error) = result {
        eprintln!("refs: failed to update {}: {error}", database.display());
    }
}

/// One column's rows under `old`, re-pointed. A table this version of the agent
/// doesn't have is skipped; a row whose new value would clash is kept.
fn rebase_column(
    connection: &rusqlite::Connection,
    table: &str,
    column: &str,
    old: &str,
    new: &str,
) {
    let query = format!("SELECT rowid, {column} FROM {table} WHERE {column} IS NOT NULL");
    let Ok(mut statement) = connection.prepare(&query) else {
        return;
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    }) else {
        return;
    };
    let moved: Vec<(i64, String)> = rows
        .flatten()
        .filter_map(|(row_id, value)| moved_path(&value, old, new).map(|moved| (row_id, moved)))
        .collect();

    let update = format!("UPDATE {table} SET {column} = ?1 WHERE rowid = ?2");
    for (row_id, value) in moved {
        if let Err(error) = connection.execute(&update, rusqlite::params![value, row_id]) {
            eprintln!("refs: kept {table}.{column} row {row_id}: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{first_cwd, move_claude_transcripts, rebase_database_columns};
    use crate::util::encode_project;

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!("pade-agent-memory-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("scratch directory");
        directory
    }

    #[test]
    fn claude_folders_inside_the_workspace_move_and_a_sibling_stays() {
        let projects = scratch("claude");
        let old = r"C:\work\pade";
        let new = r"C:\work\editor";
        let transcript = |cwd: &str| format!("{}\n", serde_json::json!({ "cwd": cwd }));
        let own = projects.join(encode_project(old));
        let inside = projects.join(encode_project(r"C:\work\pade\app"));
        let sibling = projects.join(encode_project(r"C:\work\pade-effort"));
        for folder in [&own, &inside, &sibling] {
            std::fs::create_dir_all(folder).expect("project folder");
        }
        std::fs::create_dir_all(own.join("memory")).expect("memory folder");
        std::fs::write(inside.join("a.jsonl"), transcript(r"C:\work\pade\app")).expect("write");
        std::fs::write(sibling.join("b.jsonl"), transcript(r"C:\work\pade-effort")).expect("write");

        move_claude_transcripts(&projects, old, new);

        assert!(projects.join(encode_project(new)).join("memory").is_dir());
        assert!(projects
            .join(encode_project(r"C:\work\editor\app"))
            .join("a.jsonl")
            .is_file());
        assert!(sibling.join("b.jsonl").is_file());
    }

    #[test]
    fn the_first_line_naming_a_cwd_wins() {
        let folder = scratch("first-cwd");
        let transcript = folder.join("session.jsonl");
        let lines = [
            "not json".to_string(),
            serde_json::json!({ "type": "summary", "cwd": 7 }).to_string(),
            serde_json::json!({ "type": "summary" }).to_string(),
            serde_json::json!({ "cwd": r"C:\work\pade", "message": { "content": [1, 2] } })
                .to_string(),
            serde_json::json!({ "cwd": r"C:\work\later" }).to_string(),
        ];
        std::fs::write(&transcript, lines.join("\n")).expect("write transcript");

        assert_eq!(first_cwd(&transcript).as_deref(), Some(r"C:\work\pade"));
    }

    #[test]
    fn database_rows_under_the_folder_are_re_pointed() {
        let database = scratch("database").join("state.sqlite");
        let connection = rusqlite::Connection::open(&database).expect("database");
        connection
            .execute_batch(
                r"CREATE TABLE threads (id TEXT, cwd TEXT);
                  INSERT INTO threads VALUES ('a', '\\?\C:\work\pade\src'), ('b', 'C:\work\pade-effort');",
            )
            .expect("seed");
        drop(connection);

        rebase_database_columns(
            &database,
            &[("threads", "cwd"), ("missing", "path")],
            r"C:\work\pade",
            r"C:\work\editor",
        );

        let connection = rusqlite::Connection::open(&database).expect("database");
        let cwd = |id: &str| -> String {
            connection
                .query_row("SELECT cwd FROM threads WHERE id = ?1", [id], |row| {
                    row.get(0)
                })
                .expect("row")
        };
        assert_eq!(cwd("a"), r"\\?\C:\work\editor\src");
        assert_eq!(cwd("b"), r"C:\work\pade-effort");
    }
}
