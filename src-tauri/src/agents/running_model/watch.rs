//! The watch that tells every window a session's model or effort may have
//! moved: PADE's own per-session state files, each agent's shared settings
//! folder and Copilot's session folders, each place told apart so a changed
//! path names the session it speaks for.

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager};

use super::{claude, codex, copilot, hook_sink, state_directory, STATE_FILE_EXTENSION};

/// Tells every window a session's model or effort may have changed. The payload
/// names the conversation, or is null when a settings file every session reads
/// changed.
const CHANGED_EVENT: &str = "agent-model://changed";
/// How long after an agent saves its settings its transcript has caught up.
const TRANSCRIPT_LOG_SETTLE: Duration = Duration::from_secs(1);
/// A session file outlives its session so a resumed conversation still has its
/// model; past this age nobody will resume it.
const STATE_RETENTION: Duration = Duration::from_hours(14 * 24);

/// Watch every place a session's model or effort is written, telling the
/// windows the moment one may have moved. Best-effort: without the watch a tab
/// still catches up when its session next goes idle.
pub(crate) fn init(app: &AppHandle) {
    std::thread::spawn(hook_sink::refresh);
    let Some(states) = state_directory() else {
        return;
    };
    if let Err(error) = std::fs::create_dir_all(&states) {
        eprintln!("pade: no agent model-state directory: {error}");
        return;
    }
    prune_expired(&states);

    let places = watched_places(states);
    let emitter = app.clone();
    let routes = places.clone();
    let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        for path in &event.paths {
            let Some(payload) = changed_payload(path, &routes) else {
                continue;
            };
            let is_shared_settings = payload.session_id.is_none();
            let _ = emitter.emit(CHANGED_EVENT, payload.clone());
            if is_shared_settings {
                announce_again_once_logged(&emitter, payload);
            }
        }
    });
    let Ok(mut watcher) = watcher else {
        return;
    };
    for (directory, place) in &places {
        // Best-effort per place: an agent that isn't installed has no folder.
        let _ = watcher.watch(directory, place.recursive_mode());
    }
    app.manage(ModelWatch {
        _watcher: Mutex::new(watcher),
    });
}

/// `/effort` saves the settings file and logs the level to the session's
/// transcript, which can land a moment later and fires no event of its own -
/// so a settings change is announced once more after that moment.
fn announce_again_once_logged(emitter: &AppHandle, payload: ChangedPayload) {
    let emitter = emitter.clone();
    std::thread::spawn(move || {
        std::thread::sleep(TRANSCRIPT_LOG_SETTLE);
        let _ = emitter.emit(CHANGED_EVENT, payload);
    });
}

/// Keeps the watch alive for the app's lifetime; dropping it ends the watch.
struct ModelWatch {
    _watcher: Mutex<RecommendedWatcher>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangedPayload {
    session_id: Option<String>,
}

/// What one watched place is, so a changed path can be told apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchedPlace {
    /// PADE's own per-session state files.
    SessionStates,
    /// A settings file every session of an agent reads.
    SharedSettings,
    /// An agent's own per-session folders (Copilot's session-state).
    SessionFolders,
}

impl WatchedPlace {
    /// Only per-session folders nest; everything else is one flat directory.
    fn recursive_mode(self) -> RecursiveMode {
        if self == Self::SessionFolders {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        }
    }
}

/// Every directory to watch: PADE's own state files, each agent's settings
/// folder, and Copilot's session folders.
fn watched_places(session_states: PathBuf) -> Vec<(PathBuf, WatchedPlace)> {
    let mut places = vec![(session_states, WatchedPlace::SessionStates)];
    let settings_directories = [claude::settings_directory(), codex::settings_directory()];
    for directory in settings_directories.into_iter().flatten() {
        places.push((directory, WatchedPlace::SharedSettings));
    }
    if let Some(root) = copilot::session_root() {
        places.push((root, WatchedPlace::SessionFolders));
    }
    places
}

/// Which session a changed path speaks for, or `None` when it is noise.
fn changed_payload(path: &Path, places: &[(PathBuf, WatchedPlace)]) -> Option<ChangedPayload> {
    let name = path.file_name()?.to_str()?;
    let place = places
        .iter()
        .find(|(directory, _)| path.starts_with(directory))
        .map(|(_, place)| *place)?;
    match place {
        WatchedPlace::SessionStates => {
            let session_id = name.strip_suffix(STATE_FILE_EXTENSION)?;
            Some(ChangedPayload {
                session_id: Some(session_id.to_string()),
            })
        }
        WatchedPlace::SharedSettings => {
            let is_settings = claude::is_settings_file(name) || codex::is_settings_file(name);
            is_settings.then_some(ChangedPayload { session_id: None })
        }
        WatchedPlace::SessionFolders => {
            let session_id = copilot::session_of_event_log(path)?;
            Some(ChangedPayload {
                session_id: Some(session_id),
            })
        }
    }
}

fn prune_expired(directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let is_expired = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STATE_RETENTION);
        if is_expired {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
