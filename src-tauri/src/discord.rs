//! Discord Rich Presence — report PADE on the user's Discord profile.
//!
//! Talks the Discord desktop client's local IPC protocol directly over its
//! per-user socket (a named pipe on Windows, a Unix domain socket elsewhere), so
//! no third-party crate is pulled in for what is a small, well-understood wire
//! format (minimize-dependencies). The whole feature is best-effort: when Discord
//! isn't running the socket simply isn't there, every op fails fast, and the UI
//! never sees more than an ignored error. `lib.rs` manages one [`DiscordState`]
//! and wires the two commands; nothing else touches the socket.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tauri::State;

/// PADE's **public** Discord application (client) id — the VS-Code-extension model:
/// one app registered once by the maintainer, its id shipped in the binary, so
/// every user gets "Playing PADE" with zero per-user setup (just the toggle). The
/// app owns the art assets every client references: the `pade` large image plus one
/// small image per language (see [`ART_ASSET_KEY`] and the frontend's language map).
/// To register: create the app at <https://discord.com/developers/applications>,
/// name it PADE, upload the assets, and paste its Application ID here. The
/// placeholder below is inert — Discord silently shows no presence for an
/// unregistered id (best-effort, never an error).
const APPLICATION_ID: &str = "1394000000000000000";

/// The always-present large art-asset key uploaded to the application above (the
/// PADE logo). The small overlay is a per-language key the frontend supplies; an
/// asset the app hasn't uploaded simply doesn't render, text still works.
const ART_ASSET_KEY: &str = "pade";

/// The one presence command PADE sends — set (or, with a null activity, clear)
/// the current activity. Named once so the two payload builders agree (DRY).
const SET_ACTIVITY: &str = "SET_ACTIVITY";

/// Every IPC frame is an 8-byte header (opcode + length, both little-endian u32)
/// followed by the JSON payload.
const FRAME_HEADER_LEN: usize = 8;

/// Reject a response frame claiming more than this many payload bytes — Discord's
/// frames are small, so an absurd length means a desynced/garbage pipe, not a
/// real message worth allocating for.
const MAX_FRAME_PAYLOAD: usize = 64 * 1024;

/// IPC frame opcodes. Only the two PADE sends are modelled; the protocol's `Close`
/// (2) is never written (dropping the socket is enough), so it isn't represented.
#[derive(Debug, Clone, Copy)]
enum Opcode {
    Handshake,
    Frame,
}

impl Opcode {
    /// The on-the-wire numeric code — kept as the enum's one authoritative mapping
    /// so no bare `0`/`1` leaks into the framing.
    const fn code(self) -> u32 {
        match self {
            Opcode::Handshake => 0,
            Opcode::Frame => 1,
        }
    }
}

/// A duplex byte stream to the Discord IPC socket. Unifying the Windows named pipe
/// (a `std::fs::File`) and the Unix domain socket behind one trait keeps the
/// framing/handshake logic platform-agnostic; only [`connect`] is `cfg`-gated.
trait DuplexStream: Read + Write + Send {}
impl<T: Read + Write + Send> DuplexStream for T {}

/// What can go wrong talking to Discord. A real error type (not a bare `String`)
/// so `?` composes and callers could branch on it; the command boundary maps it
/// to a message the frontend ignores.
#[derive(Debug)]
enum DiscordError {
    /// No Discord IPC socket answered — the client isn't running.
    Unavailable,
    /// A frame's length is implausibly large (a corrupt/desynced pipe).
    OversizedFrame,
    /// Underlying socket I/O failed — treated as "Discord went away".
    Io(std::io::Error),
}

impl std::fmt::Display for DiscordError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscordError::Unavailable => formatter.write_str("Discord is not running"),
            DiscordError::OversizedFrame => formatter.write_str("Discord IPC frame too large"),
            DiscordError::Io(error) => write!(formatter, "Discord IPC error: {error}"),
        }
    }
}

impl std::error::Error for DiscordError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DiscordError::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for DiscordError {
    fn from(error: std::io::Error) -> Self {
        DiscordError::Io(error)
    }
}

/// Tauri-managed presence handle. Discord's socket I/O is **blocking**, and a
/// synchronous Tauri command runs on the main (UI) thread — so talking to the socket
/// inline would freeze the window whenever a read stalls (Discord slow, restarting,
/// or a half-open pipe). Instead the state owns a dedicated worker thread that holds
/// the connection and applies presence updates serially off the UI thread; a command
/// only enqueues a message and returns at once. Started lazily on first use.
#[derive(Default)]
pub struct DiscordState {
    sender: Mutex<Option<Sender<Command>>>,
}

/// A presence update handed to the worker thread. Owns its strings, since it
/// outlives the command call that produced it.
enum Command {
    Set(OwnedActivity),
    Clear,
}

/// Owned counterpart of [`ActivityInput`] — the worker borrows from it per send.
#[derive(Default)]
struct OwnedActivity {
    details: Option<String>,
    state: Option<String>,
    image: Option<String>,
    caption: Option<String>,
}

impl DiscordState {
    /// Enqueue a presence update, starting the worker thread on first use. Never
    /// blocks on the socket. If the worker thread has died (it panicked), it is
    /// replaced and the update retried once.
    fn dispatch(&self, mut command: Command) -> Result<(), String> {
        let mut guard = self
            .sender
            .lock()
            .map_err(|_| "discord presence state poisoned".to_owned())?;
        if let Some(sender) = guard.as_ref() {
            match sender.send(command) {
                Ok(()) => return Ok(()),
                Err(returned) => command = returned.0,
            }
        }
        let (sender, receiver) = mpsc::channel::<Command>();
        thread::spawn(move || run_worker(receiver));
        let result = sender
            .send(command)
            .map_err(|_| "discord presence worker unavailable".to_owned());
        *guard = Some(sender);
        result
    }
}

/// The worker loop: owns the connection and applies each queued update in order,
/// off the UI thread, until every sender is dropped (app exit). Each update's result
/// is intentionally ignored — presence is best-effort.
fn run_worker(receiver: Receiver<Command>) {
    let mut presence = Presence::default();
    while let Ok(command) = receiver.recv() {
        let _ = match command {
            Command::Set(activity) => presence.set_activity(ActivityInput {
                details: activity.details.as_deref(),
                state: activity.state.as_deref(),
                image: activity.image.as_deref(),
                caption: activity.caption.as_deref(),
            }),
            Command::Clear => presence.clear_activity(),
        };
    }
}

#[derive(Default)]
struct Presence {
    connection: Option<Box<dyn DuplexStream>>,
    /// Unix-seconds start time sent as `timestamps.start`; set when activity first
    /// goes live, cleared on clear (or a dropped connection).
    started_at: Option<u64>,
}

impl Presence {
    /// Publish the given activity, connecting + handshaking on first use. `details`
    /// and `state` are the two optional status lines; `image`/`caption` are an
    /// optional small overlay icon (an art-asset key) and its hover text — the
    /// VS-Code-style language mark. All are omitted from the payload when `None`.
    fn set_activity(&mut self, fields: ActivityInput) -> Result<(), DiscordError> {
        self.ensure_connected()?;
        let started_at = *self.started_at.get_or_insert_with(now_unix_seconds);
        let nonce = next_nonce();
        let ActivityInput {
            details,
            state,
            image,
            caption,
        } = fields;
        let payload = activity_payload(ActivityFields {
            nonce: &nonce,
            details,
            state,
            image,
            caption,
            started_at,
        });
        self.exchange(&payload)
    }

    /// Clear the activity (keeping the socket for a later re-enable) and forget the
    /// start time so the next enable starts the timer fresh. A no-op when never
    /// connected — there's nothing on Discord to clear.
    fn clear_activity(&mut self) -> Result<(), DiscordError> {
        self.started_at = None;
        if self.connection.is_none() {
            return Ok(());
        }
        let nonce = next_nonce();
        let payload = clear_payload(&nonce);
        self.exchange(&payload)
    }

    /// Guarantee a handshaken connection, reusing the stored one. A failed connect
    /// (Discord not running) surfaces as [`DiscordError::Unavailable`].
    fn ensure_connected(&mut self) -> Result<(), DiscordError> {
        if self.connection.is_some() {
            return Ok(());
        }
        let mut stream = connect().ok_or(DiscordError::Unavailable)?;
        handshake(&mut *stream)?;
        self.connection = Some(stream);
        Ok(())
    }

    /// Send one command frame and drain its reply. Any I/O failure means Discord
    /// closed the pipe (quit or restarted), so the connection and start time are
    /// dropped — the next `set_activity` reconnects cleanly.
    fn exchange(&mut self, payload: &str) -> Result<(), DiscordError> {
        let outcome = self.write_command(payload);
        if outcome.is_err() {
            self.connection = None;
            self.started_at = None;
        }
        outcome
    }

    fn write_command(&mut self, payload: &str) -> Result<(), DiscordError> {
        let stream = self.connection.as_mut().ok_or(DiscordError::Unavailable)?;
        write_frame(&mut **stream, Opcode::Frame, payload)?;
        read_frame(&mut **stream)
    }
}

/// The caller-provided activity content: two status lines and an optional small
/// overlay icon (art-asset key + hover text). Borrowed from the command's owned
/// `String`s for the length of one send.
struct ActivityInput<'a> {
    details: Option<&'a str>,
    state: Option<&'a str>,
    image: Option<&'a str>,
    caption: Option<&'a str>,
}

/// The activity content plus the shared framing inputs, passed as one struct
/// rather than a fistful of positional args.
struct ActivityFields<'a> {
    nonce: &'a str,
    details: Option<&'a str>,
    state: Option<&'a str>,
    image: Option<&'a str>,
    caption: Option<&'a str>,
    started_at: u64,
}

/// Build the `SET_ACTIVITY` JSON for a live activity. `details`/`state`/the small
/// overlay are omitted when `None`; the PADE large asset and the run's start
/// timestamp are always included.
fn activity_payload(fields: ActivityFields) -> String {
    let ActivityFields {
        nonce,
        details,
        state,
        image,
        caption,
        started_at,
    } = fields;

    let mut activity = serde_json::Map::new();
    if let Some(details) = details {
        activity.insert("details".to_owned(), Value::from(details));
    }
    if let Some(state) = state {
        activity.insert("state".to_owned(), Value::from(state));
    }

    let mut assets = serde_json::Map::new();
    assets.insert("large_image".to_owned(), Value::from(ART_ASSET_KEY));
    assets.insert("large_text".to_owned(), Value::from("PADE"));
    if let Some(image) = image {
        assets.insert("small_image".to_owned(), Value::from(image));
    }
    if let Some(caption) = caption {
        assets.insert("small_text".to_owned(), Value::from(caption));
    }
    activity.insert("assets".to_owned(), Value::Object(assets));
    activity.insert("timestamps".to_owned(), json!({ "start": started_at }));

    json!({
        "cmd": SET_ACTIVITY,
        "nonce": nonce,
        "args": { "pid": std::process::id(), "activity": Value::Object(activity) },
    })
    .to_string()
}

/// Build the `SET_ACTIVITY` JSON that clears presence (a null activity).
fn clear_payload(nonce: &str) -> String {
    json!({
        "cmd": SET_ACTIVITY,
        "nonce": nonce,
        "args": { "pid": std::process::id(), "activity": Value::Null },
    })
    .to_string()
}

/// Perform the opening handshake and drain the READY dispatch that follows, so the
/// pipe buffer doesn't fill before the first command.
fn handshake(stream: &mut dyn DuplexStream) -> Result<(), DiscordError> {
    let payload = json!({ "v": 1, "client_id": APPLICATION_ID }).to_string();
    write_frame(stream, Opcode::Handshake, &payload)?;
    read_frame(stream)
}

/// Frame `payload` under `opcode` and write it out.
fn write_frame(
    stream: &mut dyn DuplexStream,
    opcode: Opcode,
    payload: &str,
) -> Result<(), DiscordError> {
    let frame = encode_frame(opcode, payload.as_bytes())?;
    stream.write_all(&frame)?;
    stream.flush()?;
    Ok(())
}

/// Encode one IPC frame: little-endian `opcode` and `length` headers followed by
/// the payload bytes.
fn encode_frame(opcode: Opcode, payload: &[u8]) -> Result<Vec<u8>, DiscordError> {
    let length = u32::try_from(payload.len()).map_err(|_| DiscordError::OversizedFrame)?;
    let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    frame.extend_from_slice(&opcode.code().to_le_bytes());
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Read exactly one response frame and discard it — PADE never inspects Discord's
/// replies, it only needs to keep the stream drained.
fn read_frame(stream: &mut dyn DuplexStream) -> Result<(), DiscordError> {
    let mut header = [0u8; FRAME_HEADER_LEN];
    stream.read_exact(&mut header)?;
    let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    let length = usize::try_from(length).map_err(|_| DiscordError::OversizedFrame)?;
    if length > MAX_FRAME_PAYLOAD {
        return Err(DiscordError::OversizedFrame);
    }
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload)?;
    Ok(())
}

static NONCE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A per-command nonce: an atomic counter combined with the current time, unique
/// within and across runs without needing a UUID crate.
fn next_nonce() -> String {
    let count = NONCE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_millis(),
        Err(_) => 0,
    };
    format!("{millis}-{count}")
}

/// Seconds since the Unix epoch, for `timestamps.start`.
fn now_unix_seconds() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs(),
        Err(_) => 0,
    }
}

/// Open Discord's IPC named pipe, trying `discord-ipc-0` through `-9`; the first
/// that opens wins. A duplex message pipe opens read+write through plain
/// `OpenOptions` — no `unsafe`. `None` when none answer (Discord isn't running).
#[cfg(windows)]
fn connect() -> Option<Box<dyn DuplexStream>> {
    for index in 0..=9 {
        let path = format!(r"\\.\pipe\discord-ipc-{index}");
        if let Ok(pipe) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        {
            return Some(Box::new(pipe));
        }
    }
    None
}

/// Open Discord's IPC domain socket, trying `discord-ipc-0` through `-9` under each
/// candidate runtime directory. `None` when none answer.
#[cfg(unix)]
fn connect() -> Option<Box<dyn DuplexStream>> {
    use std::os::unix::net::UnixStream;

    for dir in socket_dirs() {
        for index in 0..=9 {
            let path = dir.join(format!("discord-ipc-{index}"));
            if let Ok(stream) = UnixStream::connect(&path) {
                return Some(Box::new(stream));
            }
        }
    }
    None
}

/// Directories the Discord socket may live in, most-specific first: the XDG
/// runtime dir, then the temp dir, then `/tmp`.
#[cfg(unix)]
fn socket_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    for variable in ["XDG_RUNTIME_DIR", "TMPDIR"] {
        if let Some(value) = std::env::var_os(variable) {
            dirs.push(std::path::PathBuf::from(value));
        }
    }
    dirs.push(std::path::PathBuf::from("/tmp"));
    dirs
}

/// Publish PADE's rich presence. `details` (the project line), `state` (the
/// language line), and `image`/`caption` (the small language overlay icon key and
/// its hover text) are all optional. Arg names are single words so the JS payload
/// maps 1:1 (no snake/camel conversion); the managed state is named `presence`
/// because the activity's own `state` field is a separate argument the frontend
/// sends. Best-effort: a missing Discord returns an error the frontend ignores.
#[tauri::command]
pub fn discord_set_activity(
    presence: State<DiscordState>,
    details: Option<String>,
    state: Option<String>,
    image: Option<String>,
    caption: Option<String>,
) -> Result<(), String> {
    presence.dispatch(Command::Set(OwnedActivity {
        details,
        state,
        image,
        caption,
    }))
}

/// Clear PADE's rich presence, keeping the connection for a later re-enable.
#[tauri::command]
pub fn discord_clear_activity(presence: State<DiscordState>) -> Result<(), String> {
    presence.dispatch(Command::Clear)
}

#[cfg(test)]
mod tests {
    use super::{activity_payload, encode_frame, ActivityFields, Opcode, FRAME_HEADER_LEN};

    #[test]
    fn frame_header_is_little_endian_opcode_then_length() {
        let payload: &[u8] = br#"{"v":1}"#;
        let frame = encode_frame(Opcode::Frame, payload).expect("small payload encodes");
        assert_eq!(&frame[0..4], &Opcode::Frame.code().to_le_bytes());
        let length = u32::try_from(payload.len()).expect("length fits");
        assert_eq!(&frame[4..8], &length.to_le_bytes());
        assert_eq!(&frame[FRAME_HEADER_LEN..], payload);
    }

    #[test]
    fn handshake_opcode_encodes_as_zero() {
        let frame = encode_frame(Opcode::Handshake, b"{}").expect("encodes");
        assert_eq!(&frame[0..4], &0u32.to_le_bytes());
    }

    #[test]
    fn activity_payload_omits_absent_fields_and_carries_the_start_time() {
        let payload = activity_payload(ActivityFields {
            nonce: "nonce-1",
            details: Some("Working on pade"),
            state: None,
            image: None,
            caption: None,
            started_at: 1_700_000_000,
        });
        let value: serde_json::Value = serde_json::from_str(&payload).expect("valid json");
        assert_eq!(value["cmd"], serde_json::json!("SET_ACTIVITY"));
        let activity = &value["args"]["activity"];
        assert_eq!(activity["details"], serde_json::json!("Working on pade"));
        assert!(activity.get("state").is_none());
        assert_eq!(activity["assets"]["large_image"], serde_json::json!("pade"));
        // No language passed → no small overlay, just the PADE large asset.
        assert!(activity["assets"].get("small_image").is_none());
        assert_eq!(
            activity["timestamps"]["start"],
            serde_json::json!(1_700_000_000u64)
        );
    }

    #[test]
    fn activity_payload_carries_the_language_overlay_when_given() {
        let payload = activity_payload(ActivityFields {
            nonce: "nonce-3",
            details: Some("Working on pade"),
            state: Some("Rust"),
            image: Some("rust"),
            caption: Some("Rust"),
            started_at: 1_700_000_000,
        });
        let value: serde_json::Value = serde_json::from_str(&payload).expect("valid json");
        let assets = &value["args"]["activity"]["assets"];
        assert_eq!(assets["large_image"], serde_json::json!("pade"));
        assert_eq!(assets["small_image"], serde_json::json!("rust"));
        assert_eq!(assets["small_text"], serde_json::json!("Rust"));
    }

    #[test]
    fn clear_payload_sends_a_null_activity() {
        let payload = super::clear_payload("nonce-2");
        let value: serde_json::Value = serde_json::from_str(&payload).expect("valid json");
        assert_eq!(value["cmd"], serde_json::json!("SET_ACTIVITY"));
        assert_eq!(value["args"]["activity"], serde_json::Value::Null);
    }
}
