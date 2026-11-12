//! Workspace labels: the manual and automatic names a project row shows.

use super::store::update_settings;
use super::store::Settings;
use super::temp::require_ade_owned;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Who names a workspace - which decides both how the name is cleaned and whether
/// it may replace a label the workspace already carries.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LabelSource {
    /// The auto-namer's suggestion: kebab-cased through `naming::sanitize`, and
    /// applied only to a workspace with no label yet, so it never replaces a name
    /// the user chose.
    Auto,
    /// The user relabelling a workspace in PADE: kept as typed, and always applied.
    Manual,
}

/// The longest label a user may give a workspace, in characters.
pub(super) const MAXIMUM_MANUAL_LABEL_CHARACTERS: usize = 60;

/// A user-typed label as it will be shown - trimmed; `None` when empty, longer
/// than the cap, or carrying a control character (a newline included), since a
/// label is one line of chrome text.
pub(super) fn manual_label(raw: &str) -> Option<String> {
    let label = raw.trim();
    let has_control_character = label.chars().any(char::is_control);
    let too_long = label.chars().count() > MAXIMUM_MANUAL_LABEL_CHARACTERS;
    (!label.is_empty() && !has_control_character && !too_long).then(|| label.to_string())
}

/// Record `label` for `path` - unless it's an auto-name and the workspace is
/// already labelled, in which case the existing name (the user's own, or an
/// earlier auto-name) stands.
pub(super) fn apply_label(
    labels: &mut BTreeMap<String, String>,
    path: String,
    label: String,
    source: LabelSource,
) {
    let keeps_existing_label = source == LabelSource::Auto && labels.contains_key(&path);
    if !keeps_existing_label {
        labels.insert(path, label);
    }
}

/// Set a friendly display label for an ADE-owned workspace. Non-destructive: the
/// directory keeps its `temp-<stamp>` name on disk (the live agent holds it as
/// cwd, which the OS locks against rename); only the shown name changes. The
/// auto-name check-and-set happens under the settings lock, so an auto-name that
/// lands after the user relabelled can't overwrite their choice.
#[tauri::command]
pub fn workspace_set_label(
    path: String,
    name: String,
    source: LabelSource,
) -> Result<Settings, String> {
    let label = match source {
        LabelSource::Auto => crate::agents::naming::sanitize(&name),
        LabelSource::Manual => manual_label(&name),
    }
    .ok_or("invalid name")?;
    update_settings(|settings| {
        require_ade_owned(settings, &path, "labeled")?;
        apply_label(&mut settings.labels, path, label, source);
        Ok(())
    })
}
