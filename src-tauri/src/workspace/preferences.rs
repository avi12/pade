//! Per-project and default agent, user-added editors and the free-form preference patch.

use super::roots::canonical_path;
use super::store::update_settings;
use super::store::AddedEditor;
use super::store::Prefs;
use super::store::Settings;

/// Master switch: set the default agent for every project and clear per-project
/// overrides so the whole workspace moves to it at once.
#[tauri::command]
pub(crate) fn set_default_agent(agent: String) -> Result<Settings, String> {
    update_settings(|settings| {
        settings.default_agent = Some(agent);
        settings.project_agents.clear();
        Ok(())
    })
}

/// Override the agent for a single project.
#[tauri::command]
pub(crate) fn set_project_agent(path: String, agent: String) -> Result<Settings, String> {
    update_settings(|settings| {
        settings.project_agents.insert(path, agent);
        Ok(())
    })
}

/// Persist a user-added editor, de-duplicated by executable path (re-adding the
/// same path is a no-op move-to-end). Returns the refreshed settings.
pub(crate) fn add_editor(editor: AddedEditor) -> Result<Settings, String> {
    update_settings(|settings| {
        settings
            .prefs
            .added_editors
            .retain(|existing| existing.path != editor.path);
        settings.prefs.added_editors.push(editor);
        Ok(())
    })
}

/// Persist the user's explicit editor pick for one project - keyed by the
/// canonical path so spelling variants of the same folder resolve to one entry.
/// Returns the refreshed settings.
pub(crate) fn set_project_editor(path: &str, editor_id: &str) -> Result<Settings, String> {
    update_settings(|settings| {
        settings
            .prefs
            .ide_project_choices
            .insert(canonical_path(path), editor_id.to_string());
        Ok(())
    })
}

/// Drop a user-added editor by its id. Returns the refreshed settings; removing
/// an id that isn't present is a no-op.
pub(crate) fn remove_editor(id: &str) -> Result<Settings, String> {
    update_settings(|settings| {
        settings
            .prefs
            .added_editors
            .retain(|editor| editor.id != id);
        Ok(())
    })
}

pub(super) fn apply_prefs_patch(
    current: &mut Prefs,
    patch: serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let serde_json::Value::Object(mut prefs) =
        serde_json::to_value(&*current).map_err(|error| error.to_string())?
    else {
        return Err("preferences did not serialize as an object".into());
    };
    for (key, value) in patch {
        if value.is_null() {
            prefs.remove(&key);
        } else {
            prefs.insert(key, value);
        }
    }
    *current = serde_json::from_value(serde_json::Value::Object(prefs))
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Apply only the preference fields the frontend changed. A null value removes
/// the optional key so its default takes effect again.
#[tauri::command]
pub(crate) fn set_prefs(patch: serde_json::Map<String, serde_json::Value>) -> Result<Settings, String> {
    update_settings(|settings| apply_prefs_patch(&mut settings.prefs, patch))
}
