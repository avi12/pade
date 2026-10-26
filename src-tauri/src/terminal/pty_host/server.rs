//! The PTY host process: owns the session engine and serves one attached app
//! over the local transport. It outlives the app - a rebuild or a crash of
//! `pade.exe` leaves every agent running here - and ends itself only when it
//! has been idle (no sessions, no app) for a while, or when an app retires it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio::sync::{mpsc, Notify};

use crate::terminal::session::output::SessionEvents;
use crate::terminal::session::registry::SessionRegistry;

use super::discovery::{
    executable_fingerprint, fingerprint_of_copy, read_state, remove_state_if_owned, write_state,
    HostPaths, HostState,
};
use super::protocol::{
    is_same_token, read_frame, write_frame, Greeting, HostMessage, Opening, Outcome, Request,
    RequestFrame, PROTOCOL_VERSION,
};
use super::transport::{self, Listener, ServerStream};

/// How long a fresh connection has to say who it is.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the host lingers with no sessions and no app before exiting - long
/// enough to bridge an app restart, short enough not to linger after a quit.
const IDLE_EXIT_AFTER: Duration = Duration::from_secs(60);
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(5);
/// Messages queued for the app before a session's output pump waits. Bounded,
/// so an app that stops reading slows the agents down instead of growing the
/// host without limit.
const OUTBOUND_CAPACITY: usize = 1024;
/// Requests queued for the in-order worker before the reader waits.
const ORDERED_CAPACITY: usize = 256;
/// How long the runtime may take to wind down its tasks on exit.
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// Run the host for the instance folder `instance_directory` until it retires
/// or idles out. Called from `run()` before anything of the app starts.
pub(crate) fn run(instance_directory: PathBuf) {
    let paths = HostPaths::from_instance_directory(instance_directory);
    if let Err(error) = std::fs::create_dir_all(paths.instance_directory()) {
        eprintln!(
            "[pty-host] cannot create {}: {error}",
            paths.instance_directory().display()
        );
        return;
    }
    let log = Arc::new(HostLog::start(paths.log_file()));
    // Never hold a project folder as the working directory: that alone locks it
    // against being moved or deleted on Windows.
    if let Err(error) = std::env::set_current_dir(paths.instance_directory()) {
        log.write(&format!("could not leave the launch directory: {error}"));
    }

    #[cfg(windows)]
    let job = process_job::KillOnCloseJob::adopt_current_process()
        .inspect_err(|error| {
            log.write(&format!(
                "no kill-on-close job ({error}); agents would outlive a crash"
            ));
        })
        .ok();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build();
    match runtime {
        Ok(runtime) => {
            runtime.block_on(serve(&paths, &log));
            runtime.shutdown_timeout(RUNTIME_SHUTDOWN_TIMEOUT);
        }
        Err(error) => log.write(&format!("no async runtime: {error}")),
    }

    // A deliberate exit: the sessions are already ended, and whatever an agent
    // launched to outlive it must not be killed with the job on the way out.
    #[cfg(windows)]
    if let Some(job) = &job {
        if let Err(error) = job.release() {
            log.write(&format!("could not release the job: {error}"));
        }
    }
    remove_state_if_owned(&paths.state_file(), std::process::id());
    log.write("exited");
}

/// A host's own diagnostics, in `host.log` beside its state file: a host has no
/// console anyone sees. Started empty on every host start, and written only on
/// connection-level events, so it stays small.
struct HostLog {
    path: PathBuf,
}

impl HostLog {
    fn start(path: PathBuf) -> Self {
        if let Err(error) = std::fs::write(&path, "") {
            eprintln!("[pty-host] cannot start {}: {error}", path.display());
        }
        Self { path }
    }

    fn write(&self, message: &str) {
        use std::io::Write;
        let line = format!("{} {message}\n", crate::util::now_milliseconds());
        let appended = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .and_then(|mut file| file.write_all(line.as_bytes()));
        if let Err(error) = appended {
            eprintln!("[pty-host] {message} (log unwritable: {error})");
        }
    }
}

/// Forwards session output to the attached app, if there is one. With none,
/// output is not lost: the session's history keeps it for the next attach.
#[derive(Default)]
struct HostEvents {
    client: Mutex<Option<mpsc::Sender<HostMessage>>>,
}

impl HostEvents {
    fn attach(&self, sender: mpsc::Sender<HostMessage>) {
        if let Ok(mut client) = self.client.lock() {
            *client = Some(sender);
        }
    }

    fn detach(&self) {
        if let Ok(mut client) = self.client.lock() {
            *client = None;
        }
    }

    /// Hand `message` to the attached app, waiting while its queue is full.
    /// Called only from session threads, never from an async task.
    fn send(&self, message: HostMessage) {
        let sender = self.client.lock().ok().and_then(|client| client.clone());
        let Some(sender) = sender else {
            return;
        };
        // A closed queue means the app just detached; the history has the chunk.
        let _ = sender.blocking_send(message);
    }
}

impl SessionEvents for HostEvents {
    fn output(&self, owner: &str, id: &str, data: &str, sequence: u64) {
        self.send(HostMessage::Output {
            id: id.to_string(),
            owner: owner.to_string(),
            data: data.to_string(),
            sequence,
        });
    }

    fn exited(&self, owner: &str, id: &str) {
        self.send(HostMessage::Exited {
            id: id.to_string(),
            owner: owner.to_string(),
        });
    }
}

/// Everything one running host shares between its connections.
struct Host {
    registry: Arc<SessionRegistry>,
    events: Arc<HostEvents>,
    token: String,
    executable_fingerprint: String,
    /// The connection currently attached as the app, if any. One at a time: two
    /// apps sharing a host would share window labels, and so each other's agents.
    attached: Mutex<Option<u64>>,
    shutdown: Notify,
    log: Arc<HostLog>,
}

impl Host {
    fn try_attach(&self, connection: u64) -> bool {
        let Ok(mut attached) = self.attached.lock() else {
            return false;
        };
        if attached.is_some() {
            return false;
        }
        *attached = Some(connection);
        true
    }

    fn detach(&self, connection: u64) {
        if let Ok(mut attached) = self.attached.lock() {
            if *attached == Some(connection) {
                *attached = None;
            }
        }
    }

    fn is_attached(&self) -> bool {
        self.attached
            .lock()
            .map_or(true, |attached| attached.is_some())
    }
}

async fn serve(paths: &HostPaths, log: &Arc<HostLog>) {
    let state_file = paths.state_file();
    if let Some(existing) = read_state(&state_file) {
        let is_other_host_alive = transport::connect(&existing.endpoint).await.is_ok();
        if is_other_host_alive {
            log.write("another host already serves this instance; exiting");
            return;
        }
    }

    let endpoint = transport::new_endpoint(paths.instance_directory());
    let listener = match Listener::bind(&endpoint) {
        Ok(listener) => listener,
        Err(error) => {
            log.write(&format!("cannot listen on {endpoint}: {error}"));
            return;
        }
    };
    let events = Arc::new(HostEvents::default());
    let host = Arc::new(Host {
        registry: SessionRegistry::new(Arc::clone(&events) as Arc<dyn SessionEvents>),
        events,
        token: uuid::Uuid::new_v4().simple().to_string(),
        executable_fingerprint: own_fingerprint(),
        attached: Mutex::new(None),
        shutdown: Notify::new(),
        log: Arc::clone(log),
    });
    let state = HostState {
        protocol_version: PROTOCOL_VERSION,
        endpoint,
        token: host.token.clone(),
        process_id: std::process::id(),
        executable_fingerprint: host.executable_fingerprint.clone(),
    };
    if let Err(error) = write_state(&state_file, &state) {
        log.write(&format!("cannot publish {}: {error}", state_file.display()));
        return;
    }
    log.write(&format!("listening, protocol {PROTOCOL_VERSION}"));

    let accepting = tokio::spawn(accept_connections(listener, Arc::clone(&host)));
    let watching = tokio::spawn(exit_when_idle(Arc::clone(&host)));
    host.shutdown.notified().await;
    accepting.abort();
    watching.abort();
}

/// The build this host runs: named by its copy's file name, or measured when
/// the host runs from an executable that is not a copy (a test).
fn own_fingerprint() -> String {
    let Ok(executable) = std::env::current_exe() else {
        return String::new();
    };
    fingerprint_of_copy(&executable)
        .or_else(|| executable_fingerprint(&executable).ok())
        .unwrap_or_default()
}

async fn accept_connections(mut listener: Listener, host: Arc<Host>) {
    let next_connection = AtomicU64::new(1);
    loop {
        match listener.accept().await {
            Ok(stream) => {
                let connection = next_connection.fetch_add(1, Ordering::Relaxed);
                tokio::spawn(serve_connection(Arc::clone(&host), stream, connection));
            }
            Err(error) => {
                host.log.write(&format!("accept failed: {error}"));
                host.shutdown.notify_one();
                return;
            }
        }
    }
}

async fn exit_when_idle(host: Arc<Host>) {
    let mut quiet_since: Option<Instant> = None;
    loop {
        tokio::time::sleep(IDLE_CHECK_INTERVAL).await;
        let is_idle = !host.is_attached() && host.registry.count() == 0;
        if !is_idle {
            quiet_since = None;
            continue;
        }
        let since = *quiet_since.get_or_insert_with(Instant::now);
        if since.elapsed() >= IDLE_EXIT_AFTER {
            host.log.write("idle with no app attached; exiting");
            host.shutdown.notify_one();
            return;
        }
    }
}

async fn serve_connection(host: Arc<Host>, stream: ServerStream, connection: u64) {
    let (read_half, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    let mut buffer = Vec::new();
    let opening = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        read_frame::<_, Opening>(&mut reader, &mut buffer),
    )
    .await;
    let Ok(Ok(Some(opening))) = opening else {
        return;
    };

    match opening {
        Opening::Hello {
            protocol_version,
            token,
        } => {
            if !is_same_token(&token, &host.token) {
                host.log.write("refused a connection with a wrong token");
                return;
            }
            if protocol_version != PROTOCOL_VERSION {
                let mismatch = Greeting::VersionMismatch {
                    protocol_version: PROTOCOL_VERSION,
                };
                // The peer is leaving either way; nothing to do if it already left.
                let _ = write_frame(&mut writer, &mismatch).await;
                return;
            }
            if !host.try_attach(connection) {
                let _ = write_frame(&mut writer, &Greeting::Busy).await;
                return;
            }
            let welcome = Greeting::Welcome {
                protocol_version: PROTOCOL_VERSION,
                executable_fingerprint: host.executable_fingerprint.clone(),
                session_count: host.registry.count(),
            };
            host.log.write("app attached");
            if write_frame(&mut writer, &welcome).await.is_ok() {
                serve_attached(&host, reader, writer).await;
            }
            host.detach(connection);
            host.log.write("app detached");
        }
        Opening::Retire { token } => {
            if !is_same_token(&token, &host.token) {
                host.log.write("refused a retire with a wrong token");
                return;
            }
            // Stays attached from here on, so no app attaches to a host on its way out.
            if !host.try_attach(connection) {
                let _ = write_frame(&mut writer, &Greeting::Busy).await;
                return;
            }
            host.log.write("retiring");
            let registry = Arc::clone(&host.registry);
            if let Err(error) = tokio::task::spawn_blocking(move || registry.kill_all()).await {
                host.log
                    .write(&format!("ending the sessions failed: {error}"));
            }
            let _ = write_frame(&mut writer, &Greeting::Retired).await;
            host.shutdown.notify_one();
        }
    }
}

/// Serve the attached app until it goes away: requests in, replies and session
/// output out. Slow requests run concurrently; every other request is answered
/// in arrival order by one worker, which keeps each session's keystrokes in order.
async fn serve_attached<R, W>(host: &Arc<Host>, mut reader: BufReader<R>, mut writer: W)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (outbound, mut outbound_queue) = mpsc::channel::<HostMessage>(OUTBOUND_CAPACITY);
    host.events.attach(outbound.clone());
    let writing = tokio::spawn(async move {
        while let Some(message) = outbound_queue.recv().await {
            if write_frame(&mut writer, &message).await.is_err() {
                break;
            }
        }
    });

    let (ordered, ordered_queue) = mpsc::channel::<RequestFrame>(ORDERED_CAPACITY);
    let in_order = {
        let registry = Arc::clone(&host.registry);
        let outbound = outbound.clone();
        tokio::task::spawn_blocking(move || answer_in_order(&registry, ordered_queue, &outbound))
    };

    let mut buffer = Vec::new();
    loop {
        let frame = match read_frame::<_, RequestFrame>(&mut reader, &mut buffer).await {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(error) => {
                host.log.write(&format!("dropping the app: {error}"));
                break;
            }
        };
        if frame.request.is_long_running() {
            let registry = Arc::clone(&host.registry);
            let outbound = outbound.clone();
            tokio::task::spawn_blocking(move || {
                // A reply nobody can receive any more: the app is gone.
                let _ = outbound.blocking_send(answer(&registry, frame));
            });
        } else if ordered.send(frame).await.is_err() {
            break;
        }
    }

    host.events.detach();
    drop(ordered);
    drop(outbound);
    if let Err(error) = in_order.await {
        host.log
            .write(&format!("the request worker failed: {error}"));
    }
    writing.abort();
}

fn answer_in_order(
    registry: &Arc<SessionRegistry>,
    mut queue: mpsc::Receiver<RequestFrame>,
    outbound: &mpsc::Sender<HostMessage>,
) {
    while let Some(frame) = queue.blocking_recv() {
        if outbound.blocking_send(answer(registry, frame)).is_err() {
            return;
        }
    }
}

fn answer(registry: &Arc<SessionRegistry>, frame: RequestFrame) -> HostMessage {
    HostMessage::Reply {
        request_id: frame.request_id,
        outcome: outcome_of(registry, frame.request),
    }
}

/// Run one request against the engine. Blocking for the slow ones (see
/// `Request::is_long_running`).
fn outcome_of(registry: &Arc<SessionRegistry>, request: Request) -> Outcome {
    let done = |()| Outcome::Done;
    let result = match request {
        Request::Spawn { id, owner, launch } => registry.spawn(&id, &owner, &launch).map(done),
        Request::Write { owner, id, data } => registry.write(&owner, &id, &data).map(done),
        Request::Resize {
            owner,
            id,
            columns,
            rows,
        } => registry.resize(&owner, &id, columns, rows).map(done),
        Request::Kill { owner, id } => registry.kill(&owner, &id).map(done),
        Request::List { owner } => registry
            .list(owner.as_deref())
            .map(|sessions| Outcome::Sessions { sessions }),
        Request::History { owner, id } => registry
            .history(&owner, &id)
            .map(|snapshot| Outcome::History { snapshot }),
        Request::Transcript { owner, id } => registry
            .transcript(&owner, &id)
            .map(|text| Outcome::Transcript { text }),
        Request::RootProcessId { owner, id } => registry
            .root_process_id(&owner, &id)
            .map(|process_id| Outcome::RootProcessId { process_id }),
        Request::Transfer { from, to, ids } => registry.transfer(&from, &to, &ids).map(done),
        Request::KillOwned { owner } => {
            registry.kill_owned(&owner);
            Ok(Outcome::Done)
        }
        Request::KillAll => {
            registry.kill_all();
            Ok(Outcome::Done)
        }
    };
    result.unwrap_or_else(|error| Outcome::Failed { error })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::session::registry::SessionError;

    type Link = (
        BufReader<tokio::io::ReadHalf<transport::ClientStream>>,
        tokio::io::WriteHalf<transport::ClientStream>,
    );

    /// Connect, send `opening`, and read the greeting (`None`: dropped unanswered).
    async fn open(endpoint: &str, opening: &Opening) -> (Option<Greeting>, Link) {
        let stream = transport::connect(endpoint).await.expect("connected");
        let (read_half, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(read_half);
        write_frame(&mut writer, opening).await.expect("sent");
        let mut buffer = Vec::new();
        let greeting = read_frame(&mut reader, &mut buffer)
            .await
            .unwrap_or_default();
        (greeting, (reader, writer))
    }

    /// The whole handshake against a real host on a real pipe, in-process: a
    /// wrong token gets no answer, the right one attaches, a second app is
    /// turned away while the first is attached, and a retire ends the host.
    #[test]
    fn the_handshake_guards_the_host_and_a_retire_ends_it() {
        let root =
            std::env::temp_dir().join(format!("pade-pty-host-handshake-{}", std::process::id()));
        let paths = HostPaths::under(&root, "instance");
        std::fs::create_dir_all(paths.instance_directory()).expect("instance directory");
        let log = Arc::new(HostLog::start(paths.log_file()));
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let serving = {
                let paths = paths.clone();
                tokio::spawn(async move { serve(&paths, &log).await })
            };
            let state = loop {
                if let Some(state) = read_state(&paths.state_file()) {
                    break state;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            };
            let hello = |token: &str| Opening::Hello {
                protocol_version: PROTOCOL_VERSION,
                token: token.to_string(),
            };

            let (refused, _) = open(&state.endpoint, &hello("wrong")).await;
            assert_eq!(refused, None, "a wrong token gets no answer");

            let (welcome, attached) = open(&state.endpoint, &hello(&state.token)).await;
            assert!(
                matches!(
                    welcome,
                    Some(Greeting::Welcome {
                        session_count: 0,
                        ..
                    })
                ),
                "{welcome:?}"
            );
            let (busy, _) = open(&state.endpoint, &hello(&state.token)).await;
            assert_eq!(busy, Some(Greeting::Busy));

            let (mismatch, _) = open(
                &state.endpoint,
                &Opening::Hello {
                    protocol_version: PROTOCOL_VERSION + 1,
                    token: state.token.clone(),
                },
            )
            .await;
            assert_eq!(
                mismatch,
                Some(Greeting::VersionMismatch {
                    protocol_version: PROTOCOL_VERSION
                })
            );

            drop(attached);
            let retire = Opening::Retire {
                token: state.token.clone(),
            };
            let retired = loop {
                let (greeting, _) = open(&state.endpoint, &retire).await;
                if greeting != Some(Greeting::Busy) {
                    break greeting;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            };
            assert_eq!(retired, Some(Greeting::Retired));
            tokio::time::timeout(Duration::from_secs(10), serving)
                .await
                .expect("the host stops serving")
                .expect("serve task");
        });
        let _ = std::fs::remove_dir_all(&root);
    }

    fn idle_registry() -> Arc<SessionRegistry> {
        SessionRegistry::new(Arc::new(HostEvents::default()))
    }

    #[test]
    fn requests_against_unknown_sessions_answer_like_the_app_always_did() {
        let registry = idle_registry();
        assert_eq!(
            outcome_of(
                &registry,
                Request::Transcript {
                    owner: "main".into(),
                    id: "nobody".into()
                }
            ),
            Outcome::Transcript {
                text: String::new()
            }
        );
        assert_eq!(
            outcome_of(&registry, Request::List { owner: None }),
            Outcome::Sessions {
                sessions: Vec::new()
            }
        );
        assert_eq!(
            outcome_of(
                &registry,
                Request::Transfer {
                    from: "main".into(),
                    to: "w-1".into(),
                    ids: vec!["gone".into()]
                }
            ),
            Outcome::Failed {
                error: SessionError::Missing { id: "gone".into() }
            }
        );
    }

    #[test]
    fn one_app_attaches_at_a_time() {
        let host = Host {
            registry: idle_registry(),
            events: Arc::new(HostEvents::default()),
            token: "t".into(),
            executable_fingerprint: String::new(),
            attached: Mutex::new(None),
            shutdown: Notify::new(),
            log: Arc::new(HostLog {
                path: std::env::temp_dir().join("pade-pty-host-test.log"),
            }),
        };
        assert!(host.try_attach(1));
        assert!(!host.try_attach(2), "a second app is turned away");
        host.detach(2);
        assert!(host.is_attached(), "only the attached connection detaches");
        host.detach(1);
        assert!(host.try_attach(2));
    }
}
