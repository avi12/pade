//! Finding, starting, replacing and retiring the instance's PTY host, and greeting it to get a connection.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;
use tokio::io::BufReader;

use crate::terminal::pty_host::discovery::{
    ensure_host_copy, executable_fingerprint, instance_key, prune_host_copies, read_state,
    HostPaths, HostState,
};
use crate::terminal::pty_host::protocol::{
    read_frame, write_frame, Greeting, Opening, PROTOCOL_VERSION,
};
use crate::terminal::pty_host::transport::{self};
use crate::terminal::session::registry::EXIT_TEARDOWN_TIMEOUT;

use super::connection::{open_connection, seed_known_sessions, Answer, Connection, Link};
use super::HostError;

/// How long the host has to answer an `Opening`.
pub(super) const GREETING_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a retiring host has to end its sessions and answer.
pub(super) const RETIRE_TIMEOUT: Duration =
    EXIT_TEARDOWN_TIMEOUT.saturating_add(Duration::from_secs(5));

/// How long a freshly started host has to publish its state file.
pub(super) const HOST_START_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A host still attached to an app that just died answers `Busy` until it has
/// noticed; how often, and how far apart, to ask again before giving up on it.
pub(super) const BUSY_RETRIES: u32 = 6;

pub(super) const BUSY_RETRY_INTERVAL: Duration = Duration::from_millis(500);

/// Find, or start, this instance's host and connect to it.
pub(super) async fn establish(app: &AppHandle) -> Result<Arc<Connection>, HostError> {
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
pub(super) async fn attach_or_start(
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
pub(super) async fn ask_with_patience(state: &HostState) -> Result<Answer, HostError> {
    for _ in 0..BUSY_RETRIES {
        let answer = ask(state).await?;
        if !matches!(answer, Answer::Busy) {
            return Ok(answer);
        }
        tokio::time::sleep(BUSY_RETRY_INTERVAL).await;
    }
    ask(state).await
}

pub(super) async fn ask(state: &HostState) -> Result<Answer, HostError> {
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
pub(super) async fn open(
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
pub(super) async fn retire(state: &HostState) -> Result<(), HostError> {
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

pub(super) async fn wait_until_gone(state: &HostState) -> Result<(), HostError> {
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
pub(super) fn remove_if_unchanged(state_file: &Path, stale: &HostState) {
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

pub(super) async fn start_host(
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
        crate::terminal::pty_host::launcher::launch_host(&copy, paths.instance_directory())
    })
    .await
    .map_err(|error| HostError::Unavailable(error.to_string()))??;
    Ok(())
}

pub(super) async fn wait_for_state(state_file: &Path) -> Result<HostState, HostError> {
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
pub(super) fn prune_in_background(paths: HostPaths, fingerprint: String) {
    tauri::async_runtime::spawn_blocking(move || {
        prune_host_copies(&paths, &[fingerprint.as_str()]);
    });
}
