//! Reading a project's declared MCP servers, for the auto-restart-on-change flow.
//!
//! A running agent only picks up an **added or removed** MCP server by
//! restarting (Claude Code re-reads `.mcp.json` at launch; there is no in-session
//! reload). Merely editing an existing server's config the user reconnects by
//! hand. So the watcher restarts a session only when the *set of server names*
//! changes — this module reads that set from the config file; `watcher.rs`
//! keeps the per-file baseline and diffs it. The config-file paths themselves
//! live in `config.rs` (the one registry of known agent files).

use std::collections::BTreeSet;
use std::path::Path;

/// The declared MCP server names in the config file at `path` (the keys of its
/// `mcpServers` object), or `None` when the set can't be determined right now.
///
/// A missing file is `Some(empty)` — every server is gone, which is a real
/// remove. But a file that exists yet doesn't parse (a half-written save caught
/// mid-flush) is `None`: the caller skips it and waits for the next, complete
/// write, so a transient partial file never reads as "all servers removed".
pub fn server_names(path: &Path) -> Option<BTreeSet<String>> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Some(BTreeSet::new());
    };
    let document: serde_json::Value = serde_json::from_str(&text).ok()?;
    let names = document
        .get("mcpServers")
        .and_then(serde_json::Value::as_object)
        .map(|servers| servers.keys().cloned().collect())
        .unwrap_or_default();
    Some(names)
}

#[cfg(test)]
mod tests {
    use super::server_names;
    use std::collections::BTreeSet;

    fn scratch(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join("pade-mcp-tests").join(name);
        let _ = std::fs::remove_dir_all(path.parent().expect("scratch parent"));
        std::fs::create_dir_all(path.parent().expect("scratch parent")).expect("scratch dir");
        path
    }

    fn write(path: &std::path::Path, body: &str) {
        std::fs::write(path, body).expect("write fixture");
    }

    fn names(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn reads_the_mcp_server_keys() {
        let path = scratch("read/.mcp.json");
        write(
            &path,
            r#"{ "mcpServers": { "github": { "type": "http" }, "postgres": { "command": "x" } } }"#,
        );
        assert_eq!(server_names(&path), Some(names(&["github", "postgres"])));
    }

    /// Only the SET of names matters — an edit to an existing server's config
    /// (same keys) reads as the same set, so the watcher won't restart on it.
    #[test]
    fn a_value_only_edit_keeps_the_same_set() {
        let path = scratch("edit/.mcp.json");
        write(&path, r#"{ "mcpServers": { "github": { "url": "a" } } }"#);
        let before = server_names(&path);
        write(&path, r#"{ "mcpServers": { "github": { "url": "b" } } }"#);
        assert_eq!(server_names(&path), before);
    }

    #[test]
    fn a_missing_file_is_an_empty_set() {
        let path = scratch("gone/.mcp.json");
        assert_eq!(server_names(&path), Some(BTreeSet::new()));
    }

    #[test]
    fn no_mcp_servers_key_is_an_empty_set() {
        let path = scratch("empty/.mcp.json");
        write(&path, r#"{ "other": true }"#);
        assert_eq!(server_names(&path), Some(BTreeSet::new()));
    }

    /// A half-written file is unknowable, not "all removed" — so the caller can
    /// wait for the complete write rather than restart on a transient.
    #[test]
    fn an_unparseable_file_is_unknown() {
        let path = scratch("partial/.mcp.json");
        write(&path, r#"{ "mcpServers": { "githu"#);
        assert_eq!(server_names(&path), None);
    }
}
