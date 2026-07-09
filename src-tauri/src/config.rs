//! Agent config respect — surface (read-only for MVP) the config files the CLI
//! already uses, without shadowing them. Everything stays on disk; the ADE just
//! shows what's there. Editing writes back to the same files (planned).

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Config files/dirs the ADE knows how to surface, in display order.
const KNOWN: &[(&str, &str)] = &[
    ("CLAUDE.md", "instructions"),
    ("AGENTS.md", "instructions"),
    (".mcp.json", "mcp"),
    (".claude/settings.json", "settings"),
    (".claude/settings.local.json", "settings"),
];

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
pub fn config_list() -> Result<Vec<ConfigFile>, String> {
    let root = root()?;
    let files = KNOWN
        .iter()
        .map(|(rel, kind)| ConfigFile {
            name: Path::new(rel)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(rel)
                .to_string(),
            rel: (*rel).to_string(),
            kind: (*kind).to_string(),
            exists: root.join(rel).is_file(),
        })
        .collect();
    Ok(files)
}

/// Read one known config file. Guarded to the allowlist so this can never read
/// arbitrary paths from the frontend.
#[tauri::command]
pub fn config_read(rel: String) -> Result<String, String> {
    if !KNOWN.iter().any(|(k, _)| *k == rel) {
        return Err("not an allowed config file".into());
    }
    let path = root()?.join(&rel);
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}
