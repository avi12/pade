//! Everything that runs inside a terminal pane: the PTY sessions agents and
//! shells live in (`session`, run by the separate PTY host process -
//! `pty_host` - and reached through the app's commands in `pty`), the agent
//! process trees (`process_tree`), the task-runner dock's captured processes (`runner`)
//! and the tasks it discovers (`tasks`), and how a session is painted - each
//! agent's per-scheme theming (`theming`) and the colour schemes on offer
//! (`schemes`).

pub(crate) mod process_tree;
pub(crate) mod pty;
pub(crate) mod pty_host;
pub(crate) mod runner;
pub(crate) mod schemes;
pub(crate) mod session;
pub(crate) mod tasks;
pub(crate) mod theming;
