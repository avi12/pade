//! Recent projects and pins: opening a project records it, leaving stamps it, and the pinned list and its order.

use super::roots::canonical_path;
use super::store::update_settings;
use super::store::Settings;
use crate::util::now_milliseconds;

/// Push a path to the front of the recent list (canonicalized, deduped). The list
/// is unbounded - every project ever opened stays until removed or cleared; the
/// switcher keeps a long list cheap by loading each row's details only on screen.
pub(super) fn record_recent(settings: &mut Settings, path: &str) {
    let path = canonical_path(path);
    stamp_used(settings, &path);
    settings.recent_projects.retain(|project| project != &path);
    settings.recent_projects.insert(0, path);
}

pub(super) const MILLISECONDS_PER_DAY: u64 = 24 * 60 * 60 * 1000;

/// Record that a project was just used - its "last accessed" time, and for a
/// temp workspace the restart of its cleanup clock.
pub(super) fn stamp_used(settings: &mut Settings, path: &str) {
    settings
        .workspace_last_used
        .insert(canonical_path(path), now_milliseconds());
}

/// A window just left `path` (switched away or closed), so its last access is
/// now rather than when it was opened.
pub(crate) fn mark_left(path: &str) -> Result<(), String> {
    update_settings(|settings| {
        stamp_used(settings, path);
        Ok(())
    })?;
    Ok(())
}

/// Open a project: point the process (and thus the watcher/VCS/agent) at it and
/// remember it in the recent history.
#[tauri::command]
pub(crate) fn workspace_open(path: String) -> Result<(), String> {
    std::env::set_current_dir(&path).map_err(|error| error.to_string())?;
    update_settings(|settings| {
        record_recent(settings, &path);
        Ok(())
    })?;
    Ok(())
}

/// Clear the recent-projects history.
#[tauri::command]
pub(crate) fn workspace_clear_recent() -> Result<Settings, String> {
    update_settings(|settings| {
        settings.recent_projects.clear();
        Ok(())
    })
}

/// Pin or unpin a project in the switcher. Pinning moves it to the front of the
/// pinned list; unpinning drops it. Returns the refreshed settings.
#[tauri::command]
pub(crate) fn workspace_set_pinned(path: String, pinned: bool) -> Result<Settings, String> {
    update_settings(|settings| {
        settings
            .pinned_projects
            .retain(|pinned_path| pinned_path != &path);
        if pinned {
            settings.pinned_projects.insert(0, path);
        }
        Ok(())
    })
}

/// Forget a project from the switcher - drop it from the recent history and, if
/// pinned, from the pinned list too (a pin outlives recents, so removing only the
/// recent entry would leave the row still showing). The folder on disk is
/// untouched, and its display label is kept so a later re-open keeps the friendly
/// name. Returns the refreshed settings.
#[tauri::command]
pub(crate) fn workspace_remove_recent(path: String) -> Result<Settings, String> {
    update_settings(|settings| {
        settings.recent_projects.retain(|entry| entry != &path);
        settings.pinned_projects.retain(|entry| entry != &path);
        Ok(())
    })
}

/// Replace the pinned-project order with `paths` - a drag-reorder of the existing
/// pins. Reconciles rather than trusting the client: only already-pinned paths are
/// kept (this reorders, it never adds a pin - that stays `workspace_set_pinned`),
/// and any current pin the caller omitted is appended in its existing order, so a
/// list that raced with a toggle in another window can't silently drop a pin.
#[tauri::command]
pub(crate) fn workspace_set_pinned_order(paths: Vec<String>) -> Result<Settings, String> {
    update_settings(|settings| {
        let mut reordered: Vec<String> = paths
            .into_iter()
            .filter(|path| settings.pinned_projects.contains(path))
            .collect();
        for pinned in &settings.pinned_projects {
            if !reordered.contains(pinned) {
                reordered.push(pinned.clone());
            }
        }
        settings.pinned_projects = reordered;
        Ok(())
    })
}
