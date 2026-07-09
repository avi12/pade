//! Version-control review — Git backend.
//!
//! MVP shells out to the `git` binary the user already has (robust, no native
//! build deps). This module is the single seam behind which other backends
//! (Jujutsu, Mercurial, or a native `gix`/`git2` impl) can slot in later.

use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::workspace::config_dir;

const US: char = '\u{1f}'; // field separator inside a record

fn run_git(args: &[&str]) -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// A single changed path in the working tree.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEntry {
    path: String,
    /// created | modified | deleted | renamed | untracked
    kind: String,
    /// Staged in the index (agent commits land here after "approve").
    staged: bool,
}

/// A recent commit for the Log view.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    id: String,
    short: String,
    summary: String,
    author: String,
    when: String,
}

fn classify(index: char, worktree: char) -> (String, bool) {
    // Untracked files show as "??".
    if index == '?' {
        return ("untracked".into(), false);
    }
    let staged = index != ' ';
    let code = if staged { index } else { worktree };
    let kind = match code {
        'A' => "created",
        'D' => "deleted",
        'R' => "renamed",
        _ => "modified",
    };
    (kind.into(), staged)
}

#[tauri::command]
pub fn vcs_status() -> Result<Vec<StatusEntry>, String> {
    // NUL-delimited records survive paths with spaces/newlines.
    let raw = run_git(&["status", "--porcelain=v1", "-z"])?;
    let mut entries = Vec::new();
    let mut records = raw.split('\0');

    while let Some(rec) = records.next() {
        if rec.len() < 3 {
            continue;
        }
        let bytes: Vec<char> = rec.chars().collect();
        let index = bytes[0];
        let worktree = bytes[1];
        let path: String = rec[3..].to_string();

        // A rename record is followed by the old path in the next field; drop it.
        if index == 'R' || worktree == 'R' {
            records.next();
        }

        let (kind, staged) = classify(index, worktree);
        entries.push(StatusEntry { path, kind, staged });
    }
    Ok(entries)
}

#[tauri::command]
pub fn vcs_log(limit: u32) -> Result<Vec<Commit>, String> {
    let fmt = format!("%H{US}%h{US}%s{US}%an{US}%cr");
    let raw = run_git(&[
        "log",
        &format!("-n{limit}"),
        &format!("--pretty=format:{fmt}"),
    ])?;

    let commits = raw
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(US).collect();
            match f.as_slice() {
                [id, short, summary, author, when] => Some(Commit {
                    id: (*id).into(),
                    short: (*short).into(),
                    summary: (*summary).into(),
                    author: (*author).into(),
                    when: (*when).into(),
                }),
                _ => None,
            }
        })
        .collect();
    Ok(commits)
}

/// Local branches in the current repo (empty/Err when not a git repo).
#[tauri::command]
pub fn vcs_branches() -> Result<Vec<String>, String> {
    let raw = run_git(&["branch", "--format=%(refname:short)"])?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

/// Add a git worktree for `branch` so an agent can work it in isolation, and
/// return the worktree path. Reuses an existing worktree if already created.
/// `create` makes a new branch (`git worktree add -b`) off the current HEAD.
#[tauri::command]
pub fn vcs_worktree_add(branch: String, create: bool) -> Result<String, String> {
    let root = run_git(&["rev-parse", "--show-toplevel"])?
        .trim()
        .to_string();
    let repo = Path::new(&root)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");
    // Branches can contain '/', which would nest dirs — flatten for the folder.
    let safe = branch.replace(['/', '\\'], "-");
    let dir = config_dir()?.join("worktrees").join(repo).join(&safe);
    let dir_str = dir.to_string_lossy().into_owned();

    if dir.exists() {
        return Ok(dir_str);
    }

    if create {
        run_git(&["worktree", "add", "-b", &branch, &dir_str])?;
    } else {
        run_git(&["worktree", "add", &dir_str, &branch])?;
    }
    Ok(dir_str)
}

/// Raw unified diff for one path (staged or working-tree).
#[tauri::command]
pub fn vcs_diff(path: String, staged: bool) -> Result<String, String> {
    let mut args = vec!["diff", "--no-color"];
    if staged {
        args.push("--staged");
    }
    args.push("--");
    args.push(&path);
    run_git(&args)
}
