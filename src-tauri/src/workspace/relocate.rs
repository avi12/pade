//! Moving and renaming a workspace folder, then re-keying every path-keyed setting to the new path.

use super::patient_io::leave_if_inside;
use super::patient_io::rename_patiently;
use super::refs;
use super::roots::validated_child_path;
use super::store::load;
use super::store::update_settings;
use super::store::Settings;
use super::temp::require_ade_owned;
use super::watcher;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use tauri::AppHandle;
use tauri::Emitter;

/// The frontend-owned preferences kept per project, keyed by its path.
pub(super) const PROJECT_KEYED_PREFERENCES: [&str; 2] =
    ["agentModelProjects", "aiSessionNamingProjects"];

/// Re-point every setting that names a project by path - the lists, and each
/// per-project map's key - from the moved folder `from` (or a folder inside it)
/// to where it now lives under `to`.
pub(super) fn retarget(settings: &mut Settings, from: &str, to: &str) {
    for list in [
        &mut settings.recent_projects,
        &mut settings.owned_workspaces,
        &mut settings.pinned_projects,
    ] {
        for entry in list.iter_mut() {
            if let Some(moved) = refs::moved_path(entry, from, to) {
                *entry = moved;
            }
        }
    }
    rekey_moved(&mut settings.labels, from, to);
    rekey_moved(&mut settings.workspace_last_used, from, to);
    rekey_moved(&mut settings.project_agents, from, to);
    rekey_moved(&mut settings.prefs.ide_project_choices, from, to);
    for preference in PROJECT_KEYED_PREFERENCES {
        let Some(serde_json::Value::Object(projects)) =
            settings.prefs.passthrough.get_mut(preference)
        else {
            continue;
        };
        for (key, moved) in moved_keys(projects.keys(), from, to) {
            if let Some(value) = projects.remove(&key) {
                projects.insert(moved, value);
            }
        }
    }
}

/// `map` with every key under the moved folder re-keyed to its new path.
pub(super) fn rekey_moved<V>(map: &mut BTreeMap<String, V>, from: &str, to: &str) {
    for (key, moved) in moved_keys(map.keys(), from, to) {
        if let Some(value) = map.remove(&key) {
            map.insert(moved, value);
        }
    }
}

/// Each of `keys` under the moved folder, paired with its new path.
pub(super) fn moved_keys<'a>(
    keys: impl Iterator<Item = &'a String>,
    from: &str,
    to: &str,
) -> Vec<(String, String)> {
    keys.filter_map(|key| Some((key.clone(), refs::moved_path(key, from, to)?)))
        .collect()
}

/// Move `from` into `dest_dir` (keeping its folder name). The result is a normal
/// directory - no longer "temp" - but stays ADE-owned so it's still deletable.
#[tauri::command]
pub(crate) async fn workspace_move(
    app: AppHandle,
    from: String,
    dest_dir: String,
) -> Result<String, String> {
    require_ade_owned(&load(), &from, "moved")?;
    let name = Path::new(&from)
        .file_name()
        .ok_or("bad source path")?
        .to_owned();
    let destination = Path::new(&dest_dir).join(name);
    relocate_folder(&app, &from, &destination, MovedLabel::Keep).await
}

/// Rename the open workspace's folder where it stands - any project, not only
/// one PADE made - carrying every agent's memory of it and PADE's own settings
/// along. The frontend first stops the sessions working inside it: Windows
/// refuses to rename a folder a running process stands in.
#[tauri::command]
pub(crate) async fn workspace_rename_folder(
    app: AppHandle,
    from: String,
    new_name: String,
) -> Result<String, String> {
    let parent = Path::new(&from)
        .parent()
        .ok_or("a drive's root folder can't be renamed")?;
    let destination = validated_child_path(parent, &new_name)?;
    relocate_folder(&app, &from, &destination, MovedLabel::Drop).await
}

/// The steps a folder move goes through. Each is told to the frontend as it
/// starts: re-pointing everything that knows the folder can take a while on a
/// large project, and a prompt that only says "Renaming…" reads as stuck.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RelocateStep {
    Renaming,
    AgentMemories,
    EditorRecents,
    Links,
    Settings,
}

/// Where a folder move is, keyed by the folder it started from.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RelocateProgress<'a> {
    from: &'a str,
    step: RelocateStep,
}

pub(super) const RELOCATE_PROGRESS_EVENT: &str = "relocate://progress";

/// Whether a moved workspace keeps its display label. A rename makes the chosen
/// folder name the one shown, so a label carried over would shadow it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MovedLabel {
    Keep,
    Drop,
}

/// Move the folder `from` to `destination` and everything that knows it by
/// path along with it: PADE's own watch lets go first, then each agent's
/// memory, the IDEs' recents and PADE's settings follow. It is not opened: a
/// folder renamed from a list keeps its place in it (`retarget` re-keys the
/// entries it had) rather than jumping into the recents, and a window that had
/// it open re-points itself.
/// Off the async runtime: the rename retries while Windows closes handles, and
/// the agents' memories are files and databases.
pub(super) async fn relocate_folder(
    app: &AppHandle,
    from: &str,
    destination: &Path,
    label: MovedLabel,
) -> Result<String, String> {
    let app = app.clone();
    let from = from.to_string();
    let destination = destination.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        relocate_folder_now(&app, &from, &destination, label)
    })
    .await
    .map_err(|error| error.to_string())?
}

pub(super) fn relocate_folder_now(
    app: &AppHandle,
    from: &str,
    destination: &Path,
    label: MovedLabel,
) -> Result<String, String> {
    let source = Path::new(from);
    // A change of letter case alone renames the folder onto itself.
    let is_same_folder = std::fs::canonicalize(source)
        .ok()
        .zip(std::fs::canonicalize(destination).ok())
        .is_some_and(|(source, target)| source == target);
    if destination.exists() && !is_same_folder {
        return Err(format!("{} already exists", destination.display()));
    }

    let report = |step| {
        let _ = app.emit(RELOCATE_PROGRESS_EVENT, RelocateProgress { from, step });
    };
    report(RelocateStep::Renaming);
    watcher::release_watches_under(app, source);
    leave_if_inside(source);
    rename_patiently(source, destination).map_err(|error| error.to_string())?;
    let destination_string = destination.to_string_lossy().into_owned();
    refs::update_references(from, &destination_string, &report);
    report(RelocateStep::Settings);
    update_settings(|settings| {
        retarget(settings, from, &destination_string);
        if label == MovedLabel::Drop {
            settings.labels.remove(&destination_string);
        }
        Ok(())
    })?;
    Ok(destination_string)
}
