//! The app's side of the PTY host: finds this instance's host (starting one
//! when there is none, replacing one it cannot use), keeps one connection to
//! it, correlates requests with replies, and turns the host's session output
//! into the frontend's `pty://data` / `pty://exit` events.

mod connection;
mod lifecycle;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;

use crate::terminal::session::launch::SessionLaunch;
use crate::terminal::session::output::HistorySnapshot;
use crate::terminal::session::registry::{HostedSession, SessionError};

use super::protocol::{Outcome, Request};

use self::connection::Connection;
use self::lifecycle::establish;

/// How long one request may wait for its reply before the caller gives up -
/// generous, because a spawn resolves and starts an agent and a kill waits for
/// a process tree to end.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Why the app could not get an answer from the PTY host.
#[derive(Debug)]
pub(crate) enum HostError {
    /// No host could be found or started.
    Unavailable(String),
    /// The connection ended before the reply came - the host exited or crashed.
    Disconnected,
    TimedOut,
    /// The host answered with something this request never produces.
    UnexpectedReply,
    /// The session engine refused the request.
    Session(SessionError),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(reason) => {
                write!(formatter, "the terminal host is unavailable: {reason}")
            }
            Self::Disconnected => formatter.write_str("the terminal host went away"),
            Self::TimedOut => formatter.write_str("the terminal host did not answer in time"),
            Self::UnexpectedReply => formatter.write_str("the terminal host answered out of turn"),
            Self::Session(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for HostError {}

impl From<std::io::Error> for HostError {
    fn from(error: std::io::Error) -> Self {
        Self::Unavailable(error.to_string())
    }
}

impl From<super::protocol::FrameError> for HostError {
    fn from(error: super::protocol::FrameError) -> Self {
        Self::Unavailable(error.to_string())
    }
}

/// The app's handle on its PTY host - managed Tauri state. Connects on first
/// use and again after the host went away.
pub(crate) struct HostClient {
    app: AppHandle,
    slot: tokio::sync::Mutex<Option<Arc<Connection>>>,
}

impl HostClient {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self {
            app,
            slot: tokio::sync::Mutex::new(None),
        }
    }

    /// The open connection, establishing one first when there is none. Callers
    /// queue here while a host is being found or started.
    async fn connection(&self) -> Result<Arc<Connection>, HostError> {
        let mut slot = self.slot.lock().await;
        if let Some(connection) = slot
            .as_ref()
            .filter(|connection| connection.is_open.load(Ordering::Acquire))
        {
            return Ok(Arc::clone(connection));
        }
        let connection = establish(&self.app).await?;
        *slot = Some(Arc::clone(&connection));
        Ok(connection)
    }

    /// The open connection if there is one right now - never starting a host,
    /// and never waiting on one being started.
    fn current(&self) -> Option<Arc<Connection>> {
        let slot = self.slot.try_lock().ok()?;
        slot.as_ref()
            .filter(|connection| connection.is_open.load(Ordering::Acquire))
            .cloned()
    }

    async fn call(&self, request: Request) -> Result<Outcome, HostError> {
        self.connection().await?.call(request).await
    }

    async fn call_for_done(&self, request: Request) -> Result<(), HostError> {
        match self.call(request).await? {
            Outcome::Done => Ok(()),
            _ => Err(HostError::UnexpectedReply),
        }
    }

    pub(crate) async fn spawn(
        &self,
        id: &str,
        owner: &str,
        launch: SessionLaunch,
    ) -> Result<(), HostError> {
        let connection = self.connection().await?;
        let request = Request::Spawn {
            id: id.to_string(),
            owner: owner.to_string(),
            launch,
        };
        match connection.call(request).await? {
            Outcome::Done => {
                connection.note_session(id, owner);
                Ok(())
            }
            _ => Err(HostError::UnexpectedReply),
        }
    }

    pub(crate) async fn write(&self, owner: &str, id: &str, data: &str) -> Result<(), HostError> {
        self.call_for_done(Request::Write {
            owner: owner.to_string(),
            id: id.to_string(),
            data: data.to_string(),
        })
        .await
    }

    pub(crate) async fn resize(
        &self,
        owner: &str,
        id: &str,
        columns: u16,
        rows: u16,
    ) -> Result<(), HostError> {
        self.call_for_done(Request::Resize {
            owner: owner.to_string(),
            id: id.to_string(),
            columns,
            rows,
        })
        .await
    }

    pub(crate) async fn kill(&self, owner: &str, id: &str) -> Result<(), HostError> {
        self.call_for_done(Request::Kill {
            owner: owner.to_string(),
            id: id.to_string(),
        })
        .await
    }

    /// `owner`'s sessions, or every session the host runs for `None`.
    pub(crate) async fn list(&self, owner: Option<&str>) -> Result<Vec<HostedSession>, HostError> {
        let request = Request::List {
            owner: owner.map(ToString::to_string),
        };
        match self.call(request).await? {
            Outcome::Sessions { sessions } => Ok(sessions),
            _ => Err(HostError::UnexpectedReply),
        }
    }

    pub(crate) async fn history(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<HistorySnapshot, HostError> {
        let request = Request::History {
            owner: owner.to_string(),
            id: id.to_string(),
        };
        match self.call(request).await? {
            Outcome::History { snapshot } => Ok(snapshot),
            _ => Err(HostError::UnexpectedReply),
        }
    }

    pub(crate) async fn transcript(&self, owner: &str, id: &str) -> Result<String, HostError> {
        let request = Request::Transcript {
            owner: owner.to_string(),
            id: id.to_string(),
        };
        match self.call(request).await? {
            Outcome::Transcript { text } => Ok(text),
            _ => Err(HostError::UnexpectedReply),
        }
    }

    pub(crate) async fn root_process_id(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<Option<u32>, HostError> {
        let request = Request::RootProcessId {
            owner: owner.to_string(),
            id: id.to_string(),
        };
        match self.call(request).await? {
            Outcome::RootProcessId { process_id } => Ok(process_id),
            _ => Err(HostError::UnexpectedReply),
        }
    }

    pub(crate) async fn transfer(
        &self,
        from: &str,
        to: &str,
        ids: &[String],
    ) -> Result<(), HostError> {
        let connection = self.connection().await?;
        let request = Request::Transfer {
            from: from.to_string(),
            to: to.to_string(),
            ids: ids.to_vec(),
        };
        match connection.call(request).await? {
            Outcome::Done => {
                for id in ids {
                    connection.note_session(id, to);
                }
                Ok(())
            }
            _ => Err(HostError::UnexpectedReply),
        }
    }

    /// End every session window `owner` holds. Never starts a host: with none
    /// connected, there is nothing of this app's to end.
    pub(crate) async fn kill_owned(&self, owner: &str) -> Result<(), HostError> {
        let Some(connection) = self.current() else {
            return Ok(());
        };
        let request = Request::KillOwned {
            owner: owner.to_string(),
        };
        connection.call(request).await.map(drop)
    }

    /// End every session and wait for the teardowns - the app is exiting. Never
    /// starts a host.
    pub(crate) async fn kill_all(&self) -> Result<(), HostError> {
        let Some(connection) = self.current() else {
            return Ok(());
        };
        connection.call(Request::KillAll).await.map(drop)
    }
}

#[cfg(test)]
mod tests;
