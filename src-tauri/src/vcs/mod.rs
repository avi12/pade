//! Version-control review — Git backend.
//!
//! MVP shells out to the `git` binary the user already has (robust, no native
//! build deps). This module is the single seam behind which other backends
//! (Jujutsu, Mercurial, or a native `gix`/`git2` impl) can slot in later.
//!
//! One concern per submodule; this file holds only the shared plumbing (the
//! `git` runner, the wire separators, the status-kind vocabulary).
//!
//! A future git-bisect pair (`vcs_bisect_start` / `vcs_bisect_step`) slots in
//! behind `run_git`: start would `git bisect start <bad> <good>`, and step
//! would mark the current revision good/bad and report the next one to test.
//! Not implemented yet.

pub(crate) mod branches;
pub(crate) mod inspect;
pub(crate) mod log;
pub(crate) mod remote;
pub(crate) mod restore;
pub(crate) mod status;
pub(crate) mod worktree;

pub(crate) const US: char = '\u{1f}'; // field separator inside a record
pub(crate) const RS: char = '\u{1e}'; // record separator — marks the start of a log entry

pub(crate) fn run_git(args: &[&str]) -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let out = crate::util::command("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// How a working-tree path changed, in the exact wire strings the frontend reads.
/// One authoritative home for the status-kind literals.
#[derive(Clone, Copy)]
pub(crate) enum StatusKind {
    Created,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

impl StatusKind {
    /// The serialized string for this kind — the only place the literals live.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            StatusKind::Created => "created",
            StatusKind::Modified => "modified",
            StatusKind::Deleted => "deleted",
            StatusKind::Renamed => "renamed",
            StatusKind::Untracked => "untracked",
        }
    }
}
