//! Branch queries: the current HEAD branch and the local branch list.

use std::collections::BTreeMap;

use super::run_git;

/// The current HEAD branch name — the one authoritative "current branch" query,
/// used by `vcs_commit`, the per-project branch chip, and remote resolution.
/// `rev-parse --abbrev-ref HEAD` reports `HEAD` on a detached checkout, which
/// (with an empty result on a non-repo) maps to `None` — no branch.
pub(crate) fn current_branch(cwd: &str) -> Option<String> {
    let raw = run_git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;
    let name = raw.trim();
    if name.is_empty() || name == "HEAD" {
        return None;
    }
    Some(name.to_string())
}

/// Current HEAD branch for each of `paths`, for the switcher's per-project branch
/// chip. Queries git per path; a path that isn't a git repo or is on a detached
/// HEAD is omitted, so the frontend shows a chip only where one exists.
#[tauri::command]
pub async fn vcs_branch_of(paths: Vec<String>) -> BTreeMap<String, String> {
    paths
        .into_iter()
        .filter_map(|path| current_branch(&path).map(|branch| (path, branch)))
        .collect()
}

/// Local branches in the current repo (empty/Err when not a git repo).
#[tauri::command]
pub async fn vcs_branches(cwd: String) -> Result<Vec<String>, String> {
    let raw = run_git(&cwd, &["branch", "--format=%(refname:short)"])?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}
