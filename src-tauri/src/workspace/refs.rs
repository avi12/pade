//! Move-directory reference updater.
//!
//! When PADE moves a workspace folder (`old` path → `new` path), external tools
//! that remember the old absolute path - each agent's memory of the project
//! (see `agents`), the `JetBrains` / VS Code / Cursor "recent projects" lists -
//! would silently lose the project. This module re-points those references so
//! the linked tools keep working.
//!
//! It is strictly **best-effort**: every adapter swallows its own errors (a
//! missing, absent, or locked file must never fail the move). We only ever touch
//! data the tool already persisted locally - no network, no CLI.
//!
//! Gating: the agent adapters touch only entries for the moved folder or
//! something inside it. Each IDE adapter runs only when the moved directory
//! actually carries that IDE's marker dir (e.g. `<new>/.idea`, `<new>/.vscode`),
//! so we never rewrite recents for an IDE the project was never opened in. The
//! IDE adapters are additionally Windows-only (that's where PADE ships);
//! off-Windows they are no-ops so the crate still compiles everywhere.

mod ide_recents;
mod package_manager;
mod paths;
mod symlinks;
#[cfg(test)]
mod tests;

use super::RelocateStep;
use ide_recents::rewrite_ide_recents;
use package_manager::reconcile_package_manager;
pub use paths::moved_path;
use paths::rewrite_file_forms;
use std::path::Path;
use symlinks::fix_project_symlinks;

mod agents;

/// Best-effort: re-point every external reference from `old` to `new` (absolute
/// dir paths). Per-adapter errors are logged and ignored - one failing tool must
/// never abort the others or the move itself.
/// `report` hears each step as it starts, so a move that takes a while on a
/// large project can say where it is.
pub fn update_references(old: &str, new: &str, report: &dyn Fn(RelocateStep)) {
    let new_directory = Path::new(new);
    report(RelocateStep::AgentMemories);
    agents::relocate_memories(old, new);
    report(RelocateStep::EditorRecents);
    rewrite_ide_recents(new_directory, old, new);
    report(RelocateStep::Links);
    fix_project_symlinks(new_directory, old, new);
    reconcile_package_manager(new_directory);
}
