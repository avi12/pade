//! Agent config respect - surface (read-only for MVP) the config files the CLI
//! already uses, without shadowing them. Everything stays on disk; the ADE just
//! shows what's there. Editing writes back to the same files (planned).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::WebviewWindow;

use crate::agents::{
    ID_AIDER, ID_ANTIGRAVITY, ID_CLAUDE, ID_CODEX, ID_COPILOT, ID_CURSOR, ID_OPENCODE,
};

/// What a surfaced config file holds, in the wire strings the frontend's
/// `ConfigFile` schema reads.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
enum ConfigKind {
    Instructions,
    Mcp,
    Settings,
}

/// The JSON key Claude, Copilot and Cursor all nest their MCP servers under.
const MCP_SERVERS_JSON_KEY: &str = "mcpServers";

/// A config file the ADE can surface: (relative path, kind, agents it applies
/// to). An empty agent list means it applies to every agent.
struct ConfigDefinition {
    relative_path: &'static str,
    kind: ConfigKind,
    agents: &'static [&'static str],
    mcp_format: Option<McpFormat>,
}

impl ConfigDefinition {
    fn applies_to(&self, agent: &str) -> bool {
        self.agents.is_empty() || self.agents.contains(&agent)
    }

    fn file_name(&self) -> &'static str {
        Path::new(self.relative_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(self.relative_path)
    }
}

/// How a specific agent config represents its MCP server-name map.
#[derive(Debug, Clone, Copy)]
pub enum McpFormat {
    JsonObject { key: &'static str },
    TomlTables { key: &'static str },
}

/// Config files/dirs the ADE knows how to surface, in display order. Only the
/// files relevant to the active agent are shown - e.g. CLAUDE.md for Claude
/// Code, AGENTS.md for agents that follow that convention.
const KNOWN_CONFIG_FILES: &[ConfigDefinition] = &[
    ConfigDefinition {
        relative_path: "CLAUDE.md",
        kind: ConfigKind::Instructions,
        agents: &[ID_CLAUDE],
        mcp_format: None,
    },
    ConfigDefinition {
        relative_path: "AGENTS.md",
        kind: ConfigKind::Instructions,
        agents: &[ID_CODEX, ID_CURSOR, ID_ANTIGRAVITY, ID_AIDER],
        mcp_format: None,
    },
    ConfigDefinition {
        relative_path: ".mcp.json",
        kind: ConfigKind::Mcp,
        agents: &[ID_CLAUDE, ID_COPILOT],
        mcp_format: Some(McpFormat::JsonObject {
            key: MCP_SERVERS_JSON_KEY,
        }),
    },
    ConfigDefinition {
        relative_path: ".github/mcp.json",
        kind: ConfigKind::Mcp,
        agents: &[ID_COPILOT],
        mcp_format: Some(McpFormat::JsonObject {
            key: MCP_SERVERS_JSON_KEY,
        }),
    },
    ConfigDefinition {
        relative_path: "opencode.json",
        kind: ConfigKind::Mcp,
        agents: &[ID_OPENCODE],
        mcp_format: Some(McpFormat::JsonObject { key: "mcp" }),
    },
    ConfigDefinition {
        relative_path: ".codex/config.toml",
        kind: ConfigKind::Mcp,
        agents: &[ID_CODEX],
        mcp_format: Some(McpFormat::TomlTables { key: "mcp_servers" }),
    },
    ConfigDefinition {
        relative_path: ".cursor/mcp.json",
        kind: ConfigKind::Mcp,
        agents: &[ID_CURSOR],
        mcp_format: Some(McpFormat::JsonObject {
            key: MCP_SERVERS_JSON_KEY,
        }),
    },
    ConfigDefinition {
        relative_path: ".claude/settings.json",
        kind: ConfigKind::Settings,
        agents: &[ID_CLAUDE],
        mcp_format: None,
    },
    ConfigDefinition {
        relative_path: ".claude/settings.local.json",
        kind: ConfigKind::Settings,
        agents: &[ID_CLAUDE],
        mcp_format: None,
    },
];

/// One MCP-server config file the ADE surfaces: its repo-relative path and the
/// agents whose servers it declares. The single source of truth for "which file
/// holds a project's MCP servers" - the watcher reads this to know what to watch
/// for added/removed servers (`mcp.rs`).
pub struct McpConfig {
    pub relative_path: &'static str,
    pub agents: &'static [&'static str],
    pub format: McpFormat,
}

/// Every known project-level MCP config file (e.g. `.mcp.json` for Claude),
/// drawn from the same [`KNOWN_CONFIG_FILES`] registry the config panel shows - no second
/// list to drift.
pub fn mcp_configs() -> impl Iterator<Item = McpConfig> {
    KNOWN_CONFIG_FILES
        .iter()
        .filter_map(|definition| definition.mcp_format.map(|format| (definition, format)))
        .map(|(definition, format)| McpConfig {
            relative_path: definition.relative_path,
            agents: definition.agents,
            format,
        })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    name: String,
    #[serde(rename = "rel")]
    relative_path: String,
    kind: ConfigKind,
    exists: bool,
}

fn project_root(
    window: &WebviewWindow,
    projects: &crate::window::WindowProjects,
) -> Result<PathBuf, String> {
    projects.project_for(window.label()).map(PathBuf::from)
}

#[tauri::command]
pub async fn config_list(
    window: WebviewWindow,
    projects: tauri::State<'_, crate::window::WindowProjects>,
    agent: String,
) -> Result<Vec<ConfigFile>, String> {
    let root = project_root(&window, &projects)?;
    let files = KNOWN_CONFIG_FILES
        .iter()
        .filter(|definition| definition.applies_to(&agent))
        .map(|definition| ConfigFile {
            name: definition.file_name().to_string(),
            relative_path: definition.relative_path.to_string(),
            kind: definition.kind,
            exists: root.join(definition.relative_path).is_file(),
        })
        .collect();
    Ok(files)
}

/// Read one known config file. Guarded to the allowlist so this can never read
/// arbitrary paths from the frontend.
#[tauri::command]
pub async fn config_read(
    window: WebviewWindow,
    projects: tauri::State<'_, crate::window::WindowProjects>,
    rel: String,
) -> Result<String, String> {
    let is_allowed = KNOWN_CONFIG_FILES
        .iter()
        .any(|definition| definition.relative_path == rel);
    if !is_allowed {
        return Err("not an allowed config file".into());
    }
    let path = project_root(&window, &projects)?.join(&rel);
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}
