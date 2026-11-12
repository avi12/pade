//! External IDE integration - open the current project in the editor of choice.
//!
//! ADE is agentic-first, but you still reach for a full IDE sometimes. It
//! detects installed editors (by their CLI launcher) and opens the active
//! project directory in the one you pick.

mod census;
mod declarations;
mod editor_registry;
pub(crate) mod editors;
pub(crate) mod kinds;
pub(crate) mod languages;
mod project_registry;
pub(crate) mod suggest;
pub(crate) use languages::{note_working_tree_change, project_key};
pub(crate) mod open;
#[cfg(test)]
mod tests;
