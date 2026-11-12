//! Saving a temp workspace as a real project: the destination root, the copy that skips dependencies, and the rename.

use super::provision;
use super::recents::record_recent;
use super::refs;
use super::relocate::relocate_folder;
use super::relocate::MovedLabel;
use super::roots::canonical_path;
use super::roots::validated_child_path;
use super::store::load;
use super::store::update_settings;
use super::store::Settings;
use super::temp::require_ade_owned;
use serde::Serialize;
use std::path::Path;
use tauri::AppHandle;

/// Resolve which saved root a save/promote lands in. `Some(chosen)` must match
/// one of `settings.roots` (compared canonically); `None` falls back to the
/// primary root (`roots[0]`). Shared by `workspace_rename` and
/// `workspace_save_migrate` so the two can't resolve the destination differently.
pub(super) fn resolve_save_root(
    settings: &Settings,
    root: Option<String>,
) -> Result<String, String> {
    match root {
        Some(chosen) => {
            let chosen = canonical_path(&chosen);
            settings
                .roots
                .iter()
                .find(|saved| canonical_path(saved) == chosen)
                .ok_or_else(|| "the chosen destination is not a saved root".to_string())
                .cloned()
        }
        None => settings
            .roots
            .first()
            .ok_or_else(|| {
                "add a root folder first - rename saves into the primary root".to_string()
            })
            .cloned(),
    }
}

/// Rename a temp workspace, promoting it into a saved project root under the new
/// name - turning it into a real project. `root` picks which saved root receives
/// it (it must be one of `settings.roots`); omitted, the primary root
/// (`roots[0]`) is used.
#[tauri::command]
pub async fn workspace_rename(
    app: AppHandle,
    from: String,
    new_name: String,
    root: Option<String>,
) -> Result<String, String> {
    let settings = load();
    require_ade_owned(&settings, &from, "renamed")?;
    let root = resolve_save_root(&settings, root)?;
    let destination = validated_child_path(Path::new(&root), &new_name)?;
    relocate_folder(&app, &from, &destination, MovedLabel::Drop).await
}

/// The outcome of saving a temp workspace by copy-migration: where the new real
/// project landed, plus the command that restores the dependency directories the
/// copy skipped (or `null` when nothing needs reinstalling).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMigration {
    /// The new project's absolute path.
    path: String,
    /// The install command to restore skipped dependencies in a task pane, or
    /// null when nothing needs reinstalling.
    install: Option<String>,
}

/// Recursively copy `from` into `to`, skipping any directory whose name is a
/// re-downloadable dependency directory (`node_modules`, `.venv`, …) - those are
/// large and machine-specific, and get reinstalled afterwards. Everything else
/// (`.git`, sources, build output) is copied verbatim, tree structure preserved.
/// Symlinks are copied as plain files (via `std::fs::copy`, which follows the
/// link) so a symlinked directory can never send this into an infinite loop.
pub(super) fn copy_tree_skipping_dependencies(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let source = entry.path();
        let target = to.join(&name);
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            let is_dependency = name
                .to_str()
                .is_some_and(provision::is_dependency_directory);
            if is_dependency {
                continue;
            }
            copy_tree_skipping_dependencies(&source, &target)?;
            continue;
        }

        std::fs::copy(&source, &target)?;
    }
    Ok(())
}

/// Save a temp workspace as a real project by COPYING it into a saved root under
/// `new_name`, rather than renaming it - on Windows PADE's own recursive watcher
/// holds the temp directory open, so a `rename`/move fails with a sharing
/// violation, but reads are unaffected. The copy skips re-downloadable dependency
/// directories; the returned `install` command restores them in a task pane. The
/// destination becomes a plain real project (not marked owned/temp); the temp is
/// left in place for the frontend to delete separately after it opens the copy.
#[tauri::command]
pub async fn workspace_save_migrate(
    from: String,
    new_name: String,
    root: Option<String>,
) -> Result<SaveMigration, String> {
    let settings = load();
    require_ade_owned(&settings, &from, "saved")?;
    let root = resolve_save_root(&settings, root)?;
    let destination = validated_child_path(Path::new(&root), &new_name)?;
    let destination_string = destination.to_string_lossy().into_owned();
    if Path::new(&destination_string).exists() {
        return Err("a project with that name already exists there".into());
    }

    let copy_source = from.clone();
    let copy_destination = destination.clone();
    tauri::async_runtime::spawn_blocking(move || {
        copy_tree_skipping_dependencies(Path::new(&copy_source), &copy_destination)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())??;

    let install = provision::detect_install(Path::new(&destination_string));

    // Re-point external tools (agent memory, IDE recents) at the copied project -
    // best-effort, independent of the settings bookkeeping below. The temp is NOT
    // moved: it stays owned/labeled until the frontend deletes it.
    refs::update_references(&from, &destination_string, &|_| {});
    update_settings(|settings| {
        record_recent(settings, &destination_string);
        // The chosen folder name is the real project's authoritative display name;
        // drop any stale label carried from the temp so it can't shadow it.
        settings.labels.remove(&destination_string);
        Ok(())
    })?;

    Ok(SaveMigration {
        path: destination_string,
        install,
    })
}
