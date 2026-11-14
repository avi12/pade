//! The live session registry: every PTY session keyed by id, the window that
//! owns each, and the one owner check every operation makes. Runs inside the
//! PTY host process; the app reaches it only through the host's protocol.
//!
//! Everything here is synchronous. Spawning and ending a session block on the
//! operating system, so callers run those off any async worker; the registry
//! lock itself is only ever held for a map lookup, never across process or PTY
//! I/O, so a slow teardown cannot stall another pane's input.

mod error;
mod owner;
mod pty_handle;
mod teardown;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::util::{owner_access, OwnerAccess};

pub(crate) use self::error::SessionError;
pub(super) use self::owner::SessionOwner;
use self::pty_handle::Pty;
use self::teardown::Teardowns;

use super::launch::{launch_process, pty_size, SessionLaunch};
use super::output::{spawn_output_pump, History, HistorySnapshot, OutputBuffers, SessionEvents};

/// How many sessions one window may hold.
const MAXIMUM_SESSIONS_PER_WINDOW: usize = 32;

/// How long ending every session waits for teardowns still running elsewhere -
/// long enough for a window's worth of process-tree kills, never a hang behind
/// a wedged one.
pub(crate) const EXIT_TEARDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// How often each session's exit monitor checks whether its agent process has
/// exited on its own (see `spawn_exit_monitor`).
const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// One live session as a listing reports it: its owner window, what it runs
/// (`command` - the spawn request's agent name, `None` for the default-shell
/// fallback) and where (`cwd`). Deliberately no idleness signal: the frontend's
/// session-status store (the output-quiet detector) is the one authority on
/// idle, and a second wire-level gauge alongside it could only drift.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HostedSession {
    pub id: String,
    pub owner: String,
    pub command: Option<String>,
    pub cwd: Option<String>,
}

/// All live PTY sessions, keyed by session id, plus the sessions already out
/// of the map but still being ended.
pub(crate) struct SessionRegistry {
    sessions: Mutex<HashMap<String, Arc<Pty>>>,
    teardowns: Arc<Teardowns>,
    events: Arc<dyn SessionEvents>,
}

impl SessionRegistry {
    pub(crate) fn new(events: Arc<dyn SessionEvents>) -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            teardowns: Arc::new(Teardowns::default()),
            events,
        })
    }

    /// Start session `id` for window `owner` - a no-op when that window already
    /// runs it, so a terminal re-attaching to a live session never restarts it.
    /// Blocking: opens a PTY and starts a process.
    pub(crate) fn spawn(
        self: &Arc<Self>,
        id: &str,
        owner: &str,
        launch: &SessionLaunch,
    ) -> Result<(), SessionError> {
        {
            let sessions = self.sessions.lock()?;
            let is_already_live = session_access(&sessions, id, owner)?.is_some();
            if is_already_live {
                return Ok(());
            }
            let owner_session_count = sessions
                .values()
                .filter(|pty| pty.owner.matches_label(owner))
                .count();
            if owner_session_count >= MAXIMUM_SESSIONS_PER_WINDOW {
                return Err(SessionError::WindowFull);
            }
        }

        // Only the registry lock publishes the completed session, so existing
        // panes stay responsive while a new CLI starts.
        let launched = launch_process(launch)?;
        let transcript = Arc::new(Mutex::new(String::new()));
        let history = Arc::new(Mutex::new(History::default()));
        let session_owner = SessionOwner::new(owner.to_string());
        let pty = Arc::new(Pty {
            owner: session_owner.clone(),
            master: Mutex::new(launched.master),
            writer: Mutex::new(launched.writer),
            child: Mutex::new(launched.child),
            command: launch.command.clone(),
            cwd: launched.cwd,
            transcript: Arc::clone(&transcript),
            history: Arc::clone(&history),
        });

        // A concurrent duplicate is ended here, which terminates the unregistered
        // child through `Pty::drop`.
        let duplicate = {
            let mut sessions = self.sessions.lock()?;
            let existing = session_access(&sessions, id, owner).map(|found| found.is_some());
            if existing == Ok(false) {
                sessions.insert(id.to_string(), pty);
                None
            } else {
                Some((pty, existing))
            }
        };
        if let Some((pty, existing)) = duplicate {
            self.end_now(vec![pty]);
            return existing.map(|_| ());
        }

        spawn_output_pump(
            Arc::clone(&self.events),
            session_owner,
            id.to_string(),
            launched.reader,
            OutputBuffers {
                transcript,
                history,
            },
        );
        spawn_exit_monitor(Arc::downgrade(self), id.to_string());
        Ok(())
    }

    /// Type `data` into session `id` as if from its keyboard.
    pub(crate) fn write(&self, owner: &str, id: &str, data: &str) -> Result<(), SessionError> {
        let Some(pty) = self.owned(owner, id)? else {
            return Ok(());
        };
        let mut writer = pty.writer.lock()?;
        writer
            .write_all(data.as_bytes())
            .map_err(SessionError::failed)?;
        writer.flush().map_err(SessionError::failed)
    }

    pub(crate) fn resize(
        &self,
        owner: &str,
        id: &str,
        columns: u16,
        rows: u16,
    ) -> Result<(), SessionError> {
        let Some(pty) = self.owned(owner, id)? else {
            return Ok(());
        };
        let master = pty.master.lock()?;
        master
            .resize(pty_size(rows, columns))
            .map_err(SessionError::failed)
    }

    /// Close a session: dropping it kills and reaps the agent process (see `Pty`'s
    /// `Drop`), so by the time this returns the session's cwd is unlocked and its
    /// workspace can be deleted or moved. Blocking.
    pub(crate) fn kill(&self, owner: &str, id: &str) -> Result<(), SessionError> {
        let pty = {
            let mut sessions = self.sessions.lock()?;
            session_access(&sessions, id, owner)?;
            sessions.remove(id)
        };
        self.end_now(pty.into_iter().collect());
        Ok(())
    }

    /// Every live session `owner` holds, or every live session at all for `None`:
    /// the roster a reloaded frontend re-attaches against, and the one a
    /// restarted app recreates its windows from. A listed session is alive by
    /// construction: the exit monitor drops a dead one. Sorted by id so the
    /// order is deterministic.
    pub(crate) fn list(&self, owner: Option<&str>) -> Result<Vec<HostedSession>, SessionError> {
        let sessions = self.sessions.lock()?;
        let mut roster: Vec<HostedSession> = sessions
            .iter()
            .filter(|(_, pty)| owner.is_none_or(|owner| pty.owner.matches_label(owner)))
            .map(|(id, pty)| HostedSession {
                id: id.clone(),
                owner: pty.owner.label(),
                command: pty.command.clone(),
                cwd: pty.cwd.clone(),
            })
            .collect();
        drop(sessions);
        roster.sort_unstable_by(|left, right| left.id.cmp(&right.id));
        Ok(roster)
    }

    /// How many sessions are live - the host's idle check.
    pub(crate) fn count(&self) -> usize {
        self.sessions.lock().map_or(0, |sessions| sessions.len())
    }

    /// Everything a terminal needs to paint a session it is attaching to
    /// mid-flight. Empty for an unknown session.
    pub(crate) fn history(&self, owner: &str, id: &str) -> Result<HistorySnapshot, SessionError> {
        let Some(pty) = self.owned(owner, id)? else {
            return Ok(HistorySnapshot::default());
        };
        Ok(pty
            .history
            .lock()
            .map(|buffer| buffer.snapshot())
            .unwrap_or_default())
    }

    /// A session's current plain-text transcript, empty if it is unknown.
    pub(crate) fn transcript(&self, owner: &str, id: &str) -> Result<String, SessionError> {
        let Some(pty) = self.owned(owner, id)? else {
            return Ok(String::new());
        };
        Ok(pty
            .transcript
            .lock()
            .map(|buffer| buffer.clone())
            .unwrap_or_default())
    }

    /// The session's root process id, owner-checked. `None` when the session is
    /// gone or has no live process.
    pub(crate) fn root_process_id(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<Option<u32>, SessionError> {
        let Some(pty) = self.owned(owner, id)? else {
            return Ok(None);
        };
        let child = pty.child.lock()?;
        Ok(child.process_id())
    }

    /// Hand live sessions from window `from` to window `to` - the move behind
    /// "open this session in a new window" and "merge this window into another".
    /// All or nothing: every id must be a session `from` owns, or none moves. The
    /// processes keep running untouched; only where their output goes and who may
    /// drive them changes.
    pub(crate) fn transfer(
        &self,
        from: &str,
        to: &str,
        ids: &[String],
    ) -> Result<(), SessionError> {
        let sessions = self.sessions.lock()?;
        let moving = ids
            .iter()
            .map(|id| match session_access(&sessions, id, from)? {
                Some(pty) => Ok(Arc::clone(pty)),
                None => Err(SessionError::Missing { id: id.clone() }),
            })
            .collect::<Result<Vec<_>, SessionError>>()?;
        let receiving_count = sessions
            .values()
            .filter(|pty| pty.owner.matches_label(to))
            .count();
        if receiving_count + moving.len() > MAXIMUM_SESSIONS_PER_WINDOW {
            return Err(SessionError::ReceivingWindowFull);
        }
        // Only an owner label changes hands - a pointer-sized write per session -
        // so it happens under the registry lock and no reader sees half a move.
        for pty in moving {
            pty.owner.set(to)?;
        }
        drop(sessions);
        Ok(())
    }

    /// End every session `owner` holds - a window closed for good, so nothing it
    /// ran may keep running unreachable. Returns at once: the teardown runs on
    /// its own thread, counted so `kill_all` waits for it.
    pub(crate) fn kill_owned(&self, owner: &str) {
        let Ok(mut sessions) = self.sessions.lock() else {
            return;
        };
        let owned_ids: Vec<String> = sessions
            .iter()
            .filter(|(_, pty)| pty.owner.matches_label(owner))
            .map(|(id, _)| id.clone())
            .collect();
        let ended: Vec<Arc<Pty>> = owned_ids
            .iter()
            .filter_map(|id| sessions.remove(id))
            .collect();
        drop(sessions);
        if ended.is_empty() {
            return;
        }
        let teardown = self.teardowns.begin();
        std::thread::spawn(move || {
            drop(ended);
            drop(teardown);
        });
    }

    /// Terminate every live session and wait for the sessions already being
    /// ended elsewhere (a closed window's, a tab closed just before) - the app
    /// is exiting, or the host is retiring. Blocking, bounded by
    /// `EXIT_TEARDOWN_TIMEOUT`.
    pub(crate) fn kill_all(&self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            let all = std::mem::take(&mut *sessions);
            drop(sessions);
            self.end_now(all.into_values().collect());
        }
        self.teardowns.wait_for_all(EXIT_TEARDOWN_TIMEOUT);
    }

    /// End sessions already taken out of the map, on the calling thread, counted
    /// so a concurrent `kill_all` waits for them.
    fn end_now(&self, ended: Vec<Arc<Pty>>) {
        let teardown = self.teardowns.begin();
        drop(ended);
        drop(teardown);
    }

    /// Session `id` when `owner` may use it, released from the registry lock so
    /// the caller's PTY or process work never holds it. `None` for an unknown id.
    fn owned(&self, owner: &str, id: &str) -> Result<Option<Arc<Pty>>, SessionError> {
        let sessions = self.sessions.lock()?;
        Ok(session_access(&sessions, id, owner)?.cloned())
    }
}

/// The one owner check every session operation makes: the session when `owner`
/// spawned it, `None` when no session has the id, an error when another window
/// owns it.
fn session_access<'a>(
    sessions: &'a HashMap<String, Arc<Pty>>,
    id: &str,
    owner: &str,
) -> Result<Option<&'a Arc<Pty>>, SessionError> {
    let session = sessions.get(id);
    let session_owner = session.map(|pty| pty.owner.label());
    match owner_access(session_owner.as_deref(), owner) {
        OwnerAccess::Vacant | OwnerAccess::Owned => Ok(session),
        OwnerAccess::Foreign => Err(SessionError::Foreign),
    }
}

/// Reap the agent when it exits on its own. The reader's EOF alone can't be
/// relied on for that: on Windows the `ConPTY` host (conhost) keeps the master
/// pipe open after the child dies, so the read blocks forever, no exit ever
/// reaches the frontend, and the dead session's conhost lingers. Poll the child
/// instead; once it has exited, drop the session - that closes the `ConPTY`,
/// which EOFs the reader, whose end-of-pump exit is the one authoritative
/// source of the exit event (DRY). A hand-closed session is already out of the
/// map, which ends this thread too. The registry lock only resolves the session
/// `Arc`; process polling and destruction happen outside it.
fn spawn_exit_monitor(registry: Weak<SessionRegistry>, id: String) {
    std::thread::spawn(move || loop {
        std::thread::sleep(CHILD_POLL_INTERVAL);
        let Some(registry) = registry.upgrade() else {
            break;
        };
        let Ok(Some(pty)) = registry
            .sessions
            .lock()
            .map(|sessions| sessions.get(&id).cloned())
        else {
            break;
        };
        let is_still_running = pty
            .child
            .lock()
            .is_ok_and(|mut child| matches!(child.try_wait(), Ok(None)));
        if is_still_running {
            continue;
        }
        // A caller can kill and immediately recreate an id. Only remove the
        // exact PTY this monitor owns; a stale monitor must not reap its successor.
        let removed = registry.sessions.lock().ok().and_then(|mut sessions| {
            let is_this_session = sessions
                .get(&id)
                .is_some_and(|active| Arc::ptr_eq(active, &pty));
            is_this_session.then(|| sessions.remove(&id)).flatten()
        });
        drop(pty);
        if let Some(removed) = removed {
            registry.end_now(vec![removed]);
        }
        break;
    });
}

#[cfg(test)]
mod tests;
