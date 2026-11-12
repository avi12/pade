//! Live MCP config watching: restarts sessions when a project's declared servers change.

use super::registry::ProjectWatch;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Emitter;

// ── Live MCP config ───────────────────────────────────────────────────────────
// A project's MCP servers live in a config file (`.mcp.json` for Claude, per
// config.rs). A running agent only picks up an **added or removed** server by
// restarting - Claude re-reads `.mcp.json` at launch, and there is no in-session
// reload - while a value-only edit to an existing server the user reconnects by
// hand. So the watcher restarts the affected sessions only when the *set of
// server names* changes; the frontend does the terminate-and-resume.

/// Announces that a project's declared MCP servers changed (a name added or
/// removed). Carries which agents the config governs so the frontend restarts
/// only their sessions, and the added/removed names for the toast.
pub(super) const MCP_EVENT: &str = "mcp://changed";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct McpChange {
    /// The MCP config file that changed (absolute path).
    path: String,
    /// The watched project root. Nested configs cannot derive this from `path`.
    root: String,
    /// The agent ids whose servers this file declares (e.g. `["claude"]`).
    agents: Vec<String>,
    /// Server names gained since the last seen set.
    added: Vec<String>,
    /// Server names lost since the last seen set.
    removed: Vec<String>,
    /// Whether the file was created/deleted or server membership changed.
    membership_changed: bool,
}

/// The agents whose MCP servers `path` declares, if it is a known MCP config
/// file under `root` (e.g. `<root>/.mcp.json` → `["claude"]`). `None` for any
/// other path. Matches the full relative path, so a nested config
/// (`.cursor/mcp.json`) can't be confused with a root one.
pub(super) fn mcp_config_for_path(
    root: &Path,
    path: &Path,
) -> Option<crate::workspace::config::McpConfig> {
    // Compared root-relative, so the per-event check joins no paths.
    let relative = path.strip_prefix(root).ok()?;
    crate::workspace::config::mcp_configs()
        .find(|config| relative == Path::new(config.relative_path))
}

pub(super) fn mcp_membership_changed(
    previous: &crate::workspace::mcp::McpSnapshot,
    current: &crate::workspace::mcp::McpSnapshot,
) -> bool {
    previous.exists != current.exists || previous.names != current.names
}

/// Seed the MCP server-name baseline for `root`: record the current set for each
/// known MCP config file, so the first later change diffs against the truth on
/// disk rather than an empty set (which would restart on project open).
pub(super) fn mcp_baseline(root: &Path) -> HashMap<PathBuf, crate::workspace::mcp::McpSnapshot> {
    crate::workspace::config::mcp_configs()
        .filter_map(|config| {
            let file = root.join(config.relative_path);
            crate::workspace::mcp::snapshot(&file, config.format).map(|state| (file, state))
        })
        .collect()
}

/// Detect an MCP server add/remove for a changed `path` and, when the set truly
/// changed, emit [`MCP_EVENT`] and advance the baseline. A value-only edit (same
/// names) and an unparseable half-written file both no-op, so a session restarts
/// only on a real membership change. Runs before the ignore filter, so a
/// `.gitignore`'d `.mcp.json` still triggers.
pub(super) fn detect_mcp_change(app: &AppHandle, watch: &ProjectWatch, path: &Path) {
    let root = &watch.root;
    let Some(config) = mcp_config_for_path(root, path) else {
        return;
    };
    let Some(current) = crate::workspace::mcp::snapshot(path, config.format) else {
        return; // unparseable mid-write - wait for the complete save
    };

    let (added, removed, membership_changed) = {
        let Ok(mut baseline) = watch.mcp_servers.lock() else {
            return;
        };
        let previous = baseline.get(path).cloned().unwrap_or_default();
        if current == previous {
            return;
        }
        let added: Vec<String> = current.names.difference(&previous.names).cloned().collect();
        let removed: Vec<String> = previous.names.difference(&current.names).cloned().collect();
        let membership_changed = mcp_membership_changed(&previous, &current);
        baseline.insert(path.to_path_buf(), current);
        (added, removed, membership_changed)
    };

    let _ = app.emit_to(
        &watch.label,
        MCP_EVENT,
        McpChange {
            path: path.to_string_lossy().into_owned(),
            root: root.to_string_lossy().into_owned(),
            agents: config
                .agents
                .iter()
                .map(|agent| (*agent).to_string())
                .collect(),
            added,
            removed,
            membership_changed,
        },
    );
}
