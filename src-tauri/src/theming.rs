//! Force each installed agent's own UI theme to match ADE's light/dark scheme.
//!
//! Why a config file and not the terminal protocol: Claude Code's `auto` theme
//! follows the *terminal* — it queries the background color (OSC 11) at startup
//! and listens for color-scheme reports (DECSET 2031 → `CSI ?997;n`) — but
//! Windows `ConPTY` consumes both flavors on the way through, so the agent never
//! hears from ADE's xterm and falls back to its dark default even on a light
//! ADE. Verified live: kilobytes of session stream carry the agent's
//! `DECSET 2031` yet no OSC 10/11 query ever reaches the frontend, and an
//! injected `?997` report changes nothing. The channel that does work is the
//! agent's own settings file — Claude Code re-reads
//! `.claude/settings.local.json` live mid-session — so ADE writes each
//! installed agent's theme config on project open and again on every scheme
//! flip (`App.svelte` → `theme_sync`).

use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value};

/// ADE's resolved appearance — the frontend's `appearance.scheme`, on the wire.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Light,
    Dark,
}

/// How one agent's theme is forced — registry knowledge, declared per agent in
/// `agents.rs` and interpreted here (`SoC`: the registry knows *what*, this
/// module knows *how to write it*).
pub enum ThemeConfig {
    /// Merge `{<key>: <per-scheme value>}` into a JSON settings file at
    /// `relative_path` under the workspace, preserving every other key the
    /// file already holds (permissions, env, …). For a CLI that re-reads the
    /// file live, a scheme flip re-themes even a running session.
    WorkspaceJson {
        relative_path: &'static str,
        key: &'static str,
        light: &'static str,
        dark: &'static str,
    },
    /// Set per-scheme environment when the session spawns — for a CLI whose
    /// theme is env-driven and read once at startup (`pty.rs` applies these;
    /// a scheme flip reaches it on the next spawn). Either side may be empty
    /// when the CLI only needs help on one scheme.
    SpawnEnv {
        light: &'static [(&'static str, &'static str)],
        dark: &'static [(&'static str, &'static str)],
    },
}

/// The per-scheme environment to spawn `command` with (empty for an agent
/// whose theme is file-driven, or unknown). `pty.rs` applies it alongside the
/// static `agents::spawn_env`.
pub fn spawn_env(command: &str, scheme: Scheme) -> &'static [(&'static str, &'static str)] {
    match crate::agents::theme_config(command) {
        Some(ThemeConfig::SpawnEnv { light, dark }) => match scheme {
            Scheme::Light => light,
            Scheme::Dark => dark,
        },
        Some(ThemeConfig::WorkspaceJson { .. }) | None => &[],
    }
}

/// Write every installed agent's theme config in `workspace` to `scheme`.
/// One agent's failure doesn't stop the others; the joined errors come back.
///
/// `spawn_blocking` for the same reason as `agents_detect`: finding the
/// installed agents reads the live PATH and stats candidate files, which would
/// otherwise stall the main thread's window loop.
#[tauri::command]
pub async fn theme_sync(workspace: String, scheme: Scheme) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || sync_workspace(Path::new(&workspace), scheme))
        .await
        .map_err(|error| error.to_string())?
}

fn sync_workspace(workspace: &Path, scheme: Scheme) -> Result<(), String> {
    let mut errors = Vec::new();
    for config in crate::agents::installed_theme_configs() {
        if let Err(error) = apply(workspace, config, scheme) {
            errors.push(error);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn apply(workspace: &Path, config: &ThemeConfig, scheme: Scheme) -> Result<(), String> {
    match config {
        ThemeConfig::WorkspaceJson {
            relative_path,
            key,
            light,
            dark,
        } => {
            let value = match scheme {
                Scheme::Light => light,
                Scheme::Dark => dark,
            };
            merge_json_key(&workspace.join(relative_path), key, value)
        }
        // Applied at spawn time by pty.rs (see `spawn_env`) — nothing to write.
        ThemeConfig::SpawnEnv { .. } => Ok(()),
    }
}

/// Set `key` to `value` in the JSON object at `path`, creating the file (and
/// its directories) when absent and leaving every other key untouched. A file
/// that exists but doesn't parse is left alone — rewriting it would destroy
/// whatever the user (or the agent) had there.
fn merge_json_key(path: &Path, key: &str, value: &str) -> Result<(), String> {
    let mut settings: Map<String, Value> = match fs::read_to_string(path) {
        Err(_) => Map::new(),
        Ok(text) => serde_json::from_str(&text).map_err(|error| {
            format!("won't rewrite {}: not valid JSON ({error})", path.display())
        })?,
    };

    let already_current = settings.get(key).and_then(Value::as_str) == Some(value);
    if already_current {
        return Ok(());
    }

    settings.insert(key.into(), Value::String(value.into()));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let mut serialized =
        serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    serialized.push('\n');
    fs::write(path, serialized).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{merge_json_key, Map, Value};
    use std::fs;

    fn scratch_file(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join("pade-theming-tests").join(name);
        // Each test owns a distinct leaf directory, so clearing only that keeps
        // parallel tests out of each other's way.
        let _ = fs::remove_dir_all(path.parent().expect("scratch parent"));
        path
    }

    fn read_settings(path: &std::path::Path) -> Map<String, Value> {
        serde_json::from_str(&fs::read_to_string(path).expect("settings file"))
            .expect("valid settings JSON")
    }

    #[test]
    fn creates_the_file_and_its_directories() {
        let path = scratch_file("fresh/.claude/settings.local.json");
        merge_json_key(&path, "theme", "light").expect("merge");
        let written = read_settings(&path);
        assert_eq!(written.get("theme").and_then(Value::as_str), Some("light"));
    }

    /// The whole point of merging: an existing settings file (permissions the
    /// agent wrote, user env) keeps every key it had.
    #[test]
    fn preserves_the_other_keys() {
        let path = scratch_file("keep/settings.local.json");
        fs::create_dir_all(path.parent().expect("parent")).expect("fixture dir");
        fs::write(
            &path,
            "{\n  \"permissions\": { \"allow\": [\"Bash\"] }\n}\n",
        )
        .expect("fixture file");
        merge_json_key(&path, "theme", "dark").expect("merge");
        let written = read_settings(&path);
        assert_eq!(written.get("theme").and_then(Value::as_str), Some("dark"));
        assert!(written.get("permissions").is_some_and(Value::is_object));
    }

    /// Re-forcing the same scheme must not touch the file — the agent watches
    /// it (Claude re-reads live), and a no-op write would churn that watch and
    /// ADE's own Change Feed.
    #[test]
    fn an_already_current_value_writes_nothing() {
        let path = scratch_file("idempotent/settings.local.json");
        merge_json_key(&path, "theme", "light").expect("first merge");
        let modified = |path: &std::path::Path| {
            fs::metadata(path)
                .expect("settings metadata")
                .modified()
                .expect("mtime")
        };
        let before = modified(&path);
        merge_json_key(&path, "theme", "light").expect("repeat merge");
        assert_eq!(modified(&path), before);
    }

    /// A malformed settings file is the user's data, not ours to clobber.
    #[test]
    fn refuses_to_rewrite_invalid_json() {
        let path = scratch_file("broken/settings.local.json");
        fs::create_dir_all(path.parent().expect("parent")).expect("fixture dir");
        fs::write(&path, "{ not json").expect("fixture file");
        assert!(merge_json_key(&path, "theme", "dark").is_err());
        assert_eq!(
            fs::read_to_string(&path).expect("settings file"),
            "{ not json"
        );
    }
}
