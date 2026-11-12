//! One live PTY session: its master, child, writer and buffers, killed and reaped on drop.

use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};

use portable_pty::{Child, MasterPty};

use super::SessionOwner;
use crate::terminal::session::output::History;

/// One live session.
pub(super) struct Pty {
    /// The window allowed to re-adopt and control this session.
    pub(super) owner: SessionOwner,
    pub(super) master: Mutex<Box<dyn MasterPty + Send>>,
    pub(super) writer: Mutex<Box<dyn Write + Send>>,
    /// The agent process itself. Held so that ending a session can end the
    /// process - see `Drop`.
    pub(super) child: Mutex<Box<dyn Child + Send + Sync>>,
    /// The command the spawn asked for (the agent's canonical name, e.g.
    /// `claude`) - `None` when the spawn fell through to the default shell.
    /// Kept so a reloaded frontend can learn what each session runs.
    pub(super) command: Option<String>,
    /// The directory the session was spawned into - its workspace mapping.
    pub(super) cwd: Option<String>,
    /// Rolling, ANSI-stripped tail of this session's output - the context the
    /// AI session-namer reads. Its own lock so the read thread never contends
    /// with the sessions map.
    pub(super) transcript: Arc<Mutex<String>>,
    /// Everything the terminal would need to paint this session from scratch.
    /// A PTY has no scrollback of its own: whatever the frontend never received,
    /// or received and then threw away, is gone. So a terminal that attaches to a
    /// session already in flight - a remounted component, a reloaded window, a
    /// restarted app - has nothing to draw and sits blank while the agent, quite
    /// happily, waits for input. Keeping the raw stream is what lets it catch up.
    pub(super) history: Arc<Mutex<History>>,
}

impl Drop for Pty {
    /// Ending a session has to end its whole process *tree*. Dropping the PTY only
    /// hangs the child up, and a shell (or an agent mid-task) can outlive that.
    /// Worse, on Windows `Child::kill` terminates the immediate child alone, so
    /// the agent's own descendants - the shell it runs under, a `node`, a tool it
    /// spawned - survive and keep the working directory locked, and the workspace
    /// can then be neither deleted nor moved ("used by another process", os error
    /// 32). Kill the tree first (while it's still walkable from the child), then
    /// reap the immediate child, so the folder is free the moment the session ends.
    fn drop(&mut self) {
        let child = self.child.get_mut().unwrap_or_else(PoisonError::into_inner);
        #[cfg(windows)]
        if let Some(process_id) = child.process_id() {
            crate::terminal::process_tree::kill_process_tree(process_id);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}
