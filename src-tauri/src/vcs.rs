//! Version-control review — Git backend.
//!
//! MVP shells out to the `git` binary the user already has (robust, no native
//! build deps). This module is the single seam behind which other backends
//! (Jujutsu, Mercurial, or a native `gix`/`git2` impl) can slot in later.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::workspace::config_dir;

const US: char = '\u{1f}'; // field separator inside a record
const RS: char = '\u{1e}'; // record separator — marks the start of a log entry

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
    /// Lines added across the commit (sum of `--numstat`).
    additions: u32,
    /// Lines deleted across the commit.
    deletions: u32,
    /// Number of files the commit touched.
    files: u32,
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
    // A record-start marker (RS) precedes each commit header so we can tell a
    // header line apart from the `--numstat` rows that follow it. The header
    // fields are US-separated as before.
    let fmt = format!("{RS}%H{US}%h{US}%s{US}%an{US}%cr");
    let raw = run_git(&[
        "log",
        &format!("-n{limit}"),
        "--numstat",
        &format!("--pretty=format:{fmt}"),
    ])?;

    let mut commits: Vec<Commit> = Vec::new();
    for line in raw.lines() {
        if let Some(header) = line.strip_prefix(RS) {
            let f: Vec<&str> = header.split(US).collect();
            let [id, short, summary, author, when] = f.as_slice() else {
                continue;
            };
            commits.push(Commit {
                id: (*id).into(),
                short: (*short).into(),
                summary: (*summary).into(),
                author: (*author).into(),
                when: (*when).into(),
                additions: 0,
                deletions: 0,
                files: 0,
            });
            continue;
        }
        // A `--numstat` row: "<adds>\t<dels>\t<path>". Fold it into the current
        // commit. Binary files show "-\t-\t<path>" — count the file, not lines.
        let Some(commit) = commits.last_mut() else {
            continue;
        };
        let Some((adds, dels)) = parse_numstat(line) else {
            continue;
        };
        commit.additions += adds;
        commit.deletions += dels;
        commit.files += 1;
    }
    Ok(commits)
}

/// Parse one `git --numstat` row into `(additions, deletions)`. A row is
/// `<adds>\t<dels>\t<path>`; binary files use `-` for both counts (→ `(0, 0)`).
/// Returns `None` for a line that isn't a numstat row (e.g. a blank separator).
fn parse_numstat(line: &str) -> Option<(u32, u32)> {
    let mut cols = line.splitn(3, '\t');
    let adds = cols.next()?;
    let dels = cols.next()?;
    // The third column (path) must exist for this to be a real numstat row.
    cols.next()?;
    let count = |c: &str| {
        if c == "-" {
            0
        } else {
            c.parse::<u32>().unwrap_or(0)
        }
    };
    Some((count(adds), count(dels)))
}

// ---------------------------------------------------------------------------
// Commit inspection — one commit's message, per-file stats, and a file's diff.
// ---------------------------------------------------------------------------

/// One file changed by a commit, with its per-file line counts.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFileEntry {
    path: String,
    /// created | modified | deleted | renamed | untracked
    kind: String,
    additions: u32,
    deletions: u32,
    /// True when git reports the file as binary (line counts are meaningless).
    binary: bool,
}

/// A single commit's full detail: message, author/date, branch, and per-file
/// stats. Reuses `Commit`'s field names for the shared header fields.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    id: String,
    short: String,
    summary: String,
    /// The commit message body (everything after the subject line); empty if none.
    body: String,
    author: String,
    when: String,
    /// The current HEAD branch name (empty on a detached HEAD).
    branch: String,
    files: Vec<CommitFileEntry>,
    additions: u32,
    deletions: u32,
}

/// Map a `--name-status` status letter to a wire status-kind string. Git uses a
/// trailing similarity score for renames/copies (`R100`), so we match the head.
fn status_letter_kind(code: &str) -> StatusKind {
    match code.chars().next() {
        Some('A') => StatusKind::Created,
        Some('D') => StatusKind::Deleted,
        Some('R' | 'C') => StatusKind::Renamed,
        _ => StatusKind::Modified,
    }
}

#[tauri::command]
pub fn vcs_commit(sha: String) -> Result<CommitDetail, String> {
    // Header + full body in one shot: subject on its own line, then the body.
    let fmt = format!("%H{US}%h{US}%s{US}%an{US}%cr{US}%b");
    let head = run_git(&["show", "-s", &format!("--format={fmt}"), &sha])?;
    let f: Vec<&str> = head.trim_end_matches('\n').splitn(6, US).collect();
    let [id, short, summary, author, when, body] = f.as_slice() else {
        return Err("could not parse commit header".into());
    };

    // Both listings use `-z` so a renamed path arrives as its own NUL-separated
    // field (git otherwise packs it into "src/{old => new}/f.txt" brace notation,
    // which would poison the stored path and break vcs_commit_diff). --numstat's
    // record is "adds\tdels\tpath\0" normally, or "adds\tdels\t\0old\0new\0" for a
    // rename; --name-status is "code\0path\0" or "code\0old\0new\0".
    let numstat = run_git(&["show", "--numstat", "-z", "--format=", &sha])?;
    let namestat = run_git(&["show", "--name-status", "-z", "--format=", &sha])?;

    let kinds_by_path = status_kinds_by_path(&namestat);

    let mut files: Vec<CommitFileEntry> = Vec::new();
    let mut additions = 0u32;
    let mut deletions = 0u32;

    for record in parse_numstat_records(&numstat) {
        let kind = kinds_by_path
            .get(record.path)
            .copied()
            .unwrap_or(StatusKind::Modified);
        additions += record.additions;
        deletions += record.deletions;
        files.push(CommitFileEntry {
            path: record.path.to_string(),
            kind: kind.as_str().to_string(),
            additions: record.additions,
            deletions: record.deletions,
            binary: record.binary,
        });
    }

    let branch = current_branch().unwrap_or_default();

    Ok(CommitDetail {
        id: (*id).into(),
        short: (*short).into(),
        summary: (*summary).into(),
        body: body.trim().to_string(),
        author: (*author).into(),
        when: (*when).into(),
        branch,
        files,
        additions,
        deletions,
    })
}

/// One file's line counts from a `--numstat -z` record, keyed by the path that
/// git reports (the NEW path for a rename).
struct NumstatRecord<'input> {
    path: &'input str,
    additions: u32,
    deletions: u32,
    /// True when git reports the file as binary (`-` for both counts).
    binary: bool,
}

/// Parse a `git show --numstat -z` stream into per-file records. Fields are
/// NUL-separated: a normal record is one `"adds\tdels\tpath"` field; a rename is
/// `"adds\tdels\t"` followed by the old and new path in their own fields — we
/// take the NEW path so it matches the diff and name-status keys.
fn parse_numstat_records(numstat: &str) -> Vec<NumstatRecord<'_>> {
    let mut records = Vec::new();
    let mut fields = numstat.split('\0').filter(|field| !field.is_empty());

    while let Some(head) = fields.next() {
        let mut columns = head.splitn(3, '\t');
        let (Some(adds), Some(dels), Some(path_column)) =
            (columns.next(), columns.next(), columns.next())
        else {
            continue;
        };

        // An empty path column marks a rename: the old and new paths follow as
        // their own NUL fields. Take the new path (the second of the two).
        let path = if path_column.is_empty() {
            let Some(_old_path) = fields.next() else {
                break;
            };
            let Some(new_path) = fields.next() else {
                break;
            };
            new_path
        } else {
            path_column
        };

        let binary = adds == "-" || dels == "-";
        records.push(NumstatRecord {
            path,
            additions: adds.parse::<u32>().unwrap_or(0),
            deletions: dels.parse::<u32>().unwrap_or(0),
            binary,
        });
    }
    records
}

/// Parse a `git show --name-status -z` stream into a `new path → StatusKind` map.
/// Fields are NUL-separated: a normal record is `"code"` then `"path"`; a rename
/// or copy is `"code"` then the old and new path — we key on the NEW path so it
/// matches the path `--numstat` reports.
fn status_kinds_by_path(namestat: &str) -> HashMap<&str, StatusKind> {
    let mut kinds = HashMap::new();
    let mut fields = namestat.split('\0').filter(|field| !field.is_empty());

    while let Some(code) = fields.next() {
        let kind = status_letter_kind(code);
        let is_rename_or_copy = matches!(code.chars().next(), Some('R' | 'C'));
        if is_rename_or_copy {
            let Some(_old_path) = fields.next() else {
                break;
            };
            let Some(new_path) = fields.next() else {
                break;
            };
            kinds.insert(new_path, kind);
        } else {
            let Some(path) = fields.next() else {
                break;
            };
            kinds.insert(path, kind);
        }
    }
    kinds
}

/// Raw unified diff for one path within a commit.
#[tauri::command]
pub fn vcs_commit_diff(sha: String, path: String) -> Result<String, String> {
    run_git(&["show", "--no-color", &sha, "--", &path])
}

/// The `origin` remote URL, normalized to a browsable `https://host/owner/repo`
/// form. `None` when there's no `origin` remote.
#[tauri::command]
pub fn vcs_remote_url() -> Option<String> {
    let raw = run_git(&["remote", "get-url", "origin"]).ok()?;
    let url = raw.trim();
    if url.is_empty() {
        return None;
    }
    Some(normalize_remote(url))
}

/// Normalize a git remote to an `https://host/owner/repo` browse URL:
///  - `git@github.com:owner/repo.git` → `https://github.com/owner/repo`
///  - `ssh://git@host/owner/repo.git` → `https://host/owner/repo`
///  - an `https://…/repo.git` just loses its `.git` suffix.
fn normalize_remote(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    // strip_suffix removes a single ".git"; trim_end_matches would peel repeated
    // suffixes (e.g. "repo.git.git" → "repo"), mangling a legitimate path.
    let stripped = trimmed.strip_suffix(".git").unwrap_or(trimmed);

    // scp-like syntax: `git@host:owner/repo`.
    if let Some(rest) = stripped.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return format!("https://{host}/{path}");
        }
    }
    // `ssh://git@host/owner/repo` or `git://host/owner/repo`.
    for prefix in ["ssh://git@", "ssh://", "git://"] {
        if let Some(rest) = stripped.strip_prefix(prefix) {
            return format!("https://{rest}");
        }
    }
    stripped.to_string()
}

/// The current HEAD branch name (`None` on a detached HEAD or non-repo).
#[tauri::command]
pub fn vcs_current_branch() -> Option<String> {
    current_branch()
}

/// Shared HEAD-branch resolver used by both `vcs_current_branch` and
/// `vcs_commit`. `None`/detached maps to no branch.
fn current_branch() -> Option<String> {
    let raw = run_git(&["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;
    let name = raw.trim();
    if name.is_empty() || name == "HEAD" {
        return None;
    }
    Some(name.to_string())
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
        .map(str::to_lowercase)
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
        .take_while(char::is_ascii_digit)
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
    let raw = run_git(&["log", &format!("-n{n}"), &format!("--pretty=format:{fmt}")])?;

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
            // Token counts are tiny (a short query vs a commit subject), so the
            // usize→f32 conversion can't actually lose precision here.
            #[allow(clippy::cast_precision_loss)]
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
    let short = run_git(&["rev-parse", "--short", &sha])?.trim().to_string();
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
