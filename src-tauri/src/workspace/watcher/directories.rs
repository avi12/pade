//! Directory-listing watches behind the file browser.

use super::registry::prune_closed_windows;
use super::registry::WatcherState;
use notify::event::ModifyKind;
use notify::Event;
use notify::EventKind;
use notify::RecursiveMode;
use notify::Watcher;
use std::path::Path;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::State;
use tauri::WebviewWindow;

/// Announces that a picker-watched folder's contents changed (a project row
/// appeared or disappeared). Payload-free. This name is the frontend contract
/// (`src/lib/bridge.ts`).
pub(super) const DIRECTORIES_CHANGED_EVENT: &str = "directories://changed";

/// Watch a set of folders (non-recursively) and report when anything inside one
/// appears or disappears - the project picker's eyes. It hands over the *parents*
/// of the rows it shows, never the rows themselves: a watch holds a handle on the
/// folder it watches, and a handle on a project would be the very thing stopping
/// that project from being deleted. Re-arming replaces the previous set.
#[tauri::command]
pub async fn watch_dirs(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    dirs: Vec<String>,
) -> Result<(), String> {
    let label = window.label().to_string();
    prune_closed_windows(&app, &state);
    let mut guard = state
        .directory_watchers
        .lock()
        .map_err(|error| error.to_string())?;
    // Drop this window's old watcher before opening the new one, so its handles
    // go with it; another window's picker watch is untouched.
    guard.remove(&label);
    if dirs.is_empty() {
        return Ok(());
    }

    let app_handle = app.clone();
    let callback_label = label.clone();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        // Only appearance/disappearance moves rows on the page; edits inside a
        // project are the Change Feed's business, not the picker's. A rename
        // counts - the old name is gone - and so does Explorer's delete, which
        // moves the folder into the Recycle Bin. Routed to the owning window
        // alone so a sibling's picker doesn't rescan on our news.
        let is_appearance_change = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
        );
        if is_appearance_change {
            let _ = app_handle.emit_to(&callback_label, DIRECTORIES_CHANGED_EVENT, ());
        }
    })
    .map_err(|error| error.to_string())?;

    for directory in dirs {
        // A folder that is already gone isn't an error - that's precisely the news
        // the caller is listening for, and the next re-arm won't ask for it again.
        let _ = watcher.watch(Path::new(&directory), RecursiveMode::NonRecursive);
    }

    guard.insert(label, watcher);
    Ok(())
}
