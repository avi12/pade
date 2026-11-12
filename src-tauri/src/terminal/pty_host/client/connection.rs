//! One live connection to the PTY host: the writer queue, request-reply correlation, the known-session ledger and the reader task that turns host messages into events.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::AppHandle;
use tokio::io::{BufReader, ReadHalf, WriteHalf};
use tokio::sync::{mpsc, oneshot};

use crate::terminal::pty_host::protocol::{
    read_frame, write_frame, HostMessage, Outcome, Request, RequestFrame,
};
use crate::terminal::pty_host::transport::ClientStream;

use super::{HostError, REQUEST_TIMEOUT};

/// Requests queued for the connection's writer before a caller waits.
pub(super) const OUTBOUND_CAPACITY: usize = 256;

pub(super) type Reader = BufReader<ReadHalf<ClientStream>>;

pub(super) type Writer = WriteHalf<ClientStream>;

/// An open, handshaken stream to a host.
pub(super) struct Link {
    pub(super) reader: Reader,
    pub(super) writer: Writer,
}

/// What the host's state file pointed at, once asked.
pub(super) enum Answer {
    Welcome {
        link: Link,
        executable_fingerprint: String,
        session_count: usize,
    },
    VersionMismatch,
    Busy,
}

/// One live connection to the host.
pub(super) struct Connection {
    pub(super) outbound: mpsc::Sender<RequestFrame>,
    pub(super) pending: Mutex<HashMap<u64, oneshot::Sender<Outcome>>>,
    pub(super) next_request_id: AtomicU64,
    pub(super) is_open: AtomicBool,
    /// Which window each session the host runs belongs to, as far as this
    /// connection has seen (the roster at connect, then spawns, moves and
    /// exits). Kept for one moment only: should the host vanish, its sessions
    /// went with it, and each owner window must still get its `pty://exit`.
    pub(super) known_sessions: Mutex<HashMap<String, String>>,
}

impl Connection {
    pub(super) async fn call(&self, request: Request) -> Result<Outcome, HostError> {
        if !self.is_open.load(Ordering::Acquire) {
            return Err(HostError::Disconnected);
        }
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| HostError::Disconnected)?
            .insert(request_id, sender);
        let frame = RequestFrame {
            request_id,
            request,
        };
        if self.outbound.send(frame).await.is_err() {
            self.forget(request_id);
            return Err(HostError::Disconnected);
        }
        match tokio::time::timeout(REQUEST_TIMEOUT, receiver).await {
            Ok(Ok(Outcome::Failed { error })) => Err(HostError::Session(error)),
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(_)) => Err(HostError::Disconnected),
            Err(_) => {
                self.forget(request_id);
                Err(HostError::TimedOut)
            }
        }
    }

    pub(super) fn forget(&self, request_id: u64) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&request_id);
        }
    }

    pub(super) fn resolve(&self, request_id: u64, outcome: Outcome) {
        let waiting = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&request_id));
        if let Some(waiting) = waiting {
            // The caller timed out and left; its reply has no one to go to.
            let _ = waiting.send(outcome);
        }
    }

    pub(super) fn note_session(&self, id: &str, owner: &str) {
        if let Ok(mut known) = self.known_sessions.lock() {
            known.insert(id.to_string(), owner.to_string());
        }
    }

    pub(super) fn forget_session(&self, id: &str) {
        if let Ok(mut known) = self.known_sessions.lock() {
            known.remove(id);
        }
    }

    /// Mark the connection dead: every waiting caller gets `Disconnected`, and
    /// the sessions it knew are returned once - for their exit events.
    pub(super) fn close(&self) -> HashMap<String, String> {
        self.is_open.store(false, Ordering::Release);
        if let Ok(mut pending) = self.pending.lock() {
            pending.clear();
        }
        self.known_sessions
            .lock()
            .map(|mut known| std::mem::take(&mut *known))
            .unwrap_or_default()
    }
}

/// Start the connection's writer and reader around a handshaken link.
pub(super) fn open_connection(app: &AppHandle, link: Link) -> Arc<Connection> {
    let (outbound, mut queue) = mpsc::channel::<RequestFrame>(OUTBOUND_CAPACITY);
    let connection = Arc::new(Connection {
        outbound,
        pending: Mutex::new(HashMap::new()),
        next_request_id: AtomicU64::new(1),
        is_open: AtomicBool::new(true),
        known_sessions: Mutex::new(HashMap::new()),
    });
    let Link { reader, mut writer } = link;
    {
        let connection = Arc::clone(&connection);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(frame) = queue.recv().await {
                if write_frame(&mut writer, &frame).await.is_err() {
                    break;
                }
            }
            report_lost_sessions(&app, &connection);
        });
    }
    tauri::async_runtime::spawn(read_host_messages(
        app.clone(),
        Arc::clone(&connection),
        reader,
    ));
    connection
}

pub(super) async fn seed_known_sessions(connection: &Connection) -> Result<(), HostError> {
    let Outcome::Sessions { sessions } = connection.call(Request::List { owner: None }).await?
    else {
        return Err(HostError::UnexpectedReply);
    };
    for session in sessions {
        connection.note_session(&session.id, &session.owner);
    }
    Ok(())
}

pub(super) async fn read_host_messages(
    app: AppHandle,
    connection: Arc<Connection>,
    mut reader: Reader,
) {
    let mut buffer = Vec::new();
    loop {
        let message = match read_frame::<_, HostMessage>(&mut reader, &mut buffer).await {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(error) => {
                eprintln!("[pty-host] lost the terminal host: {error}");
                break;
            }
        };
        match message {
            HostMessage::Reply {
                request_id,
                outcome,
            } => connection.resolve(request_id, outcome),
            HostMessage::Output {
                id,
                owner,
                data,
                sequence,
            } => crate::terminal::pty::emit_output(&app, &owner, &id, &data, sequence),
            HostMessage::Exited { id, owner } => {
                connection.forget_session(&id);
                crate::terminal::pty::emit_exit(&app, &owner, &id);
            }
        }
    }
    report_lost_sessions(&app, &connection);
}

/// The host is gone and its sessions with it: let each owner window run its
/// normal exit path. Runs once - the second caller finds nothing left.
pub(super) fn report_lost_sessions(app: &AppHandle, connection: &Connection) {
    for (id, owner) in connection.close() {
        crate::terminal::pty::emit_exit(app, &owner, &id);
    }
}
