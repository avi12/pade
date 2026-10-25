//! Settings sync across PADE processes.
//!
//! Every window of one process hears a settings change through
//! `update_settings`' broadcast, but a second PADE process - a project opened
//! from Explorer's "Open in PADE", or PADE simply launched again - runs its own
//! event bus and shares nothing with this one except the settings file on disk.
//! So a temp workspace created or deleted over there never reached the rail
//! over here. Watching that file and re-broadcasting each write this process
//! did not announce itself brings every window of every instance onto the same
//! recents, pins, labels and workspaces, through the same `settings://changed`
//! path a local change takes.

use std::ffi::OsStr;
use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

/// The settings file text this process last announced - its own writes, and
/// the other processes' writes it has already passed on - so its own save, and
/// the burst of events one write raises, announce nothing a second time.
pub(super) struct Announced {
    text: Mutex<Option<String>>,
}

impl Announced {
    const fn new() -> Self {
        Self {
            text: Mutex::new(None),
        }
    }

    /// Remember `text` as announced: this process just wrote it and told its
    /// own windows.
    pub(super) fn record(&self, text: String) {
        *self.text.lock().unwrap_or_else(PoisonError::into_inner) = Some(text);
    }

    /// Is `text` news to this process? Remembers it when it is, so it is
    /// announced exactly once.
    fn is_news(&self, text: &str) -> bool {
        let mut announced = self.text.lock().unwrap_or_else(PoisonError::into_inner);
        if announced.as_deref() == Some(text) {
            return false;
        }

        *announced = Some(text.to_owned());
        true
    }
}

pub(super) static ANNOUNCED: Announced = Announced::new();

/// Kept alive for the life of the process - dropping a watcher stops it.
static WATCHER: OnceLock<Mutex<RecommendedWatcher>> = OnceLock::new();

/// Watch the settings file for writes from other PADE processes and announce
/// each one to this process's windows through `announce`. The directory is
/// watched rather than the file, because a save replaces the file (write a
/// temporary, rename it over), which a watch on the file itself would lose.
pub(super) fn watch(settings_file: &Path, announce: fn()) -> notify::Result<()> {
    let Some(directory) = settings_file.parent() else {
        return Ok(());
    };
    let file_name = settings_file.file_name().map(OsStr::to_owned);
    let settings_file = settings_file.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        let touches_settings = event
            .paths
            .iter()
            .any(|changed| changed.file_name() == file_name.as_deref());
        if touches_settings {
            announce_if_news(&settings_file, announce);
        }
    })?;
    watcher.watch(directory, RecursiveMode::NonRecursive)?;
    let _ = WATCHER.set(Mutex::new(watcher));
    Ok(())
}

fn announce_if_news(settings_file: &Path, announce: fn()) {
    // Unreadable mid-replace (Windows removes the old file before the rename
    // lands); the rename raises its own event, which reads the new text.
    let Ok(text) = std::fs::read_to_string(settings_file) else {
        return;
    };
    if ANNOUNCED.is_news(&text) {
        announce();
    }
}

#[cfg(test)]
mod tests {
    use super::Announced;

    #[test]
    fn a_write_from_another_process_is_news_once() {
        let announced = Announced::new();
        assert!(announced.is_news("{\"roots\":[]}"));
        assert!(
            !announced.is_news("{\"roots\":[]}"),
            "a burst of events for one write must announce it only once"
        );
        assert!(announced.is_news("{\"roots\":[\"root\"]}"));
    }

    #[test]
    fn this_process_own_write_is_not_news() {
        let announced = Announced::new();
        announced.record("{\"roots\":[\"root\"]}".to_string());
        assert!(
            !announced.is_news("{\"roots\":[\"root\"]}"),
            "the watcher must not re-announce what update_settings already did"
        );
    }
}
