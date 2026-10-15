//! Recent-commit log with per-commit `--numstat` totals.

use serde::Serialize;

use super::{
    commit_header_format, run_git, CommitHeader, NumstatRow, FIELD_SEPARATOR, RECORD_SEPARATOR,
};

/// A recent commit for the Log view.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    #[serde(flatten)]
    header: CommitHeader,
    /// Lines added across the commit (sum of `--numstat`).
    additions: u32,
    /// Lines deleted across the commit.
    deletions: u32,
    /// Number of files the commit touched.
    files: u32,
}

impl Commit {
    /// Fold one `--numstat` row into this commit's totals. A binary row counts
    /// the file but adds no lines.
    fn add_row(&mut self, row: &NumstatRow<'_>) {
        self.additions += row.additions;
        self.deletions += row.deletions;
        self.files += 1;
    }
}

#[tauri::command]
pub async fn vcs_log(cwd: String, limit: u32) -> Result<Vec<Commit>, String> {
    let raw = run_git(
        &cwd,
        &[
            "log",
            &format!("-n{limit}"),
            "--numstat",
            &format!(
                "--pretty=format:{RECORD_SEPARATOR}{}",
                commit_header_format()
            ),
        ],
    )?;
    Ok(parse_log(&raw))
}

/// Parse `git log --numstat` output where each commit header starts with the
/// record separator and the numstat rows that follow fold into it.
fn parse_log(raw: &str) -> Vec<Commit> {
    let mut commits: Vec<Commit> = Vec::new();
    for line in raw.lines() {
        if let Some(header_line) = line.strip_prefix(RECORD_SEPARATOR) {
            let fields: Vec<&str> = header_line.split(FIELD_SEPARATOR).collect();
            if let Some((header, [])) = CommitHeader::split_off(&fields) {
                commits.push(Commit {
                    header,
                    additions: 0,
                    deletions: 0,
                    files: 0,
                });
            }
            continue;
        }
        let Some(commit) = commits.last_mut() else {
            continue;
        };
        if let Some(row) = NumstatRow::parse(line) {
            commit.add_row(&row);
        }
    }
    commits
}

#[cfg(test)]
mod tests {
    use super::{parse_log, FIELD_SEPARATOR, RECORD_SEPARATOR};

    fn header(id: &str) -> String {
        let fields = [id, "abc", "subject", "Avi", "2 days ago"];
        format!(
            "{RECORD_SEPARATOR}{}",
            fields.join(&FIELD_SEPARATOR.to_string())
        )
    }

    #[test]
    fn numstat_rows_fold_into_the_commit_above_them() {
        let raw = format!(
            "{}\n3\t2\tsrc/app.ts\n-\t-\tassets/logo.png\n\n{}\n1\t0\tREADME.md",
            header("first"),
            header("second")
        );
        let commits = parse_log(&raw);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].header.id, "first");
        assert_eq!(
            (commits[0].additions, commits[0].deletions, commits[0].files),
            (3, 2, 2)
        );
        assert_eq!(
            (commits[1].additions, commits[1].deletions, commits[1].files),
            (1, 0, 1)
        );
    }

    #[test]
    fn a_row_without_a_path_column_is_not_counted() {
        let raw = format!("{}\n3\t2", header("only"));
        let commits = parse_log(&raw);
        assert_eq!(commits[0].files, 0);
    }

    #[test]
    fn a_malformed_header_is_skipped_with_its_rows() {
        let raw = format!("{RECORD_SEPARATOR}broken\n3\t2\tsrc/app.ts");
        assert!(parse_log(&raw).is_empty());
    }
}
