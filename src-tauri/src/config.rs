//! Agent config respect — surface (read-only for MVP) the config files the CLI
//! already uses, without shadowing them. Everything stays on disk; the ADE just
//! shows what's there. Editing writes back to the same files (planned).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::WebviewWindow;

/// A config file the ADE can surface: (relative path, kind, agents it applies
/// to). An empty agent list means it applies to every agent.
struct ConfigDefinition {
    rel: &'static str,
    kind: &'static str,
    agents: &'static [&'static str],
    mcp_format: Option<McpFormat>,
}

/// How a specific agent config represents its MCP server-name map.
#[derive(Debug, Clone, Copy)]
pub enum McpFormat {
    JsonObject { key: &'static str },
    TomlTables { key: &'static str },
}

/// Config files/dirs the ADE knows how to surface, in display order. Only the
/// files relevant to the active agent are shown — e.g. CLAUDE.md for Claude
/// Code, AGENTS.md for agents that follow that convention.
const KNOWN: &[ConfigDefinition] = &[
    ConfigDefinition {
        rel: "CLAUDE.md",
        kind: "instructions",
        agents: &["claude"],
        mcp_format: None,
    },
    ConfigDefinition {
        rel: "AGENTS.md",
        kind: "instructions",
        agents: &["codex", "cursor", "antigravity", "aider"],
        mcp_format: None,
    },
    ConfigDefinition {
        rel: ".mcp.json",
        kind: "mcp",
        agents: &["claude", "copilot"],
        mcp_format: Some(McpFormat::JsonObject { key: "mcpServers" }),
    },
    ConfigDefinition {
        rel: ".github/mcp.json",
        kind: "mcp",
        agents: &["copilot"],
        mcp_format: Some(McpFormat::JsonObject { key: "mcpServers" }),
    },
    ConfigDefinition {
        rel: "opencode.json",
        kind: "mcp",
        agents: &["opencode"],
        mcp_format: Some(McpFormat::JsonObject { key: "mcp" }),
    },
    ConfigDefinition {
        rel: ".codex/config.toml",
        kind: "mcp",
        agents: &["codex"],
        mcp_format: Some(McpFormat::TomlTables { key: "mcp_servers" }),
    },
    ConfigDefinition {
        rel: ".cursor/mcp.json",
        kind: "mcp",
        agents: &["cursor"],
        mcp_format: Some(McpFormat::JsonObject { key: "mcpServers" }),
    },
    ConfigDefinition {
        rel: ".claude/settings.json",
        kind: "settings",
        agents: &["claude"],
        mcp_format: None,
    },
    ConfigDefinition {
        rel: ".claude/settings.local.json",
        kind: "settings",
        agents: &["claude"],
        mcp_format: None,
    },
];

fn applies_to(definition: &ConfigDefinition, agent: &str) -> bool {
    definition.agents.is_empty() || definition.agents.contains(&agent)
}

/// One MCP-server config file the ADE surfaces: its repo-relative path and the
/// agents whose servers it declares. The single source of truth for "which file
/// holds a project's MCP servers" — the watcher reads this to know what to watch
/// for added/removed servers (`mcp.rs`).
pub struct McpConfig {
    pub rel: &'static str,
    pub agents: &'static [&'static str],
    pub format: McpFormat,
}

/// Every known project-level MCP config file (e.g. `.mcp.json` for Claude),
/// drawn from the same [`KNOWN`] registry the config panel shows — no second
/// list to drift.
pub fn mcp_configs() -> impl Iterator<Item = McpConfig> {
    KNOWN
        .iter()
        .filter_map(|definition| definition.mcp_format.map(|format| (definition, format)))
        .map(|(definition, format)| McpConfig {
            rel: definition.rel,
            agents: definition.agents,
            format,
        })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    name: String,
    rel: String,
    kind: String,
    exists: bool,
}

fn root(
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
    let root = root(&window, &projects)?;
    let files = KNOWN
        .iter()
        .filter(|definition| applies_to(definition, &agent))
        .map(|definition| ConfigFile {
            name: Path::new(definition.rel)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(definition.rel)
                .to_string(),
            rel: definition.rel.to_string(),
            kind: definition.kind.to_string(),
            exists: root.join(definition.rel).is_file(),
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
    if !KNOWN.iter().any(|definition| definition.rel == rel) {
        return Err("not an allowed config file".into());
    }
    let path = root(&window, &projects)?.join(&rel);
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}
