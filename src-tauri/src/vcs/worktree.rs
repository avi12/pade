//! Git worktrees - an isolated checkout per branch for per-branch agents.

use std::path::Path;

use crate::workspace::ensure_config_dir;

use super::run_git;

/// The folder under PADE's config directory that holds every agent worktree.
const WORKTREES_DIRECTORY: &str = "worktrees";
/// The repository folder name used when the toplevel path has no readable name.
const FALLBACK_REPOSITORY_NAME: &str = "repo";

/// Add a git worktree for `branch` so an agent can work it in isolation, and
/// return the worktree path. Reuses an existing worktree if already created.
/// `create` makes a new branch (`git worktree add -b`) off the current HEAD.
#[tauri::command]
pub async fn vcs_worktree_add(cwd: String, branch: String, create: bool) -> Result<String, String> {
    let toplevel = run_git(&cwd, &["rev-parse", "--show-toplevel"])?;
    let repository_name = Path::new(toplevel.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(FALLBACK_REPOSITORY_NAME);
    // Branches can contain '/', which would nest directories - flatten for the folder.
    let folder_name = branch.replace(['/', '\\'], "-");
    let directory = ensure_config_dir()?
        .join(WORKTREES_DIRECTORY)
        .join(repository_name)
        .join(folder_name);
    let directory_text = directory.to_string_lossy().into_owned();

    if directory.exists() {
        return Ok(directory_text);
    }

    if create {
        run_git(&cwd, &["worktree", "add", "-b", &branch, &directory_text])?;
    } else {
        run_git(&cwd, &["worktree", "add", &directory_text, &branch])?;
    }
    Ok(directory_text)
}
