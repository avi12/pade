//! Commit inspection - one commit's message, per-file stats, a file's diff, and
//! its rendered preview (picture, clip or document) as the commit left it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

use super::branches::current_branch;
use super::{
    commit_header_format, run_git, run_git_bytes, run_git_into, validate_object_id, CommitHeader,
    NumstatRow, StatusKind, FIELD_SEPARATOR,
};
use crate::platform::media::is_video;
use crate::platform::preview::{
    image_mime_type, preview_text, video_preview, FeedImage, FeedVideo, MAXIMUM_IMAGE_BYTES,
    MAXIMUM_PREVIEW_BYTES,
};

/// One file changed by a commit, with its per-file line counts.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFileEntry {
    path: String,
    kind: StatusKind,
    additions: u32,
    deletions: u32,
    /// True when git reports the file as binary (line counts are meaningless).
    binary: bool,
}

/// A single commit's full detail: message, author/date, branch, and per-file
/// stats. Shares the Log view's header fields through [`CommitHeader`].
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    #[serde(flatten)]
    header: CommitHeader,
    /// The commit message body (everything after the subject line); empty if none.
    body: String,
    /// The current HEAD branch name (empty on a detached HEAD).
    branch: String,
    files: Vec<CommitFileEntry>,
    additions: u32,
    deletions: u32,
}

#[tauri::command]
pub async fn vcs_commit(cwd: String, sha: String) -> Result<CommitDetail, String> {
    let sha = validate_object_id(&sha)?;
    // Header + full body in one shot: subject on its own line, then the body.
    let format = format!("--format={}{FIELD_SEPARATOR}%b", commit_header_format());
    let head = run_git(&cwd, &["show", "-s", &format, "--end-of-options", sha])?;
    let fields: Vec<&str> = head
        .trim_end_matches('\n')
        .splitn(6, FIELD_SEPARATOR)
        .collect();
    let Some((header, [body])) = CommitHeader::split_off(&fields) else {
        return Err("could not parse commit header".into());
    };

    // Both listings use `-z` so a renamed path arrives as its own NUL-separated
    // field (git otherwise packs it into "src/{old => new}/f.txt" brace notation,
    // which would poison the stored path and break vcs_commit_diff). --numstat's
    // record is "adds\tdels\tpath\0" normally, or "adds\tdels\t\0old\0new\0" for a
    // rename; --name-status is "code\0path\0" or "code\0old\0new\0".
    let show_listing = |listing: &str| {
        run_git(
            &cwd,
            &["show", listing, "-z", "--format=", "--end-of-options", sha],
        )
    };
    let numstat = show_listing("--numstat")?;
    let name_status = show_listing("--name-status")?;
    let kinds_by_path = status_kinds_by_path(&name_status);

    let files: Vec<CommitFileEntry> = parse_numstat_records(&numstat)
        .into_iter()
        .map(|record| CommitFileEntry {
            path: record.path.to_string(),
            kind: kinds_by_path
                .get(record.path)
                .copied()
                .unwrap_or(StatusKind::Modified),
            additions: record.additions,
            deletions: record.deletions,
            binary: record.is_binary,
        })
        .collect();

    Ok(CommitDetail {
        header,
        body: body.trim().to_string(),
        branch: current_branch(&cwd).unwrap_or_default(),
        additions: files.iter().map(|file| file.additions).sum(),
        deletions: files.iter().map(|file| file.deletions).sum(),
        files,
    })
}

/// Skip a `-z` rename's old path and return its new one - the path both
/// `--numstat` and `--name-status` key a renamed file on.
fn new_path_of_rename<'text>(fields: &mut impl Iterator<Item = &'text str>) -> Option<&'text str> {
    fields.next()?;
    fields.next()
}

/// Parse a `git show --numstat -z` stream into per-file rows. Fields are
/// NUL-separated: a normal record is one `"adds\tdels\tpath"` field; a rename is
/// `"adds\tdels\t"` followed by the old and new path in their own fields - we
/// take the NEW path so it matches the diff and name-status keys.
fn parse_numstat_records(numstat: &str) -> Vec<NumstatRow<'_>> {
    let mut records = Vec::new();
    let mut fields = numstat.split('\0').filter(|field| !field.is_empty());

    while let Some(head) = fields.next() {
        let Some(mut row) = NumstatRow::parse(head) else {
            continue;
        };
        let is_rename = row.path.is_empty();
        if is_rename {
            let Some(new_path) = new_path_of_rename(&mut fields) else {
                break;
            };
            row.path = new_path;
        }
        records.push(row);
    }
    records
}

/// Parse a `git show --name-status -z` stream into a `new path → StatusKind` map.
/// Fields are NUL-separated: a normal record is `"code"` then `"path"`; a rename
/// or copy is `"code"` then the old and new path - we key on the NEW path so it
/// matches the path `--numstat` reports.
fn status_kinds_by_path(name_status: &str) -> HashMap<&str, StatusKind> {
    let mut kinds = HashMap::new();
    let mut fields = name_status.split('\0').filter(|field| !field.is_empty());

    while let Some(code) = fields.next() {
        let kind = StatusKind::from_status_code(code);
        let path = if kind == StatusKind::Renamed {
            new_path_of_rename(&mut fields)
        } else {
            fields.next()
        };
        let Some(path) = path else {
            break;
        };
        kinds.insert(path, kind);
    }
    kinds
}

/// Raw unified diff for one path within a commit.
#[tauri::command]
pub async fn vcs_commit_diff(cwd: String, sha: String, path: String) -> Result<String, String> {
    let sha = validate_object_id(&sha)?;
    run_git(
        &cwd,
        &["show", "--no-color", "--end-of-options", sha, "--", &path],
    )
}

/// Largest committed video the commit view hands to ffmpeg. It is streamed to a
/// scratch file rather than held in memory, so the cap only keeps a giant
/// asset from filling the temp folder for a fifteen-second preview.
const MAXIMUM_VIDEO_BYTES: u64 = 512 * 1024 * 1024;

/// The git object holding `path` as this commit leaves it, with its size - or,
/// for a file the commit deleted, as its parent had it, so a deletion previews
/// what was removed. `path` is repository-relative, as `vcs_commit` lists it;
/// the spec starts with the validated id, so it can never read as an option.
fn commit_blob(cwd: &str, sha: &str, path: &str) -> Option<(String, u64)> {
    [format!("{sha}:{path}"), format!("{sha}^:{path}")]
        .into_iter()
        .find_map(|spec| {
            let size = run_git(cwd, &["cat-file", "-s", &spec]).ok()?;
            let size = size.trim().parse().ok()?;
            Some((spec, size))
        })
}

/// The bytes of `path` at the commit (see [`commit_blob`]), `None` when it has
/// no such file or the file is larger than `maximum_bytes`.
fn read_commit_blob(cwd: &str, sha: &str, path: &str, maximum_bytes: u64) -> Option<Vec<u8>> {
    let (spec, size) = commit_blob(cwd, sha, path)?;
    if size > maximum_bytes {
        return None;
    }
    run_git_bytes(cwd, &["cat-file", "blob", &spec]).ok()
}

/// A scratch copy of a committed file, removed when dropped.
struct ScratchFile(PathBuf);

impl Drop for ScratchFile {
    fn drop(&mut self) {
        // Already gone is fine; the temp folder is the OS's to sweep anyway.
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Copy `path` at the commit into a scratch file ffmpeg can open (it needs a
/// seekable input - an MP4's index often sits at its end). Blocking.
fn write_commit_blob(cwd: &str, sha: &str, path: &str) -> Option<ScratchFile> {
    static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

    let (spec, size) = commit_blob(cwd, sha, path)?;
    if size > MAXIMUM_VIDEO_BYTES {
        return None;
    }
    let extension = Path::new(path).extension()?.to_string_lossy();
    let scratch = ScratchFile(std::env::temp_dir().join(format!(
        "pade-commit-preview-{}-{}.{extension}",
        std::process::id(),
        NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
    )));
    let file = std::fs::File::create(&scratch.0).ok()?;
    run_git_into(cwd, &["cat-file", "blob", &spec], file).then_some(scratch)
}

/// The inline image preview of `path` as the commit left it - the commit
/// view's counterpart of `feed_image`. `None` when `path` is not a previewable
/// image, is absent from the commit, or is over [`MAXIMUM_IMAGE_BYTES`].
#[tauri::command]
pub async fn vcs_commit_image(
    cwd: String,
    sha: String,
    path: String,
) -> Result<Option<FeedImage>, String> {
    let sha = validate_object_id(&sha)?;
    if image_mime_type(Path::new(&path)).is_none() {
        return Ok(None);
    }
    Ok(read_commit_blob(&cwd, sha, &path, MAXIMUM_IMAGE_BYTES)
        .and_then(|bytes| FeedImage::of(Path::new(&path), &bytes)))
}

/// The video preview of `path` as the commit left it - the commit view's
/// counterpart of `feed_video`. The blob is copied out on a blocking thread and
/// transcoded like a changed file; `None` when it isn't a video, is absent from
/// the commit, or ffmpeg couldn't read it.
#[tauri::command]
pub async fn vcs_commit_video(
    cwd: String,
    sha: String,
    path: String,
) -> Result<Option<FeedVideo>, String> {
    let sha = validate_object_id(&sha)?.to_owned();
    if !is_video(Path::new(&path)) {
        return Ok(None);
    }
    let scratch =
        tauri::async_runtime::spawn_blocking(move || write_commit_blob(&cwd, &sha, &path))
            .await
            .map_err(|error| error.to_string())?;
    let Some(scratch) = scratch else {
        return Ok(None);
    };
    video_preview(scratch.0.clone()).await
}

/// The UTF-8 text of `path` as the commit left it, for a markdown/HTML file's
/// rendered Preview - the commit view's counterpart of `feed_text`. `None` when
/// it is absent from the commit, binary, or over [`MAXIMUM_PREVIEW_BYTES`].
#[tauri::command]
pub async fn vcs_commit_text(
    cwd: String,
    sha: String,
    path: String,
) -> Result<Option<String>, String> {
    let sha = validate_object_id(&sha)?;
    Ok(read_commit_blob(&cwd, sha, &path, MAXIMUM_PREVIEW_BYTES).and_then(preview_text))
}

#[cfg(test)]
mod tests {
    use super::{
        parse_numstat_records, read_commit_blob, run_git, status_kinds_by_path, write_commit_blob,
        StatusKind,
    };

    /// A scratch repository with two commits: the first adds `logo.png` and
    /// `notes.md`, the second deletes `logo.png`. `None` without git.
    fn scratch_repository(name: &str) -> Option<(std::path::PathBuf, String, String)> {
        let root = std::env::temp_dir().join(format!("pade-inspect-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).ok()?;
        let cwd = root.to_string_lossy().to_string();
        let git = |arguments: &[&str]| run_git(&cwd, arguments);
        git(&["init", "-q"]).ok()?;
        std::fs::write(root.join("logo.png"), b"\x89PNG bytes").ok()?;
        std::fs::write(root.join("notes.md"), "# Notes").ok()?;
        git(&["add", "."]).ok()?;
        let identity = [
            "-c",
            "user.name=PADE",
            "-c",
            "user.email=pade@example.invalid",
        ];
        git(&[&identity[..], &["commit", "-qm", "add"]].concat()).ok()?;
        let added = git(&["rev-parse", "HEAD"]).ok()?.trim().to_string();
        git(&["rm", "-q", "logo.png"]).ok()?;
        git(&[&identity[..], &["commit", "-qm", "remove"]].concat()).ok()?;
        let removed = git(&["rev-parse", "HEAD"]).ok()?.trim().to_string();
        Some((root, added, removed))
    }

    #[test]
    fn a_committed_file_reads_back_byte_for_byte() {
        let Some((root, added, _)) = scratch_repository("read") else {
            return;
        };
        let cwd = root.to_string_lossy().to_string();
        let image = read_commit_blob(&cwd, &added, "logo.png", 1024);
        let text = read_commit_blob(&cwd, &added, "notes.md", 1024);
        let oversized = read_commit_blob(&cwd, &added, "logo.png", 4);
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(image.as_deref(), Some(b"\x89PNG bytes".as_slice()));
        assert_eq!(text.as_deref(), Some(b"# Notes".as_slice()));
        assert_eq!(oversized, None);
    }

    #[test]
    fn a_deleted_file_reads_as_its_parent_had_it() {
        let Some((root, _, removed)) = scratch_repository("deleted") else {
            return;
        };
        let cwd = root.to_string_lossy().to_string();
        let image = read_commit_blob(&cwd, &removed, "logo.png", 1024);
        let missing = read_commit_blob(&cwd, &removed, "absent.png", 1024);
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(image.as_deref(), Some(b"\x89PNG bytes".as_slice()));
        assert_eq!(missing, None);
    }

    #[test]
    fn a_scratch_copy_holds_the_blob_and_is_removed_when_dropped() {
        let Some((root, added, _)) = scratch_repository("scratch") else {
            return;
        };
        let cwd = root.to_string_lossy().to_string();
        let scratch = write_commit_blob(&cwd, &added, "logo.png").expect("copy the blob out");
        let copied = std::fs::read(&scratch.0).ok();
        let scratch_path = scratch.0.clone();
        drop(scratch);
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(copied.as_deref(), Some(b"\x89PNG bytes".as_slice()));
        assert!(!scratch_path.exists());
    }

    #[test]
    fn a_plain_numstat_record_keeps_its_path_and_counts() {
        let records = parse_numstat_records("3\t2\tsrc/app.ts\0");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].path, "src/app.ts");
        assert_eq!(records[0].additions, 3);
        assert_eq!(records[0].deletions, 2);
        assert!(!records[0].is_binary);
    }

    #[test]
    fn a_rename_record_takes_the_new_path() {
        let records = parse_numstat_records("1\t1\t\0src/old.ts\0src/new.ts\0");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].path, "src/new.ts");
    }

    #[test]
    fn a_binary_record_is_marked_binary_with_zero_counts() {
        let records = parse_numstat_records("-\t-\tassets/logo.png\0");
        assert!(records[0].is_binary);
        assert_eq!(records[0].additions, 0);
        assert_eq!(records[0].deletions, 0);
    }

    #[test]
    fn name_status_keys_a_rename_on_the_new_path() {
        let kinds = status_kinds_by_path("M\0src/app.ts\0R100\0src/old.ts\0src/new.ts\0");
        assert_eq!(kinds.get("src/app.ts"), Some(&StatusKind::Modified));
        assert_eq!(kinds.get("src/new.ts"), Some(&StatusKind::Renamed));
        assert!(!kinds.contains_key("src/old.ts"));
    }

    #[test]
    fn status_codes_map_to_kinds() {
        assert_eq!(StatusKind::from_status_code("A"), StatusKind::Created);
        assert_eq!(StatusKind::from_status_code("D"), StatusKind::Deleted);
        assert_eq!(StatusKind::from_status_code("R87"), StatusKind::Renamed);
        assert_eq!(StatusKind::from_status_code("C50"), StatusKind::Renamed);
        assert_eq!(StatusKind::from_status_code("M"), StatusKind::Modified);
    }
}
