//! Branch queries: the current HEAD branch and the local branch list.

use super::run_git;

/// The current HEAD branch name, for `vcs_commit`. `None`/detached maps to no
/// branch.
pub(crate) fn current_branch() -> Option<String> {
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
