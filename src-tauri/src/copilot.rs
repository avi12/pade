//! Copilot (Windows) as an optional name source — **not yet wired**.
//!
//! The native token path needs `msalruntime.dll` (Windows-only) plus the
//! reverse-engineered `c/api/chat` protocol. Full integration notes — client id,
//! scope, the `consumer_passthrough` MSAL param, and the chat-socket handshake —
//! live in `docs/handoff-autoname-temp-projects.md`. Until that lands this
//! returns `None` so naming falls through to the heuristic. It slots into the
//! `Namer` chain in `naming.rs` ahead of the heuristic on Windows.

use crate::naming::{NameContext, Namer};

pub struct CopilotNamer;

impl Namer for CopilotNamer {
    fn suggest(&self, _ctx: &NameContext) -> Option<String> {
        None
    }
}

#[cfg(windows)]
impl CopilotNamer {
    /// Generate a session name from its recent transcript. Stage 2 wires the
    /// real Copilot chat call (MSAL token + `c/api/chat` socket); until then
    /// naming falls through to the agent CLI. `self` will carry the token/client
    /// once wired, so this stays a method.
    #[allow(clippy::unused_self)]
    pub fn suggest_session(&self, _transcript: &str) -> Option<String> {
        None
    }
}
