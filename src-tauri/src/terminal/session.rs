//! The session engine: runs agent CLIs unmodified in real pseudo-terminals.
//!
//! ToS-clean by construction - PADE only reads a PTY's output stream and
//! writes the user's keystrokes back in. It never scripts around the agent.
//!
//! Multi-session: each agent runs in its own PTY keyed by a session id and
//! owned by one window. The engine lives in the PTY host process (see
//! `pty_host`), so the agents outlive an app restart; it knows nothing of the
//! transport and is testable with none.

pub(crate) mod launch;
pub(crate) mod output;
pub(crate) mod registry;
