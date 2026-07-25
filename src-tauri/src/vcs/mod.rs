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
pub(crate) mod clone;
pub(crate) mod inspect;
pub(crate) mod log;
pub(crate) mod pull;
pub(crate) mod remote;
pub(crate) mod restore;
pub(crate) mod status;
pub(crate) mod worktree;

pub(crate) const US: char = '\u{1f}'; // field separator inside a record
pub(crate) const RS: char = '\u{1e}'; // record separator — marks the start of a log entry

/// Disable every interactive credential path so a git command that reaches the
/// network fails fast with a real error instead of hanging on an invisible
/// terminal prompt or a credential-manager popup. The one authoritative home for
/// this env pair — shared by [`run_git`] and the clone/probe sites.
pub(crate) fn harden_git(command: &mut std::process::Command) -> &mut std::process::Command {
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
}

/// Run Git inside one explicitly selected workspace. A Tauri process can host
/// several windows at once, so the process working directory belongs to no one
/// window and must never decide which repository a request reads or mutates.
pub(crate) fn run_git(cwd: &str, args: &[&str]) -> Result<String, String> {
    let mut command = crate::util::command("git");
    command.args(args).current_dir(cwd);
    let output = harden_git(&mut command)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Read a tracked file's committed text after it has disappeared from disk.
/// Used only when the watcher first encounters a path as a deletion and therefore
/// had no opportunity to capture the normal in-memory first-touch baseline.
pub(crate) fn head_file_text(
    cwd: &std::path::Path,
    path: &std::path::Path,
    max_bytes: u64,
) -> Option<String> {
    let relative = path.strip_prefix(cwd).ok()?;
    let relative = relative.to_string_lossy().replace('\\', "/");
    let object = format!("HEAD:{relative}");
    let text = run_git(&cwd.to_string_lossy(), &["show", "--no-textconv", &object]).ok()?;
    let within_limit = u64::try_from(text.len()).ok()? <= max_bytes;
    (within_limit && !text.contains('\0')).then_some(text)
}

/// Accept only object ids Git produced for the UI. This prevents a renderer from
/// turning a revision position into an option such as an external-diff switch.
pub(crate) fn validate_object_id(object_id: &str) -> Result<&str, String> {
    let valid_length = (4..=64).contains(&object_id.len());
    if valid_length && object_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(object_id);
    }
    Err("invalid Git object id".into())
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
    /// The change kind for a git status letter (`A`/`D`/`R`/`C`), defaulting to
    /// `Modified`. Renames and copies carry a trailing similarity score (`R100`),
    /// so callers pass the leading letter. One authoritative home for the
    /// letter→kind mapping — shared by working-tree status and commit inspection.
    pub(crate) fn from_git_letter(letter: char) -> StatusKind {
        match letter {
            'A' => StatusKind::Created,
            'D' => StatusKind::Deleted,
            'R' | 'C' => StatusKind::Renamed,
            _ => StatusKind::Modified,
        }
    }

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

#[cfg(test)]
mod tests {
    use super::{head_file_text, run_git, validate_object_id};

    #[test]
    fn object_ids_reject_options_and_non_hex_revisions() {
        assert_eq!(validate_object_id("a1b2c3d"), Ok("a1b2c3d"));
        for object_id in ["--ext-diff", "HEAD", "abc", "gggg"] {
            assert!(
                validate_object_id(object_id).is_err(),
                "accepted {object_id}"
            );
        }
    }

    #[test]
    fn head_file_text_recovers_a_first_sighting_deletion() {
        let scratch = std::env::temp_dir().join(format!("pade-head-text-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("create scratch repository");
        let cwd = scratch.to_string_lossy();
        if run_git(&cwd, &["init", "-q"]).is_err() {
            let _ = std::fs::remove_dir_all(scratch);
            return;
        }

        let file = scratch.join("deleted.txt");
        std::fs::write(&file, "before deletion\n").expect("write tracked file");
        run_git(&cwd, &["add", "deleted.txt"]).expect("stage tracked file");
        run_git(
            &cwd,
            &[
                "-c",
                "user.name=PADE Test",
                "-c",
                "user.email=pade@example.invalid",
                "commit",
                "-qm",
                "baseline",
            ],
        )
        .expect("commit tracked file");
        std::fs::remove_file(&file).expect("delete tracked file");

        assert_eq!(
            head_file_text(&scratch, &file, 1024).as_deref(),
            Some("before deletion\n")
        );
        std::fs::remove_dir_all(scratch).expect("scratch cleanup");
    }
}
