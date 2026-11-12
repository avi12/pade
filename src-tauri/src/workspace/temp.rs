//! Temp workspaces and ownership: which folders PADE created, which it may move or delete, and the expiry of unused temps.

use super::delete::delete_directory;
use super::recents::workspace_open;
use super::recents::MILLISECONDS_PER_DAY;
use super::roots::canonical_path;
use super::store::ensure_config_directory;
use super::store::load;
use super::store::update_settings;
use super::store::Settings;
use super::store::TEMP_WORKSPACE_PREFIX;
use super::store::WORKSPACES_DIRECTORY;
use crate::util::now_milliseconds;
use std::path::Path;
use std::path::PathBuf;

/// A path directly under the config `.../workspaces/temp-*` is one ADE created,
/// even if predating the `owned_workspaces` list.
pub(super) fn is_temp_workspace(path: &str) -> bool {
    has_temp_workspace_name(path)
        && canonical_workspaces_directory()
            .is_some_and(|workspaces| is_temp_workspace_in(path, &workspaces))
}

pub(super) fn has_temp_workspace_name(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(TEMP_WORKSPACE_PREFIX))
}

/// The temp-workspace directory, resolved through the filesystem so a
/// candidate's canonical parent can be compared against it.
pub(super) fn canonical_workspaces_directory() -> Option<PathBuf> {
    workspaces_directory()
        .ok()
        .and_then(|workspaces| std::fs::canonicalize(workspaces).ok())
}

/// [`is_temp_workspace`] against an already-resolved workspaces directory, so a
/// sweep over many folders resolves it once.
pub(super) fn is_temp_workspace_in(path: &str, canonical_workspaces: &Path) -> bool {
    has_temp_workspace_name(path)
        && std::fs::canonicalize(path)
            .is_ok_and(|target| target.parent() == Some(canonical_workspaces))
}

/// May ADE rename/move/delete this path? Only its own workspaces - never a real
/// project the user owns.
pub(super) fn is_ade_owned(settings: &Settings, path: &str) -> bool {
    settings
        .owned_workspaces
        .iter()
        .any(|workspace| workspace == path)
        || is_temp_workspace(path)
}

/// Refuse an operation on a workspace ADE did not create, naming the refused
/// operation in the message the UI shows (e.g. `"moved"`).
pub(super) fn require_ade_owned(
    settings: &Settings,
    path: &str,
    refused: &str,
) -> Result<(), String> {
    if is_ade_owned(settings, path) {
        return Ok(());
    }
    Err(format!("only ADE-created workspaces can be {refused}"))
}

/// Public ownership check for the naming module: is this an ADE-owned workspace
/// (so it's safe to scan and label)?
pub fn is_owned(path: &str) -> bool {
    is_ade_owned(&load(), path)
}

/// The directory every temp workspace is created in.
pub(super) fn workspaces_directory() -> Result<PathBuf, String> {
    Ok(ensure_config_directory()?.join(WORKSPACES_DIRECTORY))
}

/// Every temp workspace folder on disk, whether or not the lists remember it.
pub(super) fn temp_workspaces_on_disk() -> Vec<String> {
    let Ok(entries) = workspaces_directory()
        .and_then(|directory| std::fs::read_dir(directory).map_err(|error| error.to_string()))
    else {
        return Vec::new();
    };
    let Some(canonical_workspaces) = canonical_workspaces_directory() else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| canonical_path(&entry.path().to_string_lossy()))
        .filter(|path| is_temp_workspace_in(path, &canonical_workspaces))
        .collect()
}

/// The temp workspaces past the cleanup window: last used more than `days` ago,
/// and neither open in a window nor pinned. A temp with no recorded use yet is
/// never expired - the sweep starts its clock instead.
pub(super) fn expired_temps(
    settings: &Settings,
    temps: &[String],
    open_projects: &[String],
    now: u64,
) -> Vec<String> {
    let Some(days) = settings.prefs.temp_cleanup_days else {
        return Vec::new();
    };
    let window = u64::from(days) * MILLISECONDS_PER_DAY;
    let is_open = |path: &str| {
        open_projects
            .iter()
            .any(|open| crate::util::same_path(open, path))
    };
    let is_pinned = |path: &str| {
        settings
            .pinned_projects
            .iter()
            .any(|pinned| crate::util::same_path(pinned, path))
    };
    temps
        .iter()
        .filter(|path| {
            settings
                .workspace_last_used
                .get(path.as_str())
                .is_some_and(|last_used| now.saturating_sub(*last_used) > window)
        })
        .filter(|path| !is_open(path) && !is_pinned(path))
        .cloned()
        .collect()
}

/// Delete every temp workspace that has gone unused past the user's cleanup
/// window. A temp seen for the first time gets its clock started now, so
/// turning the setting on never deletes anything right away. A folder that
/// cannot be removed yet (a process still holds it) is retried on the next
/// sweep.
pub(super) fn delete_unused_temps(open_projects: &[String]) -> Result<(), String> {
    let temps = temp_workspaces_on_disk();
    let now = now_milliseconds();
    let settings = update_settings(|settings| {
        for temp in &temps {
            settings
                .workspace_last_used
                .entry(temp.clone())
                .or_insert(now);
        }
        Ok(())
    })?;
    for path in expired_temps(&settings, &temps, open_projects, now) {
        if let Err(error) = delete_directory(&path) {
            eprintln!("[pade] unused temp workspace {path} not deleted yet: {error}");
        }
    }
    Ok(())
}

/// Create a throwaway workspace so the user can start coding immediately without
/// choosing a project, then switch to a real one whenever they like.
#[tauri::command]
pub async fn workspace_temp() -> Result<String, String> {
    let directory =
        workspaces_directory()?.join(format!("{TEMP_WORKSPACE_PREFIX}{}", now_milliseconds()));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.to_string_lossy().into_owned();

    // Mark it ADE-owned so it can later be renamed/moved/deleted.
    update_settings(|settings| {
        if !settings.owned_workspaces.contains(&path) {
            settings.owned_workspaces.push(path.clone());
        }
        Ok(())
    })?;

    workspace_open(path.clone())?;
    Ok(path)
}
