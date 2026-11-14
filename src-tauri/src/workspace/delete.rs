//! Pruning vanished projects and deleting a project or its directory.

use super::patient_io::leave_if_inside;
use super::patient_io::remove_dir_all_patiently;
use super::store::is_project;
use super::store::load;
use super::store::update_settings;
use super::store::Settings;
use super::temp::delete_unused_temps;
use super::temp::is_ade_owned;
use super::temp::require_ade_owned;
use std::path::Path;
use tauri::AppHandle;

/// Has this folder been deleted out from under us? Only if its parent is still
/// there: a project on a drive that is merely unplugged has a missing folder AND a
/// missing parent, and it keeps its place in the list rather than being forgotten
/// because someone pulled a USB stick.
pub(super) fn has_vanished(path: &str) -> bool {
    let path = Path::new(path);
    !path.exists() && path.parent().is_some_and(Path::exists)
}

/// Delete the temp workspaces that went unused past the cleanup window, forget
/// every remembered path whose folder is gone, and hand back the settings the
/// picker should now show. Called on every picker refresh - including the ones
/// its directory watcher triggers - so a workspace deleted in Explorer, by a
/// script, or from a terminal leaves the page like one deleted from the menu.
#[tauri::command]
pub(crate) async fn workspace_prune(app: AppHandle) -> Result<Settings, String> {
    delete_unused_temps(&crate::platform::window::open_projects(&app))?;
    update_settings(|settings| {
        settings
            .workspace_last_used
            .retain(|path, _| !has_vanished(path));
        settings.recent_projects.retain(|path| !has_vanished(path));
        settings.pinned_projects.retain(|path| !has_vanished(path));
        settings.owned_workspaces.retain(|path| !has_vanished(path));
        settings.labels.retain(|path, _| !has_vanished(path));
        Ok(())
    })
}

/// Remove `path` from disk (stepping the process out first, riding out the
/// Windows sharing race) and forget it from every list - recents, pins, owned,
/// and its label. Shared by the owned-only `workspace_delete` and the
/// confirmation-gated `workspace_delete_directory`.
pub(super) fn delete_directory(path: &str) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("only a real project directory can be deleted".into());
    }
    let target = std::fs::canonicalize(path).map_err(|error| error.to_string())?;
    if target.parent().is_none() {
        return Err("a filesystem root cannot be deleted".into());
    }
    leave_if_inside(&target);
    remove_dir_all_patiently(&target).map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn forget_directory(settings: &mut Settings, path: &str) {
    settings.recent_projects.retain(|entry| entry != path);
    settings.pinned_projects.retain(|entry| entry != path);
    settings.owned_workspaces.retain(|entry| entry != path);
    settings.labels.remove(path);
    settings.workspace_last_used.remove(path);
}

/// Delete an ADE-owned workspace directory and forget it.
#[tauri::command]
pub(crate) async fn workspace_delete(path: String) -> Result<Settings, String> {
    require_ade_owned(&load(), &path, "deleted")?;
    delete_directory(&path)?;
    update_settings(|settings| {
        forget_directory(settings, &path);
        Ok(())
    })
}

/// May `workspace_delete_directory` remove this directory? Only a project a
/// PADE surface actually shows: one it remembers (recent or pinned, the
/// switcher's lists), one it owns, or one its home page lists - a project
/// folder directly under a saved root, which is exactly what
/// `workspace_scan` returns. Anything else (an arbitrary path, a plain folder
/// under a root, a root itself) is refused, so this ungated removal can never
/// be pointed at a directory the user never saw as a project.
pub(super) fn is_deletable_project(settings: &Settings, path: &str, target: &Path) -> bool {
    if is_ade_owned(settings, path) {
        return true;
    }

    let is_remembered = settings
        .recent_projects
        .iter()
        .chain(&settings.pinned_projects)
        .filter_map(|remembered| std::fs::canonicalize(remembered).ok())
        .any(|remembered| remembered == target);
    if is_remembered {
        return true;
    }

    let is_scanned_under_a_root = settings
        .roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .any(|root| target.parent() == Some(root.as_path()));
    is_scanned_under_a_root && is_project(target)
}

/// Delete ANY project directory from disk and forget it - the switcher's and
/// the home page's "Delete directory" action. Unlike `workspace_delete` this
/// is not gated to ADE-owned workspaces, so it can remove a real project the
/// user points at; it is gated to projects a PADE surface shows (see
/// [`is_deletable_project`]), the UI raises an explicit, path-naming
/// confirmation before calling it, and the caller (the relocator) kills the
/// sessions holding the folder first.
#[tauri::command]
pub(crate) async fn workspace_delete_directory(path: String) -> Result<Settings, String> {
    let settings = load();
    let target = std::fs::canonicalize(&path).map_err(|error| error.to_string())?;
    if !is_deletable_project(&settings, &path, &target) {
        return Err(
            "only a project shown on the home page or in the switcher can be deleted".into(),
        );
    }
    delete_directory(&path)?;
    update_settings(|settings| {
        forget_directory(settings, &path);
        Ok(())
    })
}
