//! Branch queries: the current HEAD branch, the local branch list, the branches
//! only a remote has yet, and the switch between them.

use std::collections::{BTreeMap, BTreeSet};

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

/// Every branch name one `git branch` invocation printed, in git's order and
/// without the blank padding lines. The one place the branch list is parsed —
/// [`vcs_branches`] and [`vcs_remote_branches`] differ only in the refs asked for.
fn branch_names(cwd: &str, args: &[&str]) -> Result<Vec<String>, String> {
    let raw = run_git(cwd, args)?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

/// The repo's local branches — the one query [`vcs_branches`] answers and
/// [`vcs_remote_branches`] subtracts, so both read the same list.
fn local_branches(cwd: &str) -> Result<Vec<String>, String> {
    branch_names(cwd, &["branch", "--format=%(refname:short)"])
}

/// Local branches in the current repo (empty/Err when not a git repo).
#[tauri::command]
pub async fn vcs_branches(cwd: String) -> Result<Vec<String>, String> {
    local_branches(&cwd)
}

/// Branches that exist only on a remote — `origin/feature` with no local
/// `feature` — under their short name, so the switcher can offer the work a
/// fresh clone hasn't checked out yet. `git switch <name>` then creates the
/// local tracking branch. `strip=3` drops `refs/remotes/<remote>/`, keeping a
/// slashed branch name (`feat/x`) whole; `HEAD` is the remote's symbolic
/// default pointer, not a branch of its own.
#[tauri::command]
pub async fn vcs_remote_branches(cwd: String) -> Result<Vec<String>, String> {
    let local: BTreeSet<String> = local_branches(&cwd)?.into_iter().collect();
    let remote = branch_names(
        &cwd,
        &[
            "for-each-ref",
            "--format=%(refname:strip=3)",
            "refs/remotes",
        ],
    )?;

    let mut seen = BTreeSet::new();
    Ok(remote
        .into_iter()
        .filter(|name| name != "HEAD" && !local.contains(name) && seen.insert(name.clone()))
        .collect())
}

/// Check `branch` out in the repo at `cwd` and report the branch HEAD ended on.
///
/// `--end-of-options` keeps a branch named like a switch (`-f`) an argument.
/// Git refuses a switch that would lose uncommitted work, or one whose branch is
/// already checked out in another worktree; that refusal surfaces verbatim
/// through `run_git` so the caller can say what stands in the way.
#[tauri::command]
pub async fn vcs_switch_branch(cwd: String, branch: String) -> Result<String, String> {
    run_git(&cwd, &["switch", "--end-of-options", &branch])?;
    current_branch(&cwd).ok_or_else(|| format!("switched to {branch} but HEAD is detached"))
}
