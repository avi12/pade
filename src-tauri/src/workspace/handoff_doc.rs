//! Deleting a handoff document a project kept for a continued session.

use std::path::Path;

/// The name prefix and extension of every doc auto-handoff writes.
pub(super) const HANDOFF_DOC_PREFIX: &str = "continue-";

pub(super) const HANDOFF_DOC_EXTENSION: &str = "md";

/// Delete a consumed auto-handoff doc. The one file-deletion seam the frontend
/// has, so it can only ever remove what auto-handoff itself created: a bare
/// `continue-*.md` name (no path separators) directly inside `dir`. A doc that
/// is already gone is fine - the goal is its absence.
#[tauri::command]
pub async fn handoff_doc_delete(dir: String, name: String) -> Result<(), String> {
    let is_handoff_doc = name.starts_with(HANDOFF_DOC_PREFIX)
        && Path::new(&name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(HANDOFF_DOC_EXTENSION))
        && !name.contains(['/', '\\']);
    if !is_handoff_doc {
        return Err("only a continue-*.md handoff doc can be deleted".into());
    }
    let path = Path::new(&dir).join(&name);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    Ok(())
}
