//! The wire between the app and the PTY host: newline-delimited JSON, one
//! message per line, a tagged enum per direction.
//!
//! Two layers. The **handshake** (`Opening` / `Greeting`) is frozen: it is the
//! one part two different builds must always understand, because it is how a
//! newer app learns that a running host speaks an older protocol and retires
//! it. Everything after a `Welcome` (`RequestFrame` / `HostMessage`) is
//! versioned by [`PROTOCOL_VERSION`] - bump it whenever those change shape.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::terminal::session::launch::SessionLaunch;
use crate::terminal::session::output::HistorySnapshot;
use crate::terminal::session::registry::{HostedSession, SessionError};

/// The version of everything after the handshake.
pub(crate) const PROTOCOL_VERSION: u32 = 1;

/// The longest line either side accepts. A PTY write is capped at 1 MiB by the
/// app and a replay history at 512 KiB by the engine; JSON escaping of control
/// bytes can grow either up to six-fold, so this leaves room for both while
/// still bounding what a malformed peer can make the other buffer.
const MAXIMUM_FRAME_BYTES: usize = 8 * 1024 * 1024;

const FRAME_END: u8 = b'\n';

/// The first message on every connection, from the app. Frozen across versions.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum Opening {
    /// Attach as the host's app.
    Hello {
        protocol_version: u32,
        token: String,
    },
    /// End every session and exit - how an app replaces a host it cannot use
    /// (another protocol version, or an idle host from an older build).
    Retire { token: String },
}

/// The host's answer to an `Opening`. Frozen across versions. A wrong token gets
/// no answer at all: the host drops the connection.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum Greeting {
    /// Attached. `executable_fingerprint` names the build the host runs and
    /// `session_count` how many sessions it holds, so an app from a newer build
    /// can replace an idle host instead of running on old code.
    Welcome {
        protocol_version: u32,
        executable_fingerprint: String,
        session_count: usize,
    },
    /// The host speaks another protocol; the connection ends.
    VersionMismatch { protocol_version: u32 },
    /// Another app is attached already.
    Busy,
    /// The host ended its sessions and is exiting.
    Retired,
}

/// One request from the attached app; the reply echoes `request_id`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestFrame {
    pub(crate) request_id: u64,
    pub(crate) request: Request,
}

/// What the app asks of the session engine - one variant per registry operation.
/// `owner` is the calling window's label, which the engine's owner check reads.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum Request {
    Spawn {
        id: String,
        owner: String,
        launch: SessionLaunch,
    },
    Write {
        owner: String,
        id: String,
        data: String,
    },
    Resize {
        owner: String,
        id: String,
        columns: u16,
        rows: u16,
    },
    Kill {
        owner: String,
        id: String,
    },
    /// One window's sessions, or every session for `None`.
    List {
        owner: Option<String>,
    },
    History {
        owner: String,
        id: String,
    },
    Transcript {
        owner: String,
        id: String,
    },
    RootProcessId {
        owner: String,
        id: String,
    },
    Transfer {
        from: String,
        to: String,
        ids: Vec<String>,
    },
    KillOwned {
        owner: String,
    },
    KillAll,
}

impl Request {
    /// Whether answering blocks for long (a process starting or a process tree
    /// being killed). Those run concurrently; every other request is answered
    /// in arrival order, which is what keeps a session's keystrokes in order.
    pub(crate) fn is_long_running(&self) -> bool {
        matches!(self, Self::Spawn { .. } | Self::Kill { .. } | Self::KillAll)
    }
}

/// Everything the host sends the attached app after the handshake.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum HostMessage {
    Reply {
        request_id: u64,
        outcome: Outcome,
    },
    /// A session's output chunk, for its owner window (`pty://data`).
    Output {
        id: String,
        owner: String,
        data: String,
        sequence: u64,
    },
    /// A session's process exited (`pty://exit`).
    Exited {
        id: String,
        owner: String,
    },
}

/// The result of one request.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum Outcome {
    Done,
    Sessions { sessions: Vec<HostedSession> },
    History { snapshot: HistorySnapshot },
    Transcript { text: String },
    RootProcessId { process_id: Option<u32> },
    Failed { error: SessionError },
}

/// Why a frame could not be read or written.
#[derive(Debug)]
pub(crate) enum FrameError {
    Io(std::io::Error),
    /// A line longer than `MAXIMUM_FRAME_BYTES`, or one cut off by the peer
    /// closing mid-line.
    Oversized,
    Malformed(serde_json::Error),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "pipe error: {error}"),
            Self::Oversized => formatter.write_str("frame too large or cut off"),
            Self::Malformed(error) => write!(formatter, "malformed frame: {error}"),
        }
    }
}

impl std::error::Error for FrameError {}

impl From<std::io::Error> for FrameError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for FrameError {
    fn from(error: serde_json::Error) -> Self {
        Self::Malformed(error)
    }
}

/// Write one message as one line.
pub(crate) async fn write_frame<W, T>(writer: &mut W, message: &T) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let mut line = serde_json::to_vec(message)?;
    line.push(FRAME_END);
    writer.write_all(&line).await?;
    writer.flush().await?;
    Ok(())
}

/// Read one message, reusing `buffer`. `None` when the peer closed cleanly
/// between messages.
pub(crate) async fn read_frame<R, T>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
) -> Result<Option<T>, FrameError>
where
    R: AsyncBufRead + Unpin,
    T: DeserializeOwned,
{
    buffer.clear();
    let limit = u64::try_from(MAXIMUM_FRAME_BYTES).unwrap_or(u64::MAX);
    let read = (&mut *reader)
        .take(limit)
        .read_until(FRAME_END, buffer)
        .await?;
    if read == 0 {
        return Ok(None);
    }
    if buffer.last() != Some(&FRAME_END) {
        return Err(FrameError::Oversized);
    }
    Ok(Some(serde_json::from_slice(buffer)?))
}

/// Compare a presented token with the host's without stopping at the first
/// differing byte, so the comparison's timing says nothing about the token.
pub(crate) fn is_same_token(presented: &str, expected: &str) -> bool {
    let presented = presented.as_bytes();
    let expected = expected.as_bytes();
    let mut difference = u8::from(presented.len() != expected.len());
    for (left, right) in presented.iter().zip(expected) {
        difference |= left ^ right;
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T>(message: &T) -> T
    where
        T: Serialize + DeserializeOwned,
    {
        serde_json::from_str(&serde_json::to_string(message).expect("serializable"))
            .expect("parsable")
    }

    #[test]
    fn the_handshake_keeps_its_frozen_wire_shape() {
        let hello = Opening::Hello {
            protocol_version: 1,
            token: "t".into(),
        };
        assert_eq!(
            serde_json::to_value(&hello).expect("serializable"),
            serde_json::json!({ "type": "hello", "protocolVersion": 1, "token": "t" })
        );
        assert_eq!(
            serde_json::to_value(Greeting::VersionMismatch {
                protocol_version: 9
            })
            .expect("serializable"),
            serde_json::json!({ "type": "versionMismatch", "protocolVersion": 9 })
        );
        assert_eq!(round_trip(&hello), hello);
        assert_eq!(round_trip(&Greeting::Busy), Greeting::Busy);
    }

    #[test]
    fn every_request_round_trips() {
        let requests = [
            Request::Spawn {
                id: "s".into(),
                owner: "main".into(),
                launch: SessionLaunch::default(),
            },
            Request::Write {
                owner: "main".into(),
                id: "s".into(),
                data: "\u{1b}[A\r".into(),
            },
            Request::Resize {
                owner: "main".into(),
                id: "s".into(),
                columns: 80,
                rows: 24,
            },
            Request::List { owner: None },
            Request::Transfer {
                from: "main".into(),
                to: "w-2".into(),
                ids: vec!["s".into()],
            },
            Request::KillAll,
        ];
        for request in requests {
            let frame = RequestFrame {
                request_id: 7,
                request,
            };
            assert_eq!(round_trip(&frame), frame);
        }
    }

    #[test]
    fn host_messages_round_trip() {
        let messages = [
            HostMessage::Output {
                id: "s".into(),
                owner: "main".into(),
                data: "hi\r\n".into(),
                sequence: 3,
            },
            HostMessage::Exited {
                id: "s".into(),
                owner: "w-1".into(),
            },
            HostMessage::Reply {
                request_id: 1,
                outcome: Outcome::Failed {
                    error: SessionError::Foreign,
                },
            },
            HostMessage::Reply {
                request_id: 2,
                outcome: Outcome::RootProcessId {
                    process_id: Some(42),
                },
            },
        ];
        for message in messages {
            assert_eq!(round_trip(&message), message);
        }
    }

    #[test]
    fn only_slow_requests_leave_the_ordered_queue() {
        assert!(Request::KillAll.is_long_running());
        assert!(!Request::Write {
            owner: "main".into(),
            id: "s".into(),
            data: String::new(),
        }
        .is_long_running());
    }

    #[test]
    fn tokens_compare_whole() {
        assert!(is_same_token("secret", "secret"));
        assert!(!is_same_token("secreT", "secret"));
        assert!(!is_same_token("secret-longer", "secret"));
        assert!(!is_same_token("", "secret"));
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime")
    }

    #[test]
    fn frames_read_back_in_order_then_end_cleanly() {
        runtime().block_on(async {
            let mut wire = Vec::new();
            write_frame(&mut wire, &Greeting::Busy)
                .await
                .expect("written");
            write_frame(&mut wire, &Greeting::Retired)
                .await
                .expect("written");
            let mut reader = tokio::io::BufReader::new(wire.as_slice());
            let mut buffer = Vec::new();
            let first: Option<Greeting> = read_frame(&mut reader, &mut buffer).await.expect("read");
            let second: Option<Greeting> =
                read_frame(&mut reader, &mut buffer).await.expect("read");
            let end: Option<Greeting> = read_frame(&mut reader, &mut buffer).await.expect("read");
            assert_eq!(first, Some(Greeting::Busy));
            assert_eq!(second, Some(Greeting::Retired));
            assert_eq!(end, None);
        });
    }

    #[test]
    fn a_line_cut_off_mid_frame_is_refused() {
        runtime().block_on(async {
            let mut reader = tokio::io::BufReader::new(&b"{\"type\":\"busy\""[..]);
            let mut buffer = Vec::new();
            let read: Result<Option<Greeting>, _> = read_frame(&mut reader, &mut buffer).await;
            assert!(matches!(read, Err(FrameError::Oversized)));
        });
    }
}
