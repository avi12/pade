//! File helpers: the newest match in a log's tail, and a write that skips unchanged contents.

use std::path::Path;

/// How much of an append-only log's tail to scan for the newest entry that
/// answers, widening only when the smaller read came up empty. An entry is
/// usually a few KB, but one carrying a large tool result can be far longer -
/// and these scans run on every idle, so the common case must not pull a
/// multi-megabyte transcript into memory.
const TAIL_SCAN_BYTES: [u64; 3] = [64 * 1024, 1024 * 1024, 8 * 1024 * 1024];

/// What `find` reads off the newest entry of a JSON-lines log that carries it,
/// found by scanning backwards from the end so the answer costs a tail read
/// rather than a full parse of a file that grows all session long. `find` gets
/// the tail as text and should search its lines newest first.
pub(crate) fn newest_in_tail<T>(log: &Path, find: impl Fn(&str) -> Option<T>) -> Option<T> {
    let length = std::fs::metadata(log).ok()?.len();
    for scan_bytes in TAIL_SCAN_BYTES {
        let tail = read_tail(log, scan_bytes)?;
        if let Some(found) = find(&tail) {
            return Some(found);
        }

        let has_read_whole_file = scan_bytes >= length;
        if has_read_whole_file {
            return None;
        }
    }

    None
}

/// The last `bytes` of a file, lossily decoded. The read can start mid-line and
/// even mid-character; both are harmless, because callers parse whole JSON
/// lines and a truncated first line simply fails to parse.
fn read_tail(log: &Path, bytes: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(log).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(bytes)))
        .ok()?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;
    Some(String::from_utf8_lossy(&buffer).into_owned())
}

/// Write `contents` to `path` only when the file is missing or differs, so a
/// spawn never churns the disk (or file-watcher events) with identical bytes.
pub(crate) fn write_if_stale(path: &Path, contents: &str) -> std::io::Result<()> {
    let is_current = std::fs::read_to_string(path).is_ok_and(|existing| existing == contents);
    if is_current {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
}
