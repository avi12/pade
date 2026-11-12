//! Workspace & projects.
//!
//! Two launch modes:
//!  - Launched *inside* a project directory → use it directly (the agent rules
//!    apply to that dir).
//!  - Launched with no project → the project onboarding lets the user pick root
//!    directories, browse the projects inside them, open one, or create a new
//!    one.
//!
//! Settings (roots, default/per-project agent) persist to the OS config dir.
//! They are plain JSON so they can later live in a git-backed shelf for sync.
//!
//! The submodules cover everything else that reads or reshapes an open
//! project: the Change Feed's watcher (`watcher`, filtering through
//! `gitignore`, whose segment matcher `glob` it shares with `members`), the
//! project's make-up (`members`, `ide`, `config`, `mcp`), and keeping a moved
//! or saved workspace working (`refs`, `provision`); `settings_sync` carries
//! the settings writes other PADE processes make to this one's windows.

pub(crate) mod delete;
pub(crate) mod handoff_doc;
pub(crate) mod labels;
pub(crate) mod launch;
mod patient_io;
pub(crate) mod preferences;
pub(crate) mod recents;
pub(crate) mod relocate;
pub(crate) mod roots;
pub(crate) mod save;
pub(crate) mod store;
pub(crate) mod temp;
#[cfg(test)]
mod tests;
pub(crate) use launch::canonical_launch_directory;
#[cfg(windows)]
pub(crate) use launch::webview_data_directory;
pub use preferences::add_editor;
pub use preferences::remove_editor;
pub use preferences::set_project_editor;
pub(crate) use recents::mark_left;
pub(crate) use relocate::RelocateStep;
pub(crate) use roots::canonical_path;
pub(crate) use roots::validated_child_path;
pub(crate) use store::ensure_config_directory;
pub use store::init;
pub(crate) use store::load;
pub use store::migrate_from_ade;
pub use store::AddedEditor;
pub use store::Settings;
pub use temp::is_owned;
pub(crate) mod config;
pub(crate) mod gitignore;
pub(crate) mod glob;
pub(crate) mod ide;
pub(crate) mod mcp;
pub(crate) mod members;
pub(crate) mod provision;
pub(crate) mod refs;
mod settings_sync;
pub(crate) mod watcher;
