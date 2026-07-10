//! Version-control review — Git backend.
//!
//! MVP shells out to the `git` binary the user already has (robust, no native
//! build deps). This module is the single seam behind which other backends
//! (Jujutsu, Mercurial, or a native `gix`/`git2` impl) can slot in later.

use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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

/// How a working-tree path changed, in the exact wire strings the frontend reads.
/// One authoritative home for the status-kind literals.
#[derive(Clone, Copy)]
enum StatusKind {
    Created,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

impl StatusKind {
    /// The serialized string for this kind — the only place the literals live.
    fn as_str(self) -> &'static str {
        match self {
            StatusKind::Created => "created",
            StatusKind::Modified => "modified",
            StatusKind::Deleted => "deleted",
            StatusKind::Renamed => "renamed",
            StatusKind::Untracked => "untracked",
        }
    }
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

fn classify(index: char, worktree: char) -> (StatusKind, bool) {
    // Untracked files show as "??".
    if index == '?' {
        return (StatusKind::Untracked, false);
    }
    let staged = index != ' ';
    let code = if staged { index } else { worktree };
    let kind = match code {
        'A' => StatusKind::Created,
        'D' => StatusKind::Deleted,
        'R' => StatusKind::Renamed,
        _ => StatusKind::Modified,
    };
    (kind, staged)
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
        let is_rename = index == 'R' || worktree == 'R';
        if is_rename {
            records.next();
        }

        let (kind, staged) = classify(index, worktree);
        entries.push(StatusEntry {
            path,
            kind: kind.as_str().to_string(),
            staged,
        });
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

// ---------------------------------------------------------------------------
// Restore a version — natural-language → commit.
// ---------------------------------------------------------------------------

/// A commit ranked against a natural-language restore query.
/// Mirrors `Commit`'s wire shape (camelCase) plus a relevance `score`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreCandidate {
    id: String,
    short: String,
    summary: String,
    author: String,
    when: String,
    score: f32,
}

const DEFAULT_CANDIDATE_LIMIT: u32 = 50;
/// Boost added to a commit whose committer time falls inside a query's time hint.
const TIME_HINT_BOOST: f32 = 0.5;

/// Seconds in a day — the unit for relative time-hint windows.
const DAY_SECS: u64 = 86_400;

/// Current unix time in seconds (0 if the clock predates the epoch).
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Split a string into lowercase alphanumeric-ish tokens (whitespace-separated).
fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|t| t.to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

/// An inclusive `[start, end]` unix-time window a commit's `%ct` can fall into.
struct TimeWindow {
    start: u64,
    end: u64,
}

impl TimeWindow {
    fn contains(&self, ts: u64) -> bool {
        ts >= self.start && ts <= self.end
    }
}

/// Derive a unix-time window from natural-language time hints in `query`, if any.
/// A small, dependency-free hint table; `now` is the reference "now".
fn time_hint_window(query: &str, now: u64) -> Option<TimeWindow> {
    let q = query.to_lowercase();

    // "N days ago" — window is that whole calendar-ish day (± around the offset).
    if let Some(days) = parse_days_ago(&q) {
        let offset = days.saturating_mul(DAY_SECS);
        let center = now.saturating_sub(offset);
        return Some(TimeWindow {
            start: center.saturating_sub(DAY_SECS),
            end: center,
        });
    }

    let contains_today = q.contains("today");
    if contains_today {
        return Some(TimeWindow {
            start: now.saturating_sub(DAY_SECS),
            end: now,
        });
    }

    let contains_yesterday = q.contains("yesterday");
    if contains_yesterday {
        return Some(TimeWindow {
            start: now.saturating_sub(2 * DAY_SECS),
            end: now.saturating_sub(DAY_SECS),
        });
    }

    let contains_last_week = q.contains("last week");
    if contains_last_week {
        return Some(TimeWindow {
            start: now.saturating_sub(7 * DAY_SECS),
            end: now,
        });
    }

    None
}

/// Parse a leading "N days ago" pattern out of an already-lowercased query.
/// Returns the number of days, or None when the phrase isn't present.
fn parse_days_ago(query: &str) -> Option<u64> {
    let idx = query.find("days ago").or_else(|| query.find("day ago"))?;
    // Walk back over the digits immediately preceding the phrase.
    let head = query[..idx].trim_end();
    let digits: String = head
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.parse::<u64>().ok()
}

/// Rank recent commits against a natural-language `query`.
///
/// Scores each commit by the fraction of query tokens that appear as substrings
/// of its subject, plus a time-hint boost (e.g. "yesterday", "3 days ago").
/// Returns all candidates, best score first, ties keeping git-log recency order.
#[tauri::command]
pub fn vcs_restore_candidates(
    query: String,
    limit: Option<u32>,
) -> Result<Vec<RestoreCandidate>, String> {
    let n = limit.unwrap_or(DEFAULT_CANDIDATE_LIMIT);
    // %ct is the committer unix timestamp, used for time-hint scoring.
    let fmt = format!("%H{US}%h{US}%s{US}%an{US}%cr{US}%ct");
    let raw = run_git(&[
        "log",
        &format!("-n{n}"),
        &format!("--pretty=format:{fmt}"),
    ])?;

    let query_tokens = tokenize(&query);
    let now = now_unix();
    let window = time_hint_window(&query, now);

    let mut candidates: Vec<RestoreCandidate> = raw
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(US).collect();
            let [id, short, summary, author, when, ct] = f.as_slice() else {
                return None;
            };

            let subject = summary.to_lowercase();
            let matched = query_tokens
                .iter()
                .filter(|tok| subject.contains(tok.as_str()))
                .count();
            let mut score = if query_tokens.is_empty() {
                0.0
            } else {
                matched as f32 / query_tokens.len() as f32
            };

            // Time-hint boost: reward commits committed inside the hinted window.
            let in_time_window = window
                .as_ref()
                .zip(ct.parse::<u64>().ok())
                .is_some_and(|(win, ts)| win.contains(ts));
            if in_time_window {
                score += TIME_HINT_BOOST;
            }

            Some(RestoreCandidate {
                id: (*id).into(),
                short: (*short).into(),
                summary: (*summary).into(),
                author: (*author).into(),
                when: (*when).into(),
                score,
            })
        })
        .collect();

    // Stable sort by score desc; equal scores keep git-log (recency) order.
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(candidates)
}

/// Restore a commit non-destructively by switching to a dedicated branch.
///
/// Creates (or re-enters) `pade/restore-<shortsha>` pointing at `sha` — never
/// touches the working tree with `reset --hard`. Returns the branch name.
#[tauri::command]
pub fn vcs_restore_checkout(sha: String) -> Result<String, String> {
    let short = run_git(&["rev-parse", "--short", &sha])?
        .trim()
        .to_string();
    let branch = format!("pade/restore-{short}");

    // Does the restore branch already exist? Then just switch to it.
    let branch_exists = run_git(&["rev-parse", "--verify", "--quiet", &branch]).is_ok();
    if branch_exists {
        run_git(&["switch", &branch])?;
    } else {
        // Create the branch at the target commit and switch to it in one step.
        // Any error (e.g. a dirty working tree) surfaces git's stderr verbatim.
        run_git(&["switch", "-c", &branch, &sha])?;
    }
    Ok(branch)
}

// A future git-bisect pair (`vcs_bisect_start` / `vcs_bisect_step`) slots in
// here behind `run_git`: start would `git bisect start <bad> <good>`, and step
// would mark the current revision good/bad and report the next one to test.
// Not implemented yet.
