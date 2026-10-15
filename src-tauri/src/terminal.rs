//! Everything that runs inside a terminal pane: the PTY sessions agents and
//! shells live in (`pty`), the task-runner dock's captured processes (`runner`)
//! and the tasks it discovers (`tasks`), and how a session is painted - each
//! agent's per-scheme theming (`theming`) and the colour schemes on offer
//! (`schemes`).

pub(crate) mod pty;
pub(crate) mod runner;
pub(crate) mod schemes;
pub(crate) mod tasks;
pub(crate) mod theming;
