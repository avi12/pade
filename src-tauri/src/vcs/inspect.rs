//! Commit inspection — one commit's message, per-file stats, and a file's diff.

use std::collections::HashMap;

use serde::Serialize;

use super::branches::current_branch;
use super::{run_git, StatusKind, US};

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
    StatusKind::from_git_letter(code.chars().next().unwrap_or(' '))
}

#[tauri::command]
pub async fn vcs_commit(cwd: String, sha: String) -> Result<CommitDetail, String> {
    // Header + full body in one shot: subject on its own line, then the body.
    let fmt = format!("%H{US}%h{US}%s{US}%an{US}%cr{US}%b");
    let head = run_git(&cwd, &["show", "-s", &format!("--format={fmt}"), &sha])?;
    let f: Vec<&str> = head.trim_end_matches('\n').splitn(6, US).collect();
    let [id, short, summary, author, when, body] = f.as_slice() else {
        return Err("could not parse commit header".into());
    };

    // Both listings use `-z` so a renamed path arrives as its own NUL-separated
    // field (git otherwise packs it into "src/{old => new}/f.txt" brace notation,
    // which would poison the stored path and break vcs_commit_diff). --numstat's
    // record is "adds\tdels\tpath\0" normally, or "adds\tdels\t\0old\0new\0" for a
    // rename; --name-status is "code\0path\0" or "code\0old\0new\0".
    let numstat = run_git(&cwd, &["show", "--numstat", "-z", "--format=", &sha])?;
    let namestat = run_git(&cwd, &["show", "--name-status", "-z", "--format=", &sha])?;

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

    let branch = current_branch(&cwd).unwrap_or_default();

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
pub async fn vcs_commit_diff(cwd: String, sha: String, path: String) -> Result<String, String> {
    run_git(&cwd, &["show", "--no-color", &sha, "--", &path])
}

#[cfg(test)]
mod tests {
    use super::{parse_numstat_records, status_kinds_by_path, status_letter_kind};

    #[test]
    fn a_plain_numstat_record_keeps_its_path_and_counts() {
        let records = parse_numstat_records("3\t2\tsrc/app.ts\0");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].path, "src/app.ts");
        assert_eq!(records[0].additions, 3);
        assert_eq!(records[0].deletions, 2);
        assert!(!records[0].binary);
    }

    #[test]
    fn a_rename_record_takes_the_new_path() {
        let records = parse_numstat_records("1\t1\t\0src/old.ts\0src/new.ts\0");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].path, "src/new.ts");
    }

    #[test]
    fn a_binary_record_is_marked_binary_with_zero_counts() {
        let records = parse_numstat_records("-\t-\tassets/logo.png\0");
        assert!(records[0].binary);
        assert_eq!(records[0].additions, 0);
        assert_eq!(records[0].deletions, 0);
    }

    #[test]
    fn name_status_keys_a_rename_on_the_new_path() {
        let kinds = status_kinds_by_path("M\0src/app.ts\0R100\0src/old.ts\0src/new.ts\0");
        assert_eq!(
            kinds.get("src/app.ts").map(|k| k.as_str()),
            Some("modified")
        );
        assert_eq!(kinds.get("src/new.ts").map(|k| k.as_str()), Some("renamed"));
        assert!(!kinds.contains_key("src/old.ts"));
    }

    #[test]
    fn status_letters_map_to_wire_kinds() {
        assert_eq!(status_letter_kind("A").as_str(), "created");
        assert_eq!(status_letter_kind("D").as_str(), "deleted");
        assert_eq!(status_letter_kind("R87").as_str(), "renamed");
        assert_eq!(status_letter_kind("C50").as_str(), "renamed");
        assert_eq!(status_letter_kind("M").as_str(), "modified");
    }
}
