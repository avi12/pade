//! The window a session belongs to, shared with the session's output pump.

use std::sync::{Arc, RwLock};

use super::SessionError;

/// The window a session belongs to. Shared between the registry entry and the
/// session's output pump, so moving a live session to another window
/// (`transfer`) re-routes its output from the very next chunk without
/// restarting anything.
#[derive(Clone, Debug, Default)]
pub(in crate::terminal::session) struct SessionOwner(Arc<RwLock<String>>);

impl SessionOwner {
    pub(super) fn new(label: String) -> Self {
        Self(Arc::new(RwLock::new(label)))
    }

    /// The owning window's label - empty when the lock is poisoned, which no
    /// window label matches, so a poisoned owner reads as foreign to everyone.
    pub(in crate::terminal::session) fn label(&self) -> String {
        self.0.read().map(|label| label.clone()).unwrap_or_default()
    }

    pub(super) fn is(&self, label: &str) -> bool {
        self.0.read().is_ok_and(|owner| owner.as_str() == label)
    }

    pub(super) fn set(&self, label: &str) -> Result<(), SessionError> {
        let mut owner = self.0.write()?;
        label.clone_into(&mut owner);
        Ok(())
    }
}
