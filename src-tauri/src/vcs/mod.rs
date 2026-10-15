//! Version-control review - Git backend.
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

use serde::Serialize;

/// ASCII unit separator: splits the fields inside one `--format` record.
const FIELD_SEPARATOR: char = '\u{1f}';
/// ASCII record separator: marks the start of a log entry, so a header line
/// reads apart from the `--numstat` rows that follow it.
const RECORD_SEPARATOR: char = '\u{1e}';

/// The remote a fresh clone names, and the fallback when a branch has none.
const DEFAULT_REMOTE_NAME: &str = "origin";

/// The symbolic ref git prints in place of a branch name on a detached
/// checkout, and the name of a remote's default-branch pointer.
const HEAD_REFERENCE: &str = "HEAD";

/// The shortest abbreviated object id git prints, and the longest full one
/// (SHA-256).
const MIN_OBJECT_ID_LENGTH: usize = 4;
const MAX_OBJECT_ID_LENGTH: usize = 64;

/// The `--format` placeholders for the header every commit listing shows -
/// full id, short id, subject, author, relative date - in [`CommitHeader`]'s
/// field order. A listing appends its own extra fields after these.
fn commit_header_format() -> String {
    ["%H", "%h", "%s", "%an", "%cr"].join(&FIELD_SEPARATOR.to_string())
}

/// The header fields shared by the Log view, commit inspection and restore
/// candidates. Flattened into each wire type, so their JSON stays flat.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommitHeader {
    id: String,
    short: String,
    summary: String,
    author: String,
    when: String,
}

impl CommitHeader {
    /// Split a record printed with [`commit_header_format`] into the header
    /// and whatever extra fields the listing appended after it.
    fn split_off<'fields, 'text>(
        fields: &'fields [&'text str],
    ) -> Option<(Self, &'fields [&'text str])> {
        let ([id, short, summary, author, when], rest) = fields.split_first_chunk()?;
        let header = Self {
            id: (*id).into(),
            short: (*short).into(),
            summary: (*summary).into(),
            author: (*author).into(),
            when: (*when).into(),
        };
        Some((header, rest))
    }
}

/// One `git --numstat` row: `<additions>\t<deletions>\t<path>`. Binary files
/// print `-` for both counts, which read as zero lines. The path column is
/// empty for a `-z` rename, whose paths follow as their own fields.
struct NumstatRow<'row> {
    additions: u32,
    deletions: u32,
    is_binary: bool,
    path: &'row str,
}

impl<'row> NumstatRow<'row> {
    /// `None` for a line that isn't a numstat row (e.g. a blank separator).
    fn parse(row: &'row str) -> Option<Self> {
        let mut columns = row.splitn(3, '\t');
        let additions = columns.next()?;
        let deletions = columns.next()?;
        let path = columns.next()?;
        Some(Self {
            additions: additions.parse().unwrap_or(0),
            deletions: deletions.parse().unwrap_or(0),
            is_binary: additions == "-" || deletions == "-",
            path,
        })
    }
}

/// Disable every interactive credential path so a git command that reaches the
/// network fails fast with a real error instead of hanging on an invisible
/// terminal prompt or a credential-manager popup. The one authoritative home for
/// this env pair - shared by [`run_git`] and the clone/probe sites.
fn harden_git(command: &mut std::process::Command) -> &mut std::process::Command {
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
}

/// A bare `git -C <directory>` command, for callers outside `vcs` that read
/// raw output or exit status rather than [`run_git`]'s string result. `-C`
/// scopes it to that directory, never PADE's process cwd.
pub(crate) fn git_in(directory: &std::path::Path) -> std::process::Command {
    let mut command = crate::util::command("git");
    command.arg("-C").arg(directory);
    command
}

/// Run Git inside one explicitly selected workspace. A Tauri process can host
/// several windows at once, so the process working directory belongs to no one
/// window and must never decide which repository a request reads or mutates.
fn run_git(cwd: &str, args: &[&str]) -> Result<String, String> {
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
    let object = format!("{HEAD_REFERENCE}:{relative}");
    let text = run_git(&cwd.to_string_lossy(), &["show", "--no-textconv", &object]).ok()?;
    let within_limit = u64::try_from(text.len()).ok()? <= max_bytes;
    (within_limit && !text.contains('\0')).then_some(text)
}

/// Accept only object ids Git produced for the UI. This prevents a renderer from
/// turning a revision position into an option such as an external-diff switch.
fn validate_object_id(object_id: &str) -> Result<&str, String> {
    let is_valid_length = (MIN_OBJECT_ID_LENGTH..=MAX_OBJECT_ID_LENGTH).contains(&object_id.len());
    if is_valid_length && object_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(object_id);
    }
    Err("invalid Git object id".into())
}

/// How a working-tree path changed. Serializes to the exact lowercase wire
/// strings the frontend reads - the one authoritative home for those literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum StatusKind {
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
    /// letter→kind mapping - shared by working-tree status and commit inspection.
    fn from_git_letter(letter: char) -> Self {
        match letter {
            'A' => Self::Created,
            'D' => Self::Deleted,
            'R' | 'C' => Self::Renamed,
            _ => Self::Modified,
        }
    }

    /// The change kind for a whole `--name-status` code such as `R100`.
    fn from_status_code(code: &str) -> Self {
        Self::from_git_letter(code.chars().next().unwrap_or(' '))
    }
}

#[cfg(test)]
mod tests {
    use super::{head_file_text, run_git, validate_object_id, NumstatRow, StatusKind};

    #[test]
    fn parses_a_plain_numstat_row() {
        let row = NumstatRow::parse("3\t2\tsrc/app.ts").expect("numstat row");
        assert_eq!(
            (row.additions, row.deletions, row.path),
            (3, 2, "src/app.ts")
        );
        assert!(!row.is_binary);
    }

    #[test]
    fn counts_a_binary_row_as_zero_lines() {
        let row = NumstatRow::parse("-\t-\tassets/logo.png").expect("numstat row");
        assert_eq!((row.additions, row.deletions), (0, 0));
        assert!(row.is_binary);
    }

    #[test]
    fn rejects_a_line_without_a_path_column() {
        assert!(NumstatRow::parse("3\t2").is_none());
        assert!(NumstatRow::parse("").is_none());
    }

    #[test]
    fn status_kinds_serialize_to_the_lowercase_wire_strings() {
        let kinds = [
            StatusKind::Created,
            StatusKind::Modified,
            StatusKind::Deleted,
            StatusKind::Renamed,
            StatusKind::Untracked,
        ];
        assert_eq!(
            serde_json::to_string(&kinds).expect("serialize kinds"),
            r#"["created","modified","deleted","renamed","untracked"]"#
        );
    }

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
