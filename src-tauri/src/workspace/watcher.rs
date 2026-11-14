//! Filesystem watcher feeding the Change Feed.
//!
//! MVP: watches the opened project, ignores build/VCS noise, and turns each
//! save into a `ChangeEvent` with a line-count delta and a heuristic summary.
//! Later: real per-hunk diffs and agent-authored intent replace the heuristic.

mod registry;
pub(crate) use registry::release_watches_under;
pub(crate) mod arm;
mod change;
pub(crate) mod directories;
mod events;
pub(crate) mod feed;
mod git_state;
mod ignore;
pub(crate) mod languages;
mod mcp;
#[cfg(test)]
mod tests;

use registry::WatcherState;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use tauri::AppHandle;
use tauri::Manager;

static CHANGE_EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// How long after one event for a path its next one is dropped: editors emit a
/// burst of events per save.
const SAVE_BURST_DEBOUNCE: Duration = Duration::from_millis(150);

pub(crate) fn init(app: &AppHandle) {
    app.manage(WatcherState::default());
}
