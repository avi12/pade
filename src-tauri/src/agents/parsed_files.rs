//! What parsing an agent's file found, remembered until the file changes.
//!
//! The sidebar re-lists a project's recorded conversations about once a second
//! while an agent works, and every listing would otherwise re-open and re-parse
//! the newest few hundred transcripts of each agent - most of them untouched
//! since the last listing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// One parse result per file, kept against the modified time it was read at.
pub(crate) struct ParsedFiles<T>(Mutex<HashMap<PathBuf, (SystemTime, T)>>);

impl<T> Default for ParsedFiles<T> {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

impl<T: Clone> ParsedFiles<T> {
    /// What `parse` returned for `path` the last time it had this modified
    /// time, else a fresh parse (remembered for next time). A poisoned lock only
    /// costs the reuse - the file is parsed as if seen for the first time.
    pub(crate) fn get_or_parse(
        &self,
        path: &Path,
        modified: SystemTime,
        parse: impl FnOnce(&Path) -> T,
    ) -> T {
        let remembered = self.0.lock().ok().and_then(|entries| {
            entries
                .get(path)
                .filter(|(parsed_at, _)| *parsed_at == modified)
                .map(|(_, value)| value.clone())
        });
        if let Some(value) = remembered {
            return value;
        }

        let value = parse(path);
        if let Ok(mut entries) = self.0.lock() {
            entries.insert(path.to_path_buf(), (modified, value.clone()));
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use super::ParsedFiles;

    #[test]
    fn an_unchanged_file_is_parsed_once() {
        let files = ParsedFiles::default();
        let parses = Cell::new(0);
        let parse = |_: &Path| {
            parses.set(parses.get() + 1);
            "title".to_string()
        };
        let path = Path::new("a.jsonl");
        let modified = SystemTime::UNIX_EPOCH;
        assert_eq!(files.get_or_parse(path, modified, parse), "title");
        assert_eq!(files.get_or_parse(path, modified, parse), "title");
        assert_eq!(parses.get(), 1);
    }

    #[test]
    fn a_file_written_since_is_parsed_again() {
        let files = ParsedFiles::default();
        let path = Path::new("a.jsonl");
        let first = SystemTime::UNIX_EPOCH;
        let later = first + Duration::from_secs(1);
        assert_eq!(files.get_or_parse(path, first, |_| 1), 1);
        assert_eq!(files.get_or_parse(path, later, |_| 2), 2);
    }
}
