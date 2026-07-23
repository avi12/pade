//! Branch queries: the current HEAD branch and the local branch list.

use std::collections::BTreeMap;

use super::run_git;

/// The current HEAD branch name, for `vcs_commit`. `None`/detached maps to no
/// branch.
pub(crate) fn current_branch(cwd: &str) -> Option<String> {
    let raw = run_git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;
    let name = raw.trim();
    if name.is_empty() || name == "HEAD" {
        return None;
    }
    Some(name.to_string())
}

/// The HEAD branch at `path`, or `None` when it isn't a repo / is detached.
fn branch_at(path: &str) -> Option<String> {
    let out = crate::util::command("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!name.is_empty() && name != "HEAD").then_some(name)
}

/// Current HEAD branch for each of `paths`, for the switcher's per-project branch
/// chip. Runs git per path (`-C <path>`); a path that isn't a git repo or is on a
/// detached HEAD is omitted, so the frontend shows a chip only where one exists.
#[tauri::command]
pub async fn vcs_branch_of(paths: Vec<String>) -> BTreeMap<String, String> {
    paths
        .into_iter()
        .filter_map(|path| branch_at(&path).map(|branch| (path, branch)))
        .collect()
}

/// Local branches in the current repo (empty/Err when not a git repo).
#[tauri::command]
pub async fn vcs_branches(cwd: String) -> Result<Vec<String>, String> {
    let raw = run_git(&cwd, &["branch", "--format=%(refname:short)"])?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}
