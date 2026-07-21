//! Recent-commit log with per-commit `--numstat` totals.

use serde::Serialize;

use super::{run_git, RS, US};

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

#[tauri::command]
pub fn vcs_log(cwd: String, limit: u32) -> Result<Vec<Commit>, String> {
    // A record-start marker (RS) precedes each commit header so we can tell a
    // header line apart from the `--numstat` rows that follow it. The header
    // fields are US-separated as before.
    let fmt = format!("{RS}%H{US}%h{US}%s{US}%an{US}%cr");
    let raw = run_git(
        &cwd,
        &[
            "log",
            &format!("-n{limit}"),
            "--numstat",
            &format!("--pretty=format:{fmt}"),
        ],
    )?;

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

#[cfg(test)]
mod tests {
    use super::parse_numstat;

    #[test]
    fn parses_a_plain_numstat_row() {
        assert_eq!(parse_numstat("3\t2\tsrc/app.ts"), Some((3, 2)));
    }

    #[test]
    fn counts_a_binary_row_as_zero_lines() {
        assert_eq!(parse_numstat("-\t-\tassets/logo.png"), Some((0, 0)));
    }

    #[test]
    fn rejects_a_line_without_a_path_column() {
        assert_eq!(parse_numstat("3\t2"), None);
        assert_eq!(parse_numstat(""), None);
    }
}
