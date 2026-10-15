//! Commit inspection - one commit's message, per-file stats, and a file's diff.

use std::collections::HashMap;

use serde::Serialize;

use super::branches::current_branch;
use super::{
    commit_header_format, run_git, validate_object_id, CommitHeader, NumstatRow, StatusKind,
    FIELD_SEPARATOR,
};

/// One file changed by a commit, with its per-file line counts.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFileEntry {
    path: String,
    kind: StatusKind,
    additions: u32,
    deletions: u32,
    /// True when git reports the file as binary (line counts are meaningless).
    binary: bool,
}

/// A single commit's full detail: message, author/date, branch, and per-file
/// stats. Shares the Log view's header fields through [`CommitHeader`].
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    #[serde(flatten)]
    header: CommitHeader,
    /// The commit message body (everything after the subject line); empty if none.
    body: String,
    /// The current HEAD branch name (empty on a detached HEAD).
    branch: String,
    files: Vec<CommitFileEntry>,
    additions: u32,
    deletions: u32,
}

#[tauri::command]
pub async fn vcs_commit(cwd: String, sha: String) -> Result<CommitDetail, String> {
    let sha = validate_object_id(&sha)?;
    // Header + full body in one shot: subject on its own line, then the body.
    let format = format!("--format={}{FIELD_SEPARATOR}%b", commit_header_format());
    let head = run_git(&cwd, &["show", "-s", &format, "--end-of-options", sha])?;
    let fields: Vec<&str> = head
        .trim_end_matches('\n')
        .splitn(6, FIELD_SEPARATOR)
        .collect();
    let Some((header, [body])) = CommitHeader::split_off(&fields) else {
        return Err("could not parse commit header".into());
    };

    // Both listings use `-z` so a renamed path arrives as its own NUL-separated
    // field (git otherwise packs it into "src/{old => new}/f.txt" brace notation,
    // which would poison the stored path and break vcs_commit_diff). --numstat's
    // record is "adds\tdels\tpath\0" normally, or "adds\tdels\t\0old\0new\0" for a
    // rename; --name-status is "code\0path\0" or "code\0old\0new\0".
    let show_listing = |listing: &str| {
        run_git(
            &cwd,
            &["show", listing, "-z", "--format=", "--end-of-options", sha],
        )
    };
    let numstat = show_listing("--numstat")?;
    let name_status = show_listing("--name-status")?;
    let kinds_by_path = status_kinds_by_path(&name_status);

    let files: Vec<CommitFileEntry> = parse_numstat_records(&numstat)
        .into_iter()
        .map(|record| CommitFileEntry {
            path: record.path.to_string(),
            kind: kinds_by_path
                .get(record.path)
                .copied()
                .unwrap_or(StatusKind::Modified),
            additions: record.additions,
            deletions: record.deletions,
            binary: record.is_binary,
        })
        .collect();

    Ok(CommitDetail {
        header,
        body: body.trim().to_string(),
        branch: current_branch(&cwd).unwrap_or_default(),
        additions: files.iter().map(|file| file.additions).sum(),
        deletions: files.iter().map(|file| file.deletions).sum(),
        files,
    })
}

/// Skip a `-z` rename's old path and return its new one - the path both
/// `--numstat` and `--name-status` key a renamed file on.
fn new_path_of_rename<'text>(fields: &mut impl Iterator<Item = &'text str>) -> Option<&'text str> {
    fields.next()?;
    fields.next()
}

/// Parse a `git show --numstat -z` stream into per-file rows. Fields are
/// NUL-separated: a normal record is one `"adds\tdels\tpath"` field; a rename is
/// `"adds\tdels\t"` followed by the old and new path in their own fields - we
/// take the NEW path so it matches the diff and name-status keys.
fn parse_numstat_records(numstat: &str) -> Vec<NumstatRow<'_>> {
    let mut records = Vec::new();
    let mut fields = numstat.split('\0').filter(|field| !field.is_empty());

    while let Some(head) = fields.next() {
        let Some(mut row) = NumstatRow::parse(head) else {
            continue;
        };
        let is_rename = row.path.is_empty();
        if is_rename {
            let Some(new_path) = new_path_of_rename(&mut fields) else {
                break;
            };
            row.path = new_path;
        }
        records.push(row);
    }
    records
}

/// Parse a `git show --name-status -z` stream into a `new path → StatusKind` map.
/// Fields are NUL-separated: a normal record is `"code"` then `"path"`; a rename
/// or copy is `"code"` then the old and new path - we key on the NEW path so it
/// matches the path `--numstat` reports.
fn status_kinds_by_path(name_status: &str) -> HashMap<&str, StatusKind> {
    let mut kinds = HashMap::new();
    let mut fields = name_status.split('\0').filter(|field| !field.is_empty());

    while let Some(code) = fields.next() {
        let kind = StatusKind::from_status_code(code);
        let path = if kind == StatusKind::Renamed {
            new_path_of_rename(&mut fields)
        } else {
            fields.next()
        };
        let Some(path) = path else {
            break;
        };
        kinds.insert(path, kind);
    }
    kinds
}

/// Raw unified diff for one path within a commit.
#[tauri::command]
pub async fn vcs_commit_diff(cwd: String, sha: String, path: String) -> Result<String, String> {
    let sha = validate_object_id(&sha)?;
    run_git(
        &cwd,
        &["show", "--no-color", "--end-of-options", sha, "--", &path],
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_numstat_records, status_kinds_by_path, StatusKind};

    #[test]
    fn a_plain_numstat_record_keeps_its_path_and_counts() {
        let records = parse_numstat_records("3\t2\tsrc/app.ts\0");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].path, "src/app.ts");
        assert_eq!(records[0].additions, 3);
        assert_eq!(records[0].deletions, 2);
        assert!(!records[0].is_binary);
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
        assert!(records[0].is_binary);
        assert_eq!(records[0].additions, 0);
        assert_eq!(records[0].deletions, 0);
    }

    #[test]
    fn name_status_keys_a_rename_on_the_new_path() {
        let kinds = status_kinds_by_path("M\0src/app.ts\0R100\0src/old.ts\0src/new.ts\0");
        assert_eq!(kinds.get("src/app.ts"), Some(&StatusKind::Modified));
        assert_eq!(kinds.get("src/new.ts"), Some(&StatusKind::Renamed));
        assert!(!kinds.contains_key("src/old.ts"));
    }

    #[test]
    fn status_codes_map_to_kinds() {
        assert_eq!(StatusKind::from_status_code("A"), StatusKind::Created);
        assert_eq!(StatusKind::from_status_code("D"), StatusKind::Deleted);
        assert_eq!(StatusKind::from_status_code("R87"), StatusKind::Renamed);
        assert_eq!(StatusKind::from_status_code("C50"), StatusKind::Renamed);
        assert_eq!(StatusKind::from_status_code("M"), StatusKind::Modified);
    }
}
