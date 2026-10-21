//! Copilot (`copilot.com`) as a session-name source, as the personal Microsoft
//! account Windows is already signed in to.
//!
//!   1. A token comes from Microsoft's `msalruntime.dll` (Git for Windows ships
//!      it), silently, through a PowerShell helper (`copilot_token.ps1`): this
//!      crate forbids `unsafe`, so it never calls the DLL itself. The token is
//!      cached for a while, so a burst of calls pays the PowerShell start once.
//!   2. The chat is the `SignalR` socket on `substrate.office.com` that the
//!      copilot.com page opens: a handshake, then one `chat` call whose reply
//!      comes back as the call's result.
//!
//! The free allowance is 50 messages a day and every call spends one, so a
//! spent allowance parks Copilot for a while. Each call also leaves a chat in
//! the account's Copilot history. Best-effort by design: every failure returns
//! `None`, so `naming.rs` falls back to the agent CLI (`claude -p` / `codex
//! exec`). Runs on the naming worker thread (`spawn_blocking`), so the blocking
//! calls never touch the UI thread.

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{client::IntoClientRequest, Message, WebSocket};

use crate::agents::naming::{run_agent_reply, session_naming_prompt, NAME_TIMEOUT};

const HUB: &str = "wss://substrate.office.com";
/// The chat page's own origin, which the hub checks.
const ORIGIN: &str = "https://copilot.com";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36 Edg/155.0.0.0";
/// What the hub expects after each `SignalR` message.
const RECORD_END: char = '\u{1e}';
/// The helper that prints a token, run by PowerShell.
const TOKEN_SCRIPT: &str = include_str!("copilot_token.ps1");
/// The environment variable that names `msalruntime.dll`, for the helper and
/// for overriding where this looks for it.
const MSAL_RUNTIME_VARIABLE: &str = "PADE_MSALRUNTIME";
const POWERSHELL_FLAGS: [&str; 5] = [
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-EncodedCommand",
];

/// How long a token is reused. They last longer; this keeps one fresh.
const TOKEN_LIFETIME: Duration = Duration::from_mins(20);
/// How long Copilot is left alone once its allowance is spent. The hub names a
/// reset time, but a fixed pause is simpler and checks back in the same day.
const SPENT_ALLOWANCE_PAUSE: Duration = Duration::from_hours(1);

/// How long to block on any single socket read before looping back to re-check
/// the overall deadline - so a silent server can't wedge the naming worker.
const SOCKET_READ_TIMEOUT: Duration = Duration::from_secs(8);

/// `SignalR` message types this client reacts to.
const SIGNALR_RESULT: u64 = 2;
const SIGNALR_COMPLETION: u64 = 3;
const SIGNALR_CLOSE: u64 = 7;

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// A token and when it was minted.
static TOKEN: Mutex<Option<(String, Instant)>> = Mutex::new(None);
/// When a spent allowance was noticed.
static SPENT_SINCE: Mutex<Option<Instant>> = Mutex::new(None);

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

/// Why a chat gave no reply.
#[derive(Debug, PartialEq, Eq)]
enum ChatError {
    /// The hub turned the token away, so the cached one is stale.
    TokenRefused,
    /// The day's free messages are used up.
    AllowanceSpent,
    Failed,
}

/// Name a live session from its (cleaned) transcript via a Copilot chat. `None`
/// on any failure so naming falls back to the agent CLI. Project naming stays
/// with the agent CLI / heuristic (it works from a file list, not a
/// conversation); Copilot only names live sessions.
pub(crate) fn suggest_session_name(transcript: &str) -> Option<String> {
    run_chat(&session_naming_prompt(transcript))
}

/// Run one chat turn and return the reply text. `None` on any failure (no
/// account, offline, spent allowance, malformed frames).
fn run_chat(prompt: &str) -> Option<String> {
    if is_allowance_spent() {
        return None;
    }
    let token = cached_token()?;
    match chat(HUB, &token, prompt) {
        Ok(reply) => Some(reply),
        Err(ChatError::AllowanceSpent) => {
            *lock(&SPENT_SINCE) = Some(Instant::now());
            None
        }
        Err(ChatError::TokenRefused) => {
            *lock(&TOKEN) = None;
            None
        }
        Err(ChatError::Failed) => None,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn is_allowance_spent() -> bool {
    lock(&SPENT_SINCE).is_some_and(|since| since.elapsed() < SPENT_ALLOWANCE_PAUSE)
}

/// The cached token while it is fresh, else a new one from Windows.
fn cached_token() -> Option<String> {
    let mut cached = lock(&TOKEN);
    if let Some((token, minted)) = cached.as_ref() {
        if minted.elapsed() < TOKEN_LIFETIME {
            return Some(token.clone());
        }
    }
    let token = windows_token()?;
    *cached = Some((token.clone(), Instant::now()));
    Some(token)
}

/// `msalruntime.dll`: the one `PADE_MSALRUNTIME` names, else the copy in Git
/// for Windows.
fn msal_runtime() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os(MSAL_RUNTIME_VARIABLE) {
        return Some(PathBuf::from(named));
    }
    let git_dll = |root: PathBuf| {
        root.join("Git")
            .join("mingw64")
            .join("bin")
            .join("msalruntime.dll")
    };
    ["ProgramFiles", "ProgramFiles(x86)"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|root| git_dll(root.into()))
        .chain(
            std::env::var_os("LOCALAPPDATA")
                .map(|root| git_dll(PathBuf::from(root).join("Programs"))),
        )
        .find(|dll| dll.is_file())
}

/// A token for the personal Microsoft account Windows is signed in to.
fn windows_token() -> Option<String> {
    let dll = msal_runtime()?;
    let script = format!(
        "$env:{MSAL_RUNTIME_VARIABLE} = '{}'\n{TOKEN_SCRIPT}",
        dll.display().to_string().replace('\'', "''")
    );
    let token = run_agent_reply(
        Path::new("powershell.exe"),
        &POWERSHELL_FLAGS,
        None,
        encode_command(&script),
    )?;
    let trimmed = token.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// PowerShell's `-EncodedCommand` form: the script as UTF-16LE, in base64.
fn encode_command(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = chunk.iter().enumerate().fold(0u32, |bits, (i, byte)| {
            bits | u32::from(*byte) << (16 - 8 * i)
        });
        for sextet in 0..=chunk.len() {
            let index = usize::try_from((group >> (18 - 6 * sextet)) & 0x3f).unwrap_or(0);
            encoded.push(char::from(BASE64_ALPHABET[index]));
        }
        for _ in chunk.len()..3 {
            encoded.push('=');
        }
    }
    encoded
}

/// Bound each socket read so a quiet server can't hang the worker; the read loop
/// still re-checks the wall-clock deadline between reads.
fn arm_read_timeout(socket: &mut Socket) {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => {
            let _ = stream.set_read_timeout(Some(SOCKET_READ_TIMEOUT));
        }
        MaybeTlsStream::Rustls(stream) => {
            let _ = stream.get_ref().set_read_timeout(Some(SOCKET_READ_TIMEOUT));
        }
        _ => {}
    }
}

/// One `SignalR` message: its JSON and the record separator.
fn record(value: &Value) -> Message {
    Message::text(format!("{value}{RECORD_END}"))
}

/// The `chat` call, with the least the hub accepts: no web search, no plugins,
/// and a new conversation every time.
fn chat_call(id: &str, prompt: &str) -> Value {
    json!({
        "arguments": [{
            "source": "officeweb",
            "clientCorrelationId": id,
            "sessionId": id,
            "requestId": id,
            "traceId": id,
            "isStartOfSession": true,
            "optionsSets": ["enable_msa_user"],
            "streamingMode": "ConciseWithPadding",
            "options": {},
            "extraExtensionParameters": {},
            "allowedMessageTypes": ["Chat", "Progress", "EndOfRequest"],
            "sliceIds": [],
            "threadLevelGptId": {},
            "message": {
                "author": "user",
                "inputMethod": "Keyboard",
                "text": prompt,
                "requestId": id,
                "locale": "en-us",
                "messageType": "Chat",
                "experienceType": "Default",
                "adaptiveCards": [],
            },
            "plugins": [],
            "tone": "Magic",
            "disconnectBehavior": "continue",
        }],
        "invocationId": "0",
        "target": "chat",
        "type": 4,
    })
}

/// The reply in a finished `chat` call (`SignalR`'s type 2 message), or why there
/// isn't one.
fn reply_of(call: &Value) -> Result<String, ChatError> {
    let item = &call["item"];
    let meter = item.pointer("/result/meteringInformation/0");
    let is_allowance_spent = meter.is_some_and(|meter| {
        meter["hasAccess"] == false || meter["remainingAllowance"].as_f64() == Some(0.0)
    });
    if is_allowance_spent {
        return Err(ChatError::AllowanceSpent);
    }
    let is_success = item.pointer("/result/value").and_then(Value::as_str) == Some("Success");
    if !is_success {
        return Err(ChatError::Failed);
    }
    // The chat's own answer is the last bot message that has text and isn't a
    // progress note, hint or other typed message.
    item["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|message| message["author"] == "bot" && message.get("messageType").is_none())
        .filter_map(|message| message["text"].as_str())
        .next_back()
        .map(str::trim)
        .filter(|reply| !reply.is_empty())
        .map(str::to_string)
        .ok_or(ChatError::Failed)
}

/// One chat against `hub`: the reply's text.
fn chat(hub: &str, token: &str, prompt: &str) -> Result<String, ChatError> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let url = format!(
        "{hub}/m365Copilot/Chathub/?chatsessionid={id}&clientrequestid={id}&X-SessionId={}&ConversationId={}&access_token={token}\
         &source=%22officeweb%22&product=Office&agentHost=Bizchat.FullScreen&licenseType=Free&isEdu=false&agent=web&scenario=OfficeWebFreeConsumerCopilot",
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4()
    );
    let mut request = url.into_client_request().map_err(|_| ChatError::Failed)?;
    let headers = request.headers_mut();
    headers.insert("origin", ORIGIN.parse().map_err(|_| ChatError::Failed)?);
    headers.insert(
        "user-agent",
        USER_AGENT.parse().map_err(|_| ChatError::Failed)?,
    );

    let (mut socket, _response) = tungstenite::connect(request).map_err(|error| match error {
        // The hub turns away a token it doesn't take with a 401.
        tungstenite::Error::Http(response) if response.status() == 401 => ChatError::TokenRefused,
        _ => ChatError::Failed,
    })?;
    arm_read_timeout(&mut socket);
    let reply = converse(&mut socket, &id, prompt);
    let _ = socket.close(None);
    reply
}

/// The handshake, the `chat` call, and the wait for its result.
fn converse(socket: &mut Socket, id: &str, prompt: &str) -> Result<String, ChatError> {
    socket
        .send(record(&json!({ "protocol": "json", "version": 1 })))
        .map_err(|_| ChatError::Failed)?;

    let started = Instant::now();
    let mut is_asked = false;
    while started.elapsed() < NAME_TIMEOUT {
        let message = match socket.read() {
            Ok(message) => message,
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => return Err(ChatError::Failed),
        };
        let Message::Text(text) = message else {
            if matches!(message, Message::Close(_)) {
                return Err(ChatError::Failed);
            }
            continue;
        };
        for part in text.split(RECORD_END).filter(|part| !part.is_empty()) {
            let Ok(frame) = serde_json::from_str::<Value>(part) else {
                continue;
            };
            // `{}` answers the handshake: now the chat can be asked.
            let is_handshake_answer = frame.as_object().is_some_and(serde_json::Map::is_empty);
            if is_handshake_answer && !is_asked {
                is_asked = true;
                socket
                    .send(record(&chat_call(id, prompt)))
                    .map_err(|_| ChatError::Failed)?;
                continue;
            }
            match frame["type"].as_u64() {
                Some(SIGNALR_RESULT) => return reply_of(&frame),
                Some(SIGNALR_COMPLETION | SIGNALR_CLOSE) => return Err(ChatError::Failed),
                _ => {}
            }
        }
    }
    Err(ChatError::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A fake hub on a local port: it answers the handshake, checks the URL and
    /// the `chat` call, and replies the way the real one does, with a hint and
    /// then the answer in a type 2 frame. Returns its address.
    #[allow(clippy::result_large_err)]
    fn fake_hub(result: Value) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
        let address = format!("ws://{}", listener.local_addr().expect("a local address"));
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("a client");
            let mut seen_url = String::new();
            let callback =
                |request: &tungstenite::handshake::server::Request,
                 response: tungstenite::handshake::server::Response| {
                    seen_url = request.uri().to_string();
                    Ok(response)
                };
            let mut socket = tungstenite::accept_hdr(stream, callback).expect("a handshake");
            assert!(
                seen_url.starts_with("/m365Copilot/Chathub/?")
                    && seen_url.contains("access_token=a-token"),
                "{seen_url}"
            );
            while let Ok(Message::Text(text)) = socket.read() {
                let frame: Value =
                    serde_json::from_str(text.trim_end_matches(RECORD_END)).expect("a frame");
                if frame["protocol"] == "json" {
                    socket.send(record(&json!({}))).expect("answer");
                } else if frame["target"] == "chat" {
                    let sent = &frame["arguments"][0]["message"];
                    assert!(sent["text"].as_str().expect("text").starts_with("Name it"));
                    socket
                        .send(record(&json!({ "type": 1, "target": "update" })))
                        .expect("update");
                    socket.send(record(&result)).expect("result");
                }
            }
        });
        address
    }

    fn answered(messages: &Value, metering: &Value) -> Value {
        json!({ "type": 2, "invocationId": "0", "item": {
            "messages": messages,
            "result": { "value": "Success", "meteringInformation": metering },
        }})
    }

    #[test]
    fn a_chat_answers_with_the_last_untyped_bot_text() {
        let messages = json!([
            { "author": "user", "text": "Name it" },
            { "author": "bot", "messageType": "HintInvocation", "text": "respond_language()" },
            { "author": "bot", "text": "Fix login bug\n" },
        ]);
        let hub = fake_hub(answered(
            &messages,
            &json!([{ "hasAccess": true, "remainingAllowance": 40 }]),
        ));
        assert_eq!(
            chat(&hub, "a-token", "Name it please"),
            Ok("Fix login bug".to_string())
        );
    }

    #[test]
    fn a_spent_allowance_is_reported_as_spent() {
        let hub = fake_hub(answered(
            &json!([{ "author": "bot", "text": "late" }]),
            &json!([{ "hasAccess": false, "remainingAllowance": 0, "resetAt": "2026-10-22T00:00:00+00:00" }]),
        ));
        assert_eq!(
            chat(&hub, "a-token", "Name it please"),
            Err(ChatError::AllowanceSpent)
        );
    }

    #[test]
    fn a_refused_result_is_a_failure() {
        let refused = json!({ "type": 2, "item": { "result": { "value": "Throttled" } } });
        assert_eq!(reply_of(&refused), Err(ChatError::Failed));
    }

    #[test]
    fn a_result_with_no_bot_text_is_a_failure() {
        let hints_only = answered(
            &json!([{ "author": "bot", "messageType": "Progress", "text": "Just a sec" }]),
            &json!([]),
        );
        assert_eq!(reply_of(&hints_only), Err(ChatError::Failed));
    }

    #[test]
    fn an_encoded_command_is_utf16_little_endian_base64() {
        assert_eq!(encode_command("a"), "YQA=");
        assert_eq!(encode_command("ab"), "YQBiAA==");
        assert_eq!(encode_command("abc"), "YQBiAGMA");
    }

    /// Spends one of the day's 50 free messages: run by hand with `--ignored`.
    #[test]
    #[ignore = "talks to the real Copilot"]
    fn a_live_session_is_named() {
        let transcript = "user: the login page rejects valid passwords after the session timeout
agent: found the bug in the token refresh and fixed it";
        let name = suggest_session_name(transcript);
        eprintln!("{name:?}");
        assert!(name.is_some());
    }
}
