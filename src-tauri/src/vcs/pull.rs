//! Sync the workspace from `origin` — a strictly fast-forward `git pull`.
//!
//! The Change Feed's "Sync all" runs this. It never creates a merge commit
//! (`--ff-only`) and never touches a dirty working tree: uncommitted changes to
//! tracked files are refused up front so an agent's in-progress edits can't be
//! clobbered. A branch that has diverged from its upstream (no fast-forward
//! possible) surfaces git's own error verbatim through `run_git`.

use serde::Serialize;

use super::run_git;

/// How a sync resolved. Serializes to the camelCase wire strings the frontend's
/// zod `PullOutcome` schema reads — the one authoritative home for the literals.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
enum PullStatus {
    /// Fast-forwarded the branch to new upstream commits.
    Updated,
    /// The branch already matched its upstream — nothing to fetch.
    AlreadyUpToDate,
    /// Refused: the working tree has uncommitted changes to tracked files.
    RefusedDirty,
}

/// The result of a sync the Change Feed can render: an outcome plus a short,
/// human-readable line for the status toast.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullOutcome {
    status: PullStatus,
    message: String,
}

const REFUSED_DIRTY_MESSAGE: &str = "Uncommitted changes — commit or stash before syncing.";
const ALREADY_UP_TO_DATE_MESSAGE: &str = "Already up to date.";
const UPDATED_MESSAGE: &str = "Synced with origin.";

/// Does `git status --porcelain` output show uncommitted changes to *tracked*
/// files? Untracked files (`??`) don't count — a fast-forward can't silently
/// clobber them (git refuses the pull if it would need to overwrite one), and
/// stray build artifacts shouldn't block a sync. Pure so it's tested without git.
fn has_uncommitted_changes(porcelain: &str) -> bool {
    porcelain
        .lines()
        .any(|line| !line.trim().is_empty() && !line.starts_with("??"))
}

/// Did a successful `git pull` report the branch was already current? git's
/// wording drifts across versions ("Already up to date." vs the older
/// "Already up-to-date."), so normalize the hyphen and case before matching.
fn is_already_up_to_date(pull_output: &str) -> bool {
    pull_output
        .to_lowercase()
        .replace('-', " ")
        .contains("already up to date")
}

/// Fast-forward the current branch from `origin`, refusing a dirty working tree.
///
/// Pre-checks `git status --porcelain`: any uncommitted change to a tracked file
/// returns `RefusedDirty` without running the pull. Otherwise it runs
/// `git pull --ff-only` — a diverged branch (no fast-forward) errors out with
/// git's message rather than creating a merge commit.
#[tauri::command]
pub fn vcs_pull(cwd: String) -> Result<PullOutcome, String> {
    let porcelain = run_git(&cwd, &["status", "--porcelain"])?;
    if has_uncommitted_changes(&porcelain) {
        return Ok(PullOutcome {
            status: PullStatus::RefusedDirty,
            message: REFUSED_DIRTY_MESSAGE.to_string(),
        });
    }

    let output = run_git(&cwd, &["pull", "--ff-only"])?;
    if is_already_up_to_date(&output) {
        return Ok(PullOutcome {
            status: PullStatus::AlreadyUpToDate,
            message: ALREADY_UP_TO_DATE_MESSAGE.to_string(),
        });
    }
    Ok(PullOutcome {
        status: PullStatus::Updated,
        message: UPDATED_MESSAGE.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{has_uncommitted_changes, is_already_up_to_date};

    #[test]
    fn a_modified_tracked_file_is_dirty() {
        assert!(has_uncommitted_changes(" M src/app.ts"));
        assert!(has_uncommitted_changes("M  src/app.ts"));
    }

    #[test]
    fn a_staged_addition_and_deletion_are_dirty() {
        assert!(has_uncommitted_changes("A  src/new.ts"));
        assert!(has_uncommitted_changes(" D src/gone.ts"));
    }

    #[test]
    fn an_only_untracked_tree_is_not_dirty() {
        assert!(!has_uncommitted_changes("?? build/bundle.js\n?? notes.txt"));
    }

    #[test]
    fn an_empty_status_is_not_dirty() {
        assert!(!has_uncommitted_changes(""));
        assert!(!has_uncommitted_changes("\n"));
    }

    #[test]
    fn a_tracked_change_amid_untracked_files_is_dirty() {
        assert!(has_uncommitted_changes("?? build/bundle.js\n M src/app.ts"));
    }

    #[test]
    fn both_up_to_date_spellings_are_recognized() {
        assert!(is_already_up_to_date("Already up to date.\n"));
        assert!(is_already_up_to_date("Already up-to-date.\n"));
    }

    #[test]
    fn a_fast_forward_summary_is_not_up_to_date() {
        let summary = "Updating a1b2c3d..e4f5g6h\nFast-forward\n src/app.ts | 2 +-\n";
        assert!(!is_already_up_to_date(summary));
    }

    /// End-to-end against real git (skipped when git is unavailable): a clean
    /// clone reports up-to-date, a real fast-forward classifies as an update,
    /// and a local tracked edit reads as dirty. Drives git directly with `-C`
    /// (never the process cwd) so it can't race other tests.
    #[test]
    fn real_git_fast_forward_and_dirty_detection() {
        let base = std::env::temp_dir().join(format!("pade-pull-{}", std::process::id()));
        let origin = base.join("origin");
        let clone = base.join("clone");
        std::fs::create_dir_all(&origin).expect("test dirs");

        let git = |dir: &std::path::Path, args: &[&str]| {
            crate::util::command("git")
                .arg("-C")
                .arg(dir)
                .args(args)
                .output()
        };
        let cleanup = || {
            std::fs::remove_dir_all(&base).ok();
        };
        let identity = [
            "-c",
            "user.email=pade@test.local",
            "-c",
            "user.name=PADE Test",
            "-c",
            "commit.gpgsign=false",
        ];

        let Ok(init) = git(&origin, &["init", "-q"]) else {
            cleanup();
            return;
        };
        if !init.status.success() {
            cleanup();
            return;
        }

        std::fs::write(origin.join("readme.md"), "one\n").expect("seed file");
        let mut first_commit = identity.to_vec();
        first_commit.extend_from_slice(&["commit", "-q", "-m", "first"]);
        let ran = git(&origin, &["add", "."])
            .and_then(|_| git(&origin, &first_commit))
            .and_then(|_| {
                // Clone from `base` (which exists) — `git -C <clone>` would fail
                // before git has created the clone dir.
                git(
                    &base,
                    &[
                        "clone",
                        "-q",
                        &origin.to_string_lossy(),
                        &clone.to_string_lossy(),
                    ],
                )
            });
        // Guard the whole setup chain and skip if any step the environment can't
        // do fails (e.g. git absent, or a sandbox that blocks a local clone).
        let Ok(clone_out) = ran else {
            cleanup();
            return;
        };
        if !clone_out.status.success() {
            cleanup();
            return;
        }

        let up_to_date = git(&clone, &["pull", "--ff-only"]).expect("pull");
        let up_to_date_text = String::from_utf8_lossy(&up_to_date.stdout);
        assert!(is_already_up_to_date(&up_to_date_text));

        std::fs::write(origin.join("readme.md"), "one\ntwo\n").expect("advance file");
        let mut second_commit = identity.to_vec();
        second_commit.extend_from_slice(&["commit", "-q", "-m", "second"]);
        git(&origin, &["add", "."]).expect("stage");
        git(&origin, &second_commit).expect("commit");

        let updated = git(&clone, &["pull", "--ff-only"]).expect("pull");
        assert!(updated.status.success());
        assert!(!is_already_up_to_date(&String::from_utf8_lossy(
            &updated.stdout
        )));

        std::fs::write(clone.join("readme.md"), "local edit\n").expect("local edit");
        let dirty = git(&clone, &["status", "--porcelain"]).expect("status");
        assert!(has_uncommitted_changes(&String::from_utf8_lossy(
            &dirty.stdout
        )));

        cleanup();
    }
}
