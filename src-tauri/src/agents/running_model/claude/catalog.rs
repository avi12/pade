//! The models Claude Code itself lists: its cached `/model` picker catalog read
//! into names, effort levels with their labels and the recommended level.

use super::{settings_directory, EffortOption, ModelOption};

/// The catalog surface Claude Code's own `/model` picker reads.
const CATALOG_SURFACE_SUFFIX: &str = "-cc.json";
/// The badge Claude puts on the level it recommends for a model.
const RECOMMENDED_BADGE: &str = "Recommended";

/// The models Claude Code itself lists, from the catalog it caches under
/// `~/.claude/cache/model-catalog` - names, effort levels with their labels,
/// and the recommended level, exactly as its picker shows them.
pub(in crate::agents::running_model) fn options() -> Vec<ModelOption> {
    let Some(catalog) = newest_catalog() else {
        return Vec::new();
    };
    let Some(models) = catalog
        .pointer("/catalog/config/models")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    models.iter().filter_map(model_option).collect()
}

pub(super) fn model_option(model: &serde_json::Value) -> Option<ModelOption> {
    let id = model.get("id")?.as_str()?;
    let effort_options = model
        .pointer("/thinking/effort_options")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let recommended_effort = effort_options
        .iter()
        .find(|option| is_recommended(option))
        .and_then(|option| option.get("id")?.as_str())
        .map(str::to_string);
    Some(ModelOption {
        id: id.to_string(),
        name: text_or(model, "name", id),
        efforts: effort_options.iter().filter_map(effort_option).collect(),
        recommended_effort,
    })
}

fn effort_option(option: &serde_json::Value) -> Option<EffortOption> {
    let id = option.get("id")?.as_str()?;
    Some(EffortOption {
        id: id.to_string(),
        name: text_or(option, "name", id),
    })
}

fn is_recommended(effort_option: &serde_json::Value) -> bool {
    effort_option
        .pointer("/badge/message")
        .and_then(serde_json::Value::as_str)
        == Some(RECOMMENDED_BADGE)
}

/// `value[key]` as text, else `fallback`.
fn text_or(value: &serde_json::Value, key: &str, fallback: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

/// The most recently written catalog for Claude Code's own picker.
pub(super) fn newest_catalog() -> Option<serde_json::Value> {
    let directory = settings_directory()?.join("cache").join("model-catalog");
    let newest = std::fs::read_dir(directory)
        .ok()?
        .flatten()
        .filter(is_picker_catalog)
        .max_by_key(|entry| {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
        })?;
    crate::util::read_json(&newest.path())
}

fn is_picker_catalog(entry: &std::fs::DirEntry) -> bool {
    entry
        .file_name()
        .to_string_lossy()
        .ends_with(CATALOG_SURFACE_SUFFIX)
}
