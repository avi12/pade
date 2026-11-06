//! The keyboard shortcuts a user gave an agent, read from the agent's own
//! file - so PADE presses the keys that agent really answers to when it must
//! interrupt it through its terminal (usage pacing's stop). Read only: PADE
//! never edits them.
//!
//! Only Claude Code has such a file today (`~/.claude/keybindings.json`). Its
//! shape is checked where it is used (`lib/agent/interrupt-keys.ts`); a file
//! Claude itself would reject is read as no customisation, as Claude does.

/// Claude Code's own keybindings file, beside its settings.
const CLAUDE_KEYBINDINGS_FILE: &str = "keybindings.json";

/// The user's Claude Code keybindings as written, or `None` when there are
/// none (or they don't parse). Off the UI thread: it reads a file.
#[tauri::command]
pub async fn claude_keybindings() -> Option<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(|| {
        let path = super::claude_directory()?.join(CLAUDE_KEYBINDINGS_FILE);
        crate::util::read_jsonc(&path)
    })
    .await
    .ok()
    .flatten()
}
