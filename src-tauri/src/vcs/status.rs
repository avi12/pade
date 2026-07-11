//! Working-tree status: the changed-path list feeding the panel's
//! unreviewed/staged groups, and the per-path working-tree/staged diff.

use serde::Serialize;

use super::{run_git, StatusKind};

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

#[cfg(test)]
mod tests {
    use super::classify;

    #[test]
    fn untracked_is_never_staged() {
        let (kind, staged) = classify('?', '?');
        assert_eq!(kind.as_str(), "untracked");
        assert!(!staged);
    }

    #[test]
    fn a_worktree_edit_is_unstaged_modified() {
        let (kind, staged) = classify(' ', 'M');
        assert_eq!(kind.as_str(), "modified");
        assert!(!staged);
    }

    #[test]
    fn an_index_addition_is_staged_created() {
        let (kind, staged) = classify('A', ' ');
        assert_eq!(kind.as_str(), "created");
        assert!(staged);
    }

    #[test]
    fn an_index_rename_is_staged_renamed() {
        let (kind, staged) = classify('R', ' ');
        assert_eq!(kind.as_str(), "renamed");
        assert!(staged);
    }

    #[test]
    fn a_worktree_deletion_is_unstaged_deleted() {
        let (kind, staged) = classify(' ', 'D');
        assert_eq!(kind.as_str(), "deleted");
        assert!(!staged);
    }
}
