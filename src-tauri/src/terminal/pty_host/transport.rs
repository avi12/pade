//! The local channel between the app and the PTY host: a named pipe on Windows,
//! a Unix domain socket elsewhere (compile-level parity; Windows ships). Both
//! sides see a plain byte stream; the protocol and its token sit on top.

use std::io;
use std::path::Path;
use std::time::Duration;

/// How long `connect` keeps retrying a pipe whose every instance is busy - the
/// short gap between the host accepting one client and listening for the next.
const BUSY_RETRY_WINDOW: Duration = Duration::from_secs(2);
const BUSY_RETRY_INTERVAL: Duration = Duration::from_millis(50);

/// A fresh, unguessable endpoint for a host starting in `instance_directory`.
/// Random rather than derived, so another local account can neither squat on
/// it ahead of the host nor find it without reading the user's own state file.
pub(crate) fn new_endpoint(instance_directory: &Path) -> String {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    if cfg!(windows) {
        return format!(r"\\.\pipe\pade-pty-host-{suffix}");
    }
    instance_directory
        .join(format!("host-{suffix}.sock"))
        .to_string_lossy()
        .into_owned()
}

/// Whether an open failed because nothing listens at the endpoint - a host that
/// is gone, as opposed to one that is busy or misbehaving.
pub(crate) fn is_absent(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    )
}

#[cfg(windows)]
mod platform {
    use super::{io, BUSY_RETRY_INTERVAL, BUSY_RETRY_WINDOW};
    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };

    /// Every pipe instance is in use (`ERROR_PIPE_BUSY`).
    const ERROR_PIPE_BUSY: i32 = 231;

    pub(crate) type ServerStream = NamedPipeServer;
    pub(crate) type ClientStream = NamedPipeClient;

    /// The host's listening side. One pipe instance always waits for the next
    /// client, created before the connected one is handed off.
    pub(crate) struct Listener {
        endpoint: String,
        waiting: NamedPipeServer,
    }

    impl Listener {
        /// Create the endpoint. `first_pipe_instance` makes this fail if the
        /// name exists already, so nobody else can be listening on it; remote
        /// clients are refused outright. Must run inside the tokio runtime.
        pub(crate) fn bind(endpoint: &str) -> io::Result<Self> {
            let waiting = ServerOptions::new()
                .first_pipe_instance(true)
                .reject_remote_clients(true)
                .create(endpoint)?;
            Ok(Self {
                endpoint: endpoint.to_string(),
                waiting,
            })
        }

        pub(crate) async fn accept(&mut self) -> io::Result<ServerStream> {
            self.waiting.connect().await?;
            let next = ServerOptions::new()
                .reject_remote_clients(true)
                .create(&self.endpoint)?;
            Ok(std::mem::replace(&mut self.waiting, next))
        }
    }

    pub(crate) async fn connect(endpoint: &str) -> io::Result<ClientStream> {
        let deadline = tokio::time::Instant::now() + BUSY_RETRY_WINDOW;
        loop {
            match ClientOptions::new().open(endpoint) {
                Err(error)
                    if error.raw_os_error() == Some(ERROR_PIPE_BUSY)
                        && tokio::time::Instant::now() < deadline =>
                {
                    tokio::time::sleep(BUSY_RETRY_INTERVAL).await;
                }
                opened => return opened,
            }
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::io;
    use tokio::net::{UnixListener, UnixStream};

    pub(crate) type ServerStream = UnixStream;
    pub(crate) type ClientStream = UnixStream;

    pub(crate) struct Listener(UnixListener);

    impl Listener {
        pub(crate) fn bind(endpoint: &str) -> io::Result<Self> {
            UnixListener::bind(endpoint).map(Self)
        }

        pub(crate) async fn accept(&mut self) -> io::Result<ServerStream> {
            self.0.accept().await.map(|(stream, _)| stream)
        }
    }

    pub(crate) async fn connect(endpoint: &str) -> io::Result<ClientStream> {
        UnixStream::connect(endpoint).await
    }
}

pub(crate) use platform::{connect, ClientStream, Listener, ServerStream};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_endpoint_is_fresh() {
        let directory = std::env::temp_dir();
        assert_ne!(new_endpoint(&directory), new_endpoint(&directory));
    }

    #[test]
    fn a_missing_endpoint_reads_as_absent() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let endpoint = new_endpoint(&std::env::temp_dir());
        let opened = runtime.block_on(connect(&endpoint));
        assert!(opened.as_ref().is_err_and(is_absent), "{opened:?}");
    }
}
