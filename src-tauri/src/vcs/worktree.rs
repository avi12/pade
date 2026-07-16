//! Git worktrees — an isolated checkout per branch for per-branch agents.

use std::path::Path;

use crate::workspace::ensure_config_dir;

use super::run_git;

/// Add a git worktree for `branch` so an agent can work it in isolation, and
/// return the worktree path. Reuses an existing worktree if already created.
/// `create` makes a new branch (`git worktree add -b`) off the current HEAD.
#[tauri::command]
pub fn vcs_worktree_add(branch: String, create: bool) -> Result<String, String> {
    let root = run_git(&["rev-parse", "--show-toplevel"])?
        .trim()
        .to_string();
    let repo = Path::new(&root)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");
    // Branches can contain '/', which would nest dirs — flatten for the folder.
    let safe = branch.replace(['/', '\\'], "-");
    let dir = ensure_config_dir()?
        .join("worktrees")
        .join(repo)
        .join(&safe);
    let dir_str = dir.to_string_lossy().into_owned();

    if dir.exists() {
        return Ok(dir_str);
    }

    if create {
        run_git(&["worktree", "add", "-b", &branch, &dir_str])?;
    } else {
        run_git(&["worktree", "add", &dir_str, &branch])?;
    }
    Ok(dir_str)
}
