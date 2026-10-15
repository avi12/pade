//! Working-tree status: the changed-path list feeding the panel's
//! unreviewed/staged groups, and the per-path working-tree/staged diff.

use serde::Serialize;

use super::{run_git, StatusKind};

/// The index letter `git status --porcelain` prints for an untracked path (`??`).
const UNTRACKED_LETTER: char = '?';
/// The letter for "no change on this side" in a porcelain status pair.
const UNCHANGED_LETTER: char = ' ';
/// The letters that mark a rename and a copy - the two records whose original
/// path follows as the next `-z` field.
const RENAMED_LETTER: char = 'R';
const COPIED_LETTER: char = 'C';
/// Where the path starts in a porcelain record: after `XY` and one space.
const PATH_OFFSET: usize = 3;

/// A single changed path in the working tree.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEntry {
    path: String,
    kind: StatusKind,
    /// Staged in the index (agent commits land here after "approve").
    staged: bool,
}

fn classify(index: char, worktree: char) -> (StatusKind, bool) {
    if index == UNTRACKED_LETTER {
        return (StatusKind::Untracked, false);
    }
    let is_staged = index != UNCHANGED_LETTER;
    let letter = if is_staged { index } else { worktree };
    (StatusKind::from_git_letter(letter), is_staged)
}

#[tauri::command]
pub async fn vcs_status(cwd: String) -> Result<Vec<StatusEntry>, String> {
    // NUL-delimited records survive paths with spaces/newlines.
    let raw = run_git(&cwd, &["status", "--porcelain=v1", "-z"])?;
    Ok(parse_status(&raw))
}

/// Parse `git status --porcelain=v1 -z`: each record is `XY <path>`, and a
/// rename or copy record is followed by its original path in the next field,
/// which is dropped.
fn parse_status(raw: &str) -> Vec<StatusEntry> {
    let mut entries = Vec::new();
    let mut records = raw.split('\0');

    while let Some(record) = records.next() {
        let mut letters = record.chars();
        let (Some(index), Some(worktree)) = (letters.next(), letters.next()) else {
            continue;
        };
        let Some(path) = record.get(PATH_OFFSET..) else {
            continue;
        };

        let has_original_path = [index, worktree]
            .iter()
            .any(|letter| matches!(*letter, RENAMED_LETTER | COPIED_LETTER));
        if has_original_path {
            records.next();
        }

        let (kind, staged) = classify(index, worktree);
        entries.push(StatusEntry {
            path: path.to_string(),
            kind,
            staged,
        });
    }
    entries
}

/// Raw unified diff for one path (staged or working-tree).
#[tauri::command]
pub async fn vcs_diff(cwd: String, path: String, staged: bool) -> Result<String, String> {
    let mut arguments = vec!["diff", "--no-color"];
    if staged {
        arguments.push("--staged");
    }
    arguments.push("--");
    arguments.push(&path);
    run_git(&cwd, &arguments)
}

#[cfg(test)]
mod tests {
    use super::{classify, parse_status, StatusKind};

    #[test]
    fn untracked_is_never_staged() {
        assert_eq!(classify('?', '?'), (StatusKind::Untracked, false));
    }

    #[test]
    fn a_worktree_edit_is_unstaged_modified() {
        assert_eq!(classify(' ', 'M'), (StatusKind::Modified, false));
    }

    #[test]
    fn an_index_addition_is_staged_created() {
        assert_eq!(classify('A', ' '), (StatusKind::Created, true));
    }

    #[test]
    fn an_index_rename_is_staged_renamed() {
        assert_eq!(classify('R', ' '), (StatusKind::Renamed, true));
    }

    #[test]
    fn a_worktree_deletion_is_unstaged_deleted() {
        assert_eq!(classify(' ', 'D'), (StatusKind::Deleted, false));
    }

    #[test]
    fn a_rename_record_drops_the_old_path_field() {
        let entries = parse_status(" M src/app.ts\0R  src/new.ts\0src/old.ts\0?? notes.md\0");
        let paths: Vec<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["src/app.ts", "src/new.ts", "notes.md"]);
        assert_eq!(entries[1].kind, StatusKind::Renamed);
        assert!(entries[1].staged);
    }

    /// A copy record carries its source path as the next field too; reading
    /// that field as a record of its own invented a bogus entry.
    #[test]
    fn a_copy_record_drops_the_original_path_field() {
        let entries = parse_status("C  src/copy.ts\0src/original.ts\0 M src/app.ts\0");
        let paths: Vec<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["src/copy.ts", "src/app.ts"]);
    }

    #[test]
    fn a_short_record_is_skipped() {
        assert!(parse_status("M\0\0").is_empty());
    }
}
