//! Agent config respect — surface (read-only for MVP) the config files the CLI
//! already uses, without shadowing them. Everything stays on disk; the ADE just
//! shows what's there. Editing writes back to the same files (planned).

use std::path::{Path, PathBuf};

use serde::Serialize;

/// A config file the ADE can surface: (relative path, kind, agents it applies
/// to). An empty agent list means it applies to every agent.
struct ConfigDef {
    rel: &'static str,
    kind: &'static str,
    agents: &'static [&'static str],
}

/// Config files/dirs the ADE knows how to surface, in display order. Only the
/// files relevant to the active agent are shown — e.g. CLAUDE.md for Claude
/// Code, AGENTS.md for agents that follow that convention.
const KNOWN: &[ConfigDef] = &[
    ConfigDef {
        rel: "CLAUDE.md",
        kind: "instructions",
        agents: &["claude"],
    },
    ConfigDef {
        rel: "AGENTS.md",
        kind: "instructions",
        agents: &["codex", "cursor", "antigravity", "aider"],
    },
    ConfigDef {
        rel: ".mcp.json",
        kind: "mcp",
        agents: &["claude"],
    },
    ConfigDef {
        rel: ".claude/settings.json",
        kind: "settings",
        agents: &["claude"],
    },
    ConfigDef {
        rel: ".claude/settings.local.json",
        kind: "settings",
        agents: &["claude"],
    },
];

fn applies_to(def: &ConfigDef, agent: &str) -> bool {
    def.agents.is_empty() || def.agents.contains(&agent)
}

/// The `kind` marking an MCP-server config file in [`KNOWN`].
const MCP_KIND: &str = "mcp";

/// One MCP-server config file the ADE surfaces: its repo-relative path and the
/// agents whose servers it declares. The single source of truth for "which file
/// holds a project's MCP servers" — the watcher reads this to know what to watch
/// for added/removed servers (`mcp.rs`).
pub struct McpConfig {
    pub rel: &'static str,
    pub agents: &'static [&'static str],
}

/// Every known project-level MCP config file (e.g. `.mcp.json` for Claude),
/// drawn from the same [`KNOWN`] registry the config panel shows — no second
/// list to drift.
pub fn mcp_configs() -> impl Iterator<Item = McpConfig> {
    KNOWN
        .iter()
        .filter(|def| def.kind == MCP_KIND)
        .map(|def| McpConfig {
            rel: def.rel,
            agents: def.agents,
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

fn root() -> Result<PathBuf, String> {
    std::env::current_dir().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn config_list(agent: String) -> Result<Vec<ConfigFile>, String> {
    let root = root()?;
    let files = KNOWN
        .iter()
        .filter(|def| applies_to(def, &agent))
        .map(|def| ConfigFile {
            name: Path::new(def.rel)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(def.rel)
                .to_string(),
            rel: def.rel.to_string(),
            kind: def.kind.to_string(),
            exists: root.join(def.rel).is_file(),
        })
        .collect();
    Ok(files)
}

/// Read one known config file. Guarded to the allowlist so this can never read
/// arbitrary paths from the frontend.
#[tauri::command]
pub async fn config_read(rel: String) -> Result<String, String> {
    if !KNOWN.iter().any(|def| def.rel == rel) {
        return Err("not an allowed config file".into());
    }
    let path = root()?.join(&rel);
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}
