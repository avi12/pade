//! `OpenCode`: a per-launch plugin, selected through the launch's environment.
//!
//! `OPENCODE_CONFIG_CONTENT` adds PADE's plugin to the user's plugin list for
//! this launch alone — their config is never touched. The plugin writes the
//! session's model and reasoning variant to the file `PADE_MODEL_STATE_FILE`
//! names, from `chat.message` (every prompt) and `session.updated`. A model
//! picked in the TUI stays inside the TUI until the next prompt is sent, so
//! that is when the tab follows; before the first prompt the configured model
//! stands in. Measured against `OpenCode` 1.18.33.

use std::path::PathBuf;

use super::{read_session_state, state_file, EffortOption, ModelOption, RunningModel};

const CONFIG_CONTENT_VARIABLE: &str = "OPENCODE_CONFIG_CONTENT";
const STATE_FILE_VARIABLE: &str = "PADE_MODEL_STATE_FILE";
const PLUGIN_FILE: &str = "pade-model.js";
const CONFIG_FILE: &str = "opencode.json";

/// Records the session's model and variant in PADE's state-file shape.
/// `default` is the "no variant chosen" value.
const PLUGIN_SOURCE: &str = r#"// PADE: records this session's model and reasoning variant for its tab.
import { writeFileSync } from "node:fs";

export const PadeModel = async () => {
  const file = process.env.PADE_MODEL_STATE_FILE;
  let current = null;
  const record = (providerID, modelID, variant) => {
    if (!file || !modelID) return;
    const now = Date.now();
    const effort = variant && variant !== "default" ? variant : null;
    const model = providerID ? `${providerID}/${modelID}` : modelID;
    try {
      writeFileSync(file, JSON.stringify({ model, modelAt: now, effort, effortAt: now, agentSession: current }));
    } catch (error) {
      console.error("pade: could not record the model", error);
    }
  };
  return {
    "chat.message": async (input) => {
      current = input.sessionID;
      record(input.model?.providerID, input.model?.modelID, input.variant);
    },
    event: async ({ event }) => {
      if (event.type !== "session.updated") return;
      const info = event.properties?.info;
      if (!info || info.id !== current || !info.model) return;
      record(info.model.providerID, info.model.id, info.model.variant);
    },
  };
};
"#;

/// The environment that loads the plugin and tells it where to write.
pub(super) fn attach(conversation_id: &str) -> Vec<(String, String)> {
    let (Some(plugin), Some(state)) = (materialize_plugin(), state_file(conversation_id)) else {
        return Vec::new();
    };
    let plugin_url = format!(
        "file:///{}",
        plugin
            .to_string_lossy()
            .replace('\\', "/")
            .trim_start_matches('/')
    );
    let inherited = std::env::var(CONFIG_CONTENT_VARIABLE).ok();
    vec![
        (
            CONFIG_CONTENT_VARIABLE.to_string(),
            config_with_plugin(inherited.as_deref(), &plugin_url),
        ),
        (
            STATE_FILE_VARIABLE.to_string(),
            state.to_string_lossy().into_owned(),
        ),
    ]
}

/// The plugin on disk, rewritten only when its source changed.
fn materialize_plugin() -> Option<PathBuf> {
    let path = crate::util::pade_data_dir()?
        .join("opencode")
        .join(PLUGIN_FILE);
    crate::util::write_if_stale(&path, PLUGIN_SOURCE).ok()?;
    Some(path)
}

/// Inline config adding the plugin — on top of any inline config PADE itself
/// was started with, so a user's own `OPENCODE_CONFIG_CONTENT` still applies.
fn config_with_plugin(inherited: Option<&str>, plugin_url: &str) -> String {
    let mut config = inherited
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    let plugins = config.as_object_mut().map(|object| {
        object
            .entry("plugin")
            .or_insert_with(|| serde_json::json!([]))
    });
    if let Some(serde_json::Value::Array(list)) = plugins {
        list.push(serde_json::Value::String(plugin_url.to_string()));
    }
    config.to_string()
}

pub(super) fn read(conversation_id: &str) -> Option<RunningModel> {
    let reported = read_session_state(conversation_id).and_then(|state| {
        Some(RunningModel {
            model: state.model?,
            effort: state.effort,
        })
    });
    reported.or_else(configured_model)
}

/// The model the user's global config names — what a fresh session opens on.
fn configured_model() -> Option<RunningModel> {
    Some(RunningModel {
        model: configured_model_id()?,
        effort: None,
    })
}

fn configured_model_id() -> Option<String> {
    let path = crate::util::home_dir()?
        .join(".config")
        .join("opencode")
        .join(CONFIG_FILE);
    let config = read_json(&path)?;
    Some(config.get("model")?.as_str()?.to_string())
}

fn read_json(path: &std::path::Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// The reasoning-option kind whose values are effort levels.
const EFFORT_OPTION_KIND: &str = "effort";

/// The models the user reaches for — the configured one, favourites and
/// recents from the picker state `OpenCode` saves — each described by the
/// models.dev catalog it caches (name, effort values).
pub(super) fn options() -> Vec<ModelOption> {
    let Some(home) = crate::util::home_dir() else {
        return Vec::new();
    };
    let picker_state = read_json(
        &home
            .join(".local")
            .join("state")
            .join("opencode")
            .join("model.json"),
    );
    let catalog = read_json(&home.join(".cache").join("opencode").join("models.json"));

    let favourites_then_recents = ["favorite", "recent"].into_iter().flat_map(|list| {
        picker_state
            .as_ref()
            .and_then(|state| state.get(list)?.as_array().cloned())
            .unwrap_or_default()
    });
    let mut ids: Vec<String> = configured_model_id().into_iter().collect();
    for entry in favourites_then_recents {
        let (Some(provider), Some(model)) = (
            entry.get("providerID").and_then(serde_json::Value::as_str),
            entry.get("modelID").and_then(serde_json::Value::as_str),
        ) else {
            continue;
        };
        let id = format!("{provider}/{model}");
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids.into_iter()
        .map(|id| describe(&id, catalog.as_ref()))
        .collect()
}

fn describe(id: &str, catalog: Option<&serde_json::Value>) -> ModelOption {
    let entry = id
        .split_once('/')
        .and_then(|(provider, model)| catalog?.get(provider)?.get("models")?.get(model));
    let efforts = entry
        .and_then(|entry| entry.get("reasoning_options")?.as_array())
        .and_then(|options| {
            options.iter().find(|option| {
                option.get("type").and_then(serde_json::Value::as_str) == Some(EFFORT_OPTION_KIND)
            })
        })
        .and_then(|option| option.get("values")?.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(EffortOption::plain)
                .collect()
        })
        .unwrap_or_default();
    ModelOption {
        id: id.to_string(),
        name: entry
            .and_then(|entry| entry.get("name")?.as_str())
            .unwrap_or(id)
            .to_string(),
        efforts,
        recommended_effort: None,
    }
}

#[cfg(test)]
mod tests {
    use super::config_with_plugin;

    #[test]
    fn the_plugin_joins_an_inherited_inline_config() {
        let merged = config_with_plugin(Some(r#"{"plugin":["mine"],"theme":"x"}"#), "file:///p.js");
        let value: serde_json::Value = serde_json::from_str(&merged).expect("valid JSON");
        assert_eq!(value["plugin"], serde_json::json!(["mine", "file:///p.js"]));
        assert_eq!(value["theme"], "x");
    }

    #[test]
    fn without_one_the_plugin_stands_alone() {
        assert_eq!(
            config_with_plugin(None, "file:///p.js"),
            r#"{"plugin":["file:///p.js"]}"#
        );
    }
}
