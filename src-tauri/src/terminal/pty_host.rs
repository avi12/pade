//! The PTY host: a long-lived process, separate from the app, that owns every
//! agent's pseudo-terminal - so a rebuilt, restarted or crashed app finds its
//! agents still running and re-attaches to them.
//!
//! It is the same executable run in another mode (`--pty-host <instance
//! folder>`), from a build-named copy so the original stays replaceable. One
//! host serves one app instance (an instance is a launch project: two projects
//! launched side by side are two instances, each with its own window labels).
//!
//! - `server` runs in the host: the session engine (`terminal::session`) behind
//!   the transport.
//! - `client` runs in the app: finds or starts the host, and turns its output
//!   into the frontend's `pty://data` / `pty://exit` events.
//! - `protocol` is the wire between them, `transport` the pipe it travels on,
//!   `discovery` the files on disk that connect the two, and `launcher` how a
//!   host is started outside everything that ends the app.

pub(crate) mod client;
mod discovery;
mod launcher;
mod protocol;
mod server;
mod transport;

use std::path::PathBuf;

/// The argument that puts the executable in PTY-host mode, followed by the
/// instance folder the host serves.
pub(crate) const PTY_HOST_FLAG: &str = "--pty-host";

/// Run as the PTY host when the process was started as one, and report whether
/// it was - the caller then returns before anything of the app starts.
pub fn run_if_requested() -> bool {
    let arguments: Vec<String> = std::env::args().collect();
    let Some(instance_directory) = instance_directory_argument(&arguments) else {
        return false;
    };
    server::run(instance_directory);
    true
}

/// The folder a host started with `--pty-host <folder>` serves.
fn instance_directory_argument(arguments: &[String]) -> Option<PathBuf> {
    let flag_position = arguments
        .iter()
        .position(|argument| argument == PTY_HOST_FLAG)?;
    arguments.get(flag_position + 1).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_flag_names_its_instance_folder() {
        let arguments = vec![
            "pade.exe".to_string(),
            PTY_HOST_FLAG.to_string(),
            r"C:\data\pty-host\x".to_string(),
        ];
        assert_eq!(
            instance_directory_argument(&arguments),
            Some(PathBuf::from(r"C:\data\pty-host\x"))
        );
        assert_eq!(instance_directory_argument(&arguments[..1]), None);
        assert_eq!(instance_directory_argument(&arguments[..2]), None);
    }
}

/// The end-to-end smoke test, in two phases run as separate processes with the
/// client killed in between - exactly what a dev rebuild does to the app. Needs
/// a built `pade` binary beside the test binary (`cargo build` first):
///
/// 1. `cargo test … smoke_phase_one -- --ignored`: starts a real host from a
///    copy, spawns `ping` in it, reads its output, then waits - kill that cargo
///    process tree (`taskkill /F /T`) while it waits.
/// 2. `cargo test … smoke_phase_two -- --ignored`: reconnects, finds the
///    session and its history still there, then retires the host.
#[cfg(all(test, windows))]
mod smoke {
    use std::path::PathBuf;
    use std::time::Duration;

    use tokio::io::{BufReader, ReadHalf, WriteHalf};

    use super::discovery::{ensure_host_copy, executable_fingerprint, read_state, HostPaths};
    use super::protocol::{
        read_frame, write_frame, Greeting, HostMessage, Opening, Outcome, Request, RequestFrame,
        PROTOCOL_VERSION,
    };
    use super::transport::{self, ClientStream};
    use crate::terminal::session::launch::SessionLaunch;

    const SESSION: &str = "smoke-session";
    const OWNER: &str = "main";
    /// Long enough to outlive the client being killed between the phases.
    const PING_COUNT: &str = "120";

    struct Link {
        reader: BufReader<ReadHalf<ClientStream>>,
        writer: WriteHalf<ClientStream>,
        buffer: Vec<u8>,
    }

    impl Link {
        async fn request(&mut self, request: Request) -> Outcome {
            let frame = RequestFrame {
                request_id: 1,
                request,
            };
            write_frame(&mut self.writer, &frame).await.expect("sent");
            loop {
                let message: HostMessage = read_frame(&mut self.reader, &mut self.buffer)
                    .await
                    .expect("read")
                    .expect("open");
                if let HostMessage::Reply { outcome, .. } = message {
                    return outcome;
                }
            }
        }
    }

    fn paths() -> HostPaths {
        HostPaths::under(
            &std::env::temp_dir().join("pade-pty-host-smoke"),
            "instance",
        )
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime")
    }

    async fn attach(paths: &HostPaths) -> (Link, usize) {
        let state = loop {
            if let Some(state) = read_state(&paths.state_file()) {
                break state;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        let stream = transport::connect(&state.endpoint)
            .await
            .expect("connected");
        let (read_half, mut writer) = tokio::io::split(stream);
        let hello = Opening::Hello {
            protocol_version: PROTOCOL_VERSION,
            token: state.token,
        };
        write_frame(&mut writer, &hello).await.expect("sent");
        let mut link = Link {
            reader: BufReader::new(read_half),
            writer,
            buffer: Vec::new(),
        };
        let greeting: Greeting = read_frame(&mut link.reader, &mut link.buffer)
            .await
            .expect("read")
            .expect("greeted");
        let Greeting::Welcome { session_count, .. } = greeting else {
            panic!("not welcomed: {greeting:?}");
        };
        (link, session_count)
    }

    #[test]
    #[ignore = "starts a real PTY host process; run by hand, see the module docs"]
    fn smoke_phase_one() {
        let paths = paths();
        std::fs::create_dir_all(paths.instance_directory()).expect("instance directory");
        let _ = std::fs::remove_file(paths.state_file());
        let executable: PathBuf = std::env::current_exe()
            .expect("test binary")
            .parent()
            .and_then(std::path::Path::parent)
            .expect("target directory")
            .join("pade.exe");
        let fingerprint = executable_fingerprint(&executable).expect("a built pade.exe");
        let copy = ensure_host_copy(&paths, &executable, &fingerprint).expect("copied");
        super::launcher::launch_host(&copy, paths.instance_directory()).expect("launched");

        runtime().block_on(async {
            let (mut link, _) = attach(&paths).await;
            let launch = SessionLaunch {
                command: Some("ping".into()),
                columns: 80,
                rows: 24,
                arguments: vec!["-n".into(), PING_COUNT.into(), "127.0.0.1".into()],
                environment: std::env::vars().collect(),
                ..SessionLaunch::default()
            };
            let spawned = link
                .request(Request::Spawn {
                    id: SESSION.into(),
                    owner: OWNER.into(),
                    launch,
                })
                .await;
            assert_eq!(spawned, Outcome::Done);
            let mut said = String::new();
            while !said.contains("127.0.0.1") {
                let message: HostMessage = read_frame(&mut link.reader, &mut link.buffer)
                    .await
                    .expect("read")
                    .expect("open");
                if let HostMessage::Output { data, .. } = message {
                    said.push_str(&data);
                }
            }
            println!("SMOKE phase one: streaming; kill this process tree now");
            tokio::time::sleep(Duration::from_secs(90)).await;
        });
    }

    #[test]
    #[ignore = "needs smoke_phase_one's host; run by hand, see the module docs"]
    fn smoke_phase_two() {
        let paths = paths();
        runtime().block_on(async {
            let (mut link, session_count) = attach(&paths).await;
            assert_eq!(session_count, 1, "the session outlived its client");
            let Outcome::Sessions { sessions } = link.request(Request::List { owner: None }).await
            else {
                panic!("no roster");
            };
            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].id, SESSION);
            assert_eq!(sessions[0].owner, OWNER);
            let Outcome::History { snapshot } = link
                .request(Request::History {
                    owner: OWNER.into(),
                    id: SESSION.into(),
                })
                .await
            else {
                panic!("no history");
            };
            let replay = serde_json::to_value(&snapshot).expect("serializable");
            let data = replay["data"].as_str().unwrap_or_default();
            assert!(data.contains("127.0.0.1"), "history: {data:?}");
            println!("SMOKE phase two: reattached, history {} bytes", data.len());

            let state = read_state(&paths.state_file()).expect("state");
            drop(link);
            let retire = Opening::Retire { token: state.token };
            let greeting = loop {
                let stream = transport::connect(&state.endpoint)
                    .await
                    .expect("connected");
                let (read_half, mut writer) = tokio::io::split(stream);
                write_frame(&mut writer, &retire).await.expect("sent");
                let mut reader = BufReader::new(read_half);
                let mut buffer = Vec::new();
                let greeting: Option<Greeting> =
                    read_frame(&mut reader, &mut buffer).await.expect("read");
                if greeting != Some(Greeting::Busy) {
                    break greeting;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            };
            assert_eq!(greeting, Some(Greeting::Retired));
            println!("SMOKE phase two: host retired");
        });
    }
}
