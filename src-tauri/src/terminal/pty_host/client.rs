//! The app's side of the PTY host: finds this instance's host (starting one
//! when there is none, replacing one it cannot use), keeps one connection to
//! it, correlates requests with replies, and turns the host's session output
//! into the frontend's `pty://data` / `pty://exit` events.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::AppHandle;
use tokio::io::{BufReader, ReadHalf, WriteHalf};
use tokio::sync::{mpsc, oneshot};

use crate::terminal::session::launch::SessionLaunch;
use crate::terminal::session::output::HistorySnapshot;
use crate::terminal::session::registry::{HostedSession, SessionError, EXIT_TEARDOWN_TIMEOUT};

use super::discovery::{
    ensure_host_copy, executable_fingerprint, instance_key, prune_host_copies, read_state,
    HostPaths, HostState,
};
use super::protocol::{
    read_frame, write_frame, Greeting, HostMessage, Opening, Outcome, Request, RequestFrame,
    PROTOCOL_VERSION,
};
use super::transport::{self, ClientStream};

/// How long one request may wait for its reply before the caller gives up -
/// generous, because a spawn resolves and starts an agent and a kill waits for
/// a process tree to end.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// How long the host has to answer an `Opening`.
const GREETING_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a retiring host has to end its sessions and answer.
const RETIRE_TIMEOUT: Duration = EXIT_TEARDOWN_TIMEOUT.saturating_add(Duration::from_secs(5));
/// How long a freshly started host has to publish its state file.
const HOST_START_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// A host still attached to an app that just died answers `Busy` until it has
/// noticed; how often, and how far apart, to ask again before giving up on it.
const BUSY_RETRIES: u32 = 6;
const BUSY_RETRY_INTERVAL: Duration = Duration::from_millis(500);
/// Requests queued for the connection's writer before a caller waits.
const OUTBOUND_CAPACITY: usize = 256;

/// Why the app could not get an answer from the PTY host.
#[derive(Debug)]
pub enum HostError {
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

type Reader = BufReader<ReadHalf<ClientStream>>;
type Writer = WriteHalf<ClientStream>;

/// An open, handshaken stream to a host.
struct Link {
    reader: Reader,
    writer: Writer,
}

/// What the host's state file pointed at, once asked.
enum Answer {
    Welcome {
        link: Link,
        executable_fingerprint: String,
        session_count: usize,
    },
    VersionMismatch,
    Busy,
}

/// One live connection to the host.
struct Connection {
    outbound: mpsc::Sender<RequestFrame>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Outcome>>>,
    next_request_id: AtomicU64,
    is_open: AtomicBool,
    /// Which window each session the host runs belongs to, as far as this
    /// connection has seen (the roster at connect, then spawns, moves and
    /// exits). Kept for one moment only: should the host vanish, its sessions
    /// went with it, and each owner window must still get its `pty://exit`.
    known_sessions: Mutex<HashMap<String, String>>,
}

impl Connection {
    async fn call(&self, request: Request) -> Result<Outcome, HostError> {
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

    fn forget(&self, request_id: u64) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&request_id);
        }
    }

    fn resolve(&self, request_id: u64, outcome: Outcome) {
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

    fn note_session(&self, id: &str, owner: &str) {
        if let Ok(mut known) = self.known_sessions.lock() {
            known.insert(id.to_string(), owner.to_string());
        }
    }

    fn forget_session(&self, id: &str) {
        if let Ok(mut known) = self.known_sessions.lock() {
            known.remove(id);
        }
    }

    /// Mark the connection dead: every waiting caller gets `Disconnected`, and
    /// the sessions it knew are returned once - for their exit events.
    fn close(&self) -> HashMap<String, String> {
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

/// The app's handle on its PTY host - managed Tauri state. Connects on first
/// use and again after the host went away.
pub struct HostClient {
    app: AppHandle,
    slot: tokio::sync::Mutex<Option<Arc<Connection>>>,
}

impl HostClient {
    pub fn new(app: AppHandle) -> Self {
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

    pub async fn spawn(
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

    pub async fn write(&self, owner: &str, id: &str, data: &str) -> Result<(), HostError> {
        self.call_for_done(Request::Write {
            owner: owner.to_string(),
            id: id.to_string(),
            data: data.to_string(),
        })
        .await
    }

    pub async fn resize(
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

    pub async fn kill(&self, owner: &str, id: &str) -> Result<(), HostError> {
        self.call_for_done(Request::Kill {
            owner: owner.to_string(),
            id: id.to_string(),
        })
        .await
    }

    /// `owner`'s sessions, or every session the host runs for `None`.
    pub async fn list(&self, owner: Option<&str>) -> Result<Vec<HostedSession>, HostError> {
        let request = Request::List {
            owner: owner.map(ToString::to_string),
        };
        match self.call(request).await? {
            Outcome::Sessions { sessions } => Ok(sessions),
            _ => Err(HostError::UnexpectedReply),
        }
    }

    pub async fn history(&self, owner: &str, id: &str) -> Result<HistorySnapshot, HostError> {
        let request = Request::History {
            owner: owner.to_string(),
            id: id.to_string(),
        };
        match self.call(request).await? {
            Outcome::History { snapshot } => Ok(snapshot),
            _ => Err(HostError::UnexpectedReply),
        }
    }

    pub async fn transcript(&self, owner: &str, id: &str) -> Result<String, HostError> {
        let request = Request::Transcript {
            owner: owner.to_string(),
            id: id.to_string(),
        };
        match self.call(request).await? {
            Outcome::Transcript { text } => Ok(text),
            _ => Err(HostError::UnexpectedReply),
        }
    }

    pub async fn root_process_id(&self, owner: &str, id: &str) -> Result<Option<u32>, HostError> {
        let request = Request::RootProcessId {
            owner: owner.to_string(),
            id: id.to_string(),
        };
        match self.call(request).await? {
            Outcome::RootProcessId { process_id } => Ok(process_id),
            _ => Err(HostError::UnexpectedReply),
        }
    }

    pub async fn transfer(&self, from: &str, to: &str, ids: &[String]) -> Result<(), HostError> {
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
    pub async fn kill_owned(&self, owner: &str) -> Result<(), HostError> {
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
    pub async fn kill_all(&self) -> Result<(), HostError> {
        let Some(connection) = self.current() else {
            return Ok(());
        };
        connection.call(Request::KillAll).await.map(drop)
    }
}

/// Find, or start, this instance's host and connect to it.
async fn establish(app: &AppHandle) -> Result<Arc<Connection>, HostError> {
    let executable = std::env::current_exe()?;
    let fingerprint = {
        let executable = executable.clone();
        tauri::async_runtime::spawn_blocking(move || executable_fingerprint(&executable))
            .await
            .map_err(|error| HostError::Unavailable(error.to_string()))??
    };
    let instance = {
        let launch =
            tauri::async_runtime::spawn_blocking(crate::workspace::canonical_launch_directory)
                .await
                .map_err(|error| HostError::Unavailable(error.to_string()))?;
        instance_key(&launch)
    };

    // A host attached to another live app of this same instance (the same
    // project launched twice) is not ours to take: this app gets its own.
    let private_instance = format!("{instance}-{}", std::process::id());
    for key in [&instance, &private_instance] {
        let paths = HostPaths::for_instance(key)
            .ok_or_else(|| HostError::Unavailable("no per-user data folder".into()))?;
        if let Some(link) = attach_or_start(&paths, &executable, &fingerprint).await? {
            let connection = open_connection(app, link);
            seed_known_sessions(&connection).await?;
            prune_in_background(paths, fingerprint);
            return Ok(connection);
        }
    }
    Err(HostError::Unavailable(
        "another PADE holds this project's terminal host".into(),
    ))
}

/// A link to a usable host for `paths`, starting one when needed. `None` when
/// the host there stays attached to another app.
async fn attach_or_start(
    paths: &HostPaths,
    executable: &Path,
    fingerprint: &str,
) -> Result<Option<Link>, HostError> {
    let state_file = paths.state_file();
    if let Some(state) = read_state(&state_file) {
        match ask_with_patience(&state).await {
            Ok(Answer::Welcome {
                link,
                executable_fingerprint,
                session_count,
            }) => {
                let is_outdated_and_idle =
                    executable_fingerprint != fingerprint && session_count == 0;
                if !is_outdated_and_idle {
                    return Ok(Some(link));
                }
                // An idle host from an older build: replace it, so new sessions
                // run this build's launch code. Detach first so it can retire.
                drop(link);
                retire(&state).await?;
            }
            // A host speaking another protocol cannot serve this app, and must
            // not keep agents running where no window reaches them: retire it.
            Ok(Answer::VersionMismatch) => retire(&state).await?,
            Ok(Answer::Busy) => return Ok(None),
            Err(error) => eprintln!("[pty-host] the recorded host is gone ({error}); starting one"),
        }
        remove_if_unchanged(&state_file, &state);
    }

    start_host(paths, executable, fingerprint).await?;
    let state = wait_for_state(&state_file).await?;
    match ask_with_patience(&state).await? {
        Answer::Welcome { link, .. } => Ok(Some(link)),
        Answer::Busy => Ok(None),
        Answer::VersionMismatch => Err(HostError::Unavailable(
            "the host just started speaks another protocol".into(),
        )),
    }
}

/// Say hello, asking again for a few seconds while the host is still attached
/// to an app that has just died and not yet noticed.
async fn ask_with_patience(state: &HostState) -> Result<Answer, HostError> {
    for _ in 0..BUSY_RETRIES {
        let answer = ask(state).await?;
        if !matches!(answer, Answer::Busy) {
            return Ok(answer);
        }
        tokio::time::sleep(BUSY_RETRY_INTERVAL).await;
    }
    ask(state).await
}

async fn ask(state: &HostState) -> Result<Answer, HostError> {
    let opening = Opening::Hello {
        protocol_version: PROTOCOL_VERSION,
        token: state.token.clone(),
    };
    let (greeting, link) = open(state, &opening, GREETING_TIMEOUT).await?;
    match greeting {
        Greeting::Welcome {
            executable_fingerprint,
            session_count,
            ..
        } => Ok(Answer::Welcome {
            link,
            executable_fingerprint,
            session_count,
        }),
        Greeting::VersionMismatch { .. } => Ok(Answer::VersionMismatch),
        Greeting::Busy => Ok(Answer::Busy),
        Greeting::Retired => Err(HostError::UnexpectedReply),
    }
}

/// Connect, send `opening`, and read the host's greeting.
async fn open(
    state: &HostState,
    opening: &Opening,
    patience: Duration,
) -> Result<(Greeting, Link), HostError> {
    let stream = transport::connect(&state.endpoint).await?;
    let (read_half, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    write_frame(&mut writer, opening).await?;
    let mut buffer = Vec::new();
    let greeting = tokio::time::timeout(
        patience,
        read_frame::<_, Greeting>(&mut reader, &mut buffer),
    )
    .await
    .map_err(|_| HostError::TimedOut)??;
    // No greeting at all is how a host refuses a token it does not know.
    let greeting = greeting.ok_or(HostError::Disconnected)?;
    Ok((greeting, Link { reader, writer }))
}

/// Ask the host behind `state` to end its sessions and exit, and wait until it
/// has stopped listening.
async fn retire(state: &HostState) -> Result<(), HostError> {
    let opening = Opening::Retire {
        token: state.token.clone(),
    };
    for _ in 0..=BUSY_RETRIES {
        let greeting = match open(state, &opening, RETIRE_TIMEOUT).await {
            Ok((greeting, _)) => greeting,
            Err(HostError::Unavailable(_)) => return Ok(()),
            Err(error) => return Err(error),
        };
        if greeting == Greeting::Retired {
            return wait_until_gone(state).await;
        }
        tokio::time::sleep(BUSY_RETRY_INTERVAL).await;
    }
    Err(HostError::Unavailable(
        "the old terminal host would not retire".into(),
    ))
}

async fn wait_until_gone(state: &HostState) -> Result<(), HostError> {
    let deadline = tokio::time::Instant::now() + HOST_START_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        match transport::connect(&state.endpoint).await {
            Err(error) if transport::is_absent(&error) => return Ok(()),
            _ => tokio::time::sleep(POLL_INTERVAL).await,
        }
    }
    Err(HostError::TimedOut)
}

/// Delete a state file whose host is gone - unless it has changed since it was
/// read, which means a new host has just published itself.
fn remove_if_unchanged(state_file: &Path, stale: &HostState) {
    if read_state(state_file).as_ref() != Some(stale) {
        return;
    }
    if let Err(error) = std::fs::remove_file(state_file) {
        eprintln!(
            "[pty-host] could not clear {}: {error}",
            state_file.display()
        );
    }
}

async fn start_host(
    paths: &HostPaths,
    executable: &Path,
    fingerprint: &str,
) -> Result<(), HostError> {
    let paths = paths.clone();
    let executable = executable.to_path_buf();
    let fingerprint = fingerprint.to_string();
    tauri::async_runtime::spawn_blocking(move || -> std::io::Result<()> {
        std::fs::create_dir_all(paths.instance_directory())?;
        let copy = ensure_host_copy(&paths, &executable, &fingerprint)?;
        super::launcher::launch_host(&copy, paths.instance_directory())
    })
    .await
    .map_err(|error| HostError::Unavailable(error.to_string()))??;
    Ok(())
}

async fn wait_for_state(state_file: &Path) -> Result<HostState, HostError> {
    let deadline = tokio::time::Instant::now() + HOST_START_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if let Some(state) = read_state(state_file) {
            return Ok(state);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    Err(HostError::Unavailable(
        "the terminal host did not start in time".into(),
    ))
}

/// Remove host copies of builds no host runs any more, off the startup path.
fn prune_in_background(paths: HostPaths, fingerprint: String) {
    tauri::async_runtime::spawn_blocking(move || {
        prune_host_copies(&paths, &[fingerprint.as_str()]);
    });
}

/// Start the connection's writer and reader around a handshaken link.
fn open_connection(app: &AppHandle, link: Link) -> Arc<Connection> {
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

async fn seed_known_sessions(connection: &Connection) -> Result<(), HostError> {
    let Outcome::Sessions { sessions } = connection.call(Request::List { owner: None }).await?
    else {
        return Err(HostError::UnexpectedReply);
    };
    for session in sessions {
        connection.note_session(&session.id, &session.owner);
    }
    Ok(())
}

async fn read_host_messages(app: AppHandle, connection: Arc<Connection>, mut reader: Reader) {
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
fn report_lost_sessions(app: &AppHandle, connection: &Connection) {
    for (id, owner) in connection.close() {
        crate::terminal::pty::emit_exit(app, &owner, &id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_connection_reports_each_known_session_once() {
        let (outbound, _queue) = mpsc::channel(1);
        let connection = Connection {
            outbound,
            pending: Mutex::new(HashMap::new()),
            next_request_id: AtomicU64::new(1),
            is_open: AtomicBool::new(true),
            known_sessions: Mutex::new(HashMap::new()),
        };
        connection.note_session("a", "main");
        connection.note_session("b", "w-1");
        connection.note_session("b", "w-2");
        connection.forget_session("a");
        assert_eq!(
            connection.close(),
            HashMap::from([("b".to_string(), "w-2".to_string())])
        );
        assert!(connection.close().is_empty());
        assert!(!connection.is_open.load(Ordering::Acquire));
    }
}
