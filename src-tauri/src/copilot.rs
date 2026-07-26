//! Anonymous Copilot (`copilot.microsoft.com`) as a session-name source.
//!
//! This talks to consumer Copilot's chat WebSocket directly, as an anonymous
//! *guest* — no Microsoft account, no bundled SDK, no browser. The flow (verified
//! against the `reverse-engineer-copliot` reference):
//!
//!   1. `POST /c/api/start` → mints the `__Host-copilot-anon` cookie that
//!      authorizes the socket.
//!   2. open `wss://copilot.microsoft.com/c/api/chat` with `Origin:
//!      https://copilot.microsoft.com` and that cookie. Setting the origin freely
//!      is what a browser extension *cannot* do (its origin is forged-proof), and
//!      is why staying same-site here needs no `support.microsoft.com` detour.
//!   3. the server opens with a `hashcash` challenge; solve the proof-of-work.
//!   4. `send` the prompt; reassemble the `appendText` deltas until `done`.
//!
//! Best-effort by design: the first message can be gated by Cloudflare Turnstile
//! (a bot check a headless client can't clear) or by region/consent rules. Every
//! failure path returns `None`, so `naming.rs` falls back to the agent CLI
//! (`claude -p` / `codex exec`) — Copilot is a lighter-weight *preferred* source,
//! never a hard dependency. Runs on the naming worker thread (`spawn_blocking`),
//! so the blocking HTTP/WS calls never touch the UI thread.

use std::net::TcpStream;
use std::time::Instant;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{client::IntoClientRequest, Message, WebSocket};

use crate::naming::{session_naming_prompt, NameContext, Namer, NAME_TIMEOUT};

const ORIGIN: &str = "https://copilot.microsoft.com";
const START_URL: &str = "https://copilot.microsoft.com/c/api/start";
const CHAT_URL: &str = "wss://copilot.microsoft.com/c/api/chat?api-version=2";
/// An Edge-on-Windows UA — the client Copilot's guest surface expects to see.
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36 Edg/149.0.0.0";

/// A `hashcash` difficulty this far past what the guest tier asks for (a handful
/// of bits) means something is wrong; refuse rather than spin. 24 zero bits is
/// already ~16M hashes.
const MAX_HASHCASH_DIFFICULTY: u32 = 24;

/// How long to block on any single socket read before looping back to re-check
/// the overall deadline — so a silent server can't wedge the naming worker.
const SOCKET_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

pub struct CopilotNamer;

impl Namer for CopilotNamer {
    /// Project naming stays with the agent CLI / heuristic (it works from a file
    /// list, not a conversation); Copilot only names live sessions.
    fn suggest(&self, _context: &NameContext) -> Option<String> {
        None
    }
}

impl CopilotNamer {
    /// Name a live session from its (cleaned) transcript via an anonymous Copilot
    /// guest chat. `None` on any failure so naming falls back to the agent CLI.
    #[allow(clippy::unused_self)]
    pub fn suggest_session(&self, transcript: &str) -> Option<String> {
        run_chat(&session_naming_prompt(transcript))
    }
}

/// Open an anonymous guest session and return the cookie that authorizes the chat
/// socket. `None` if the bootstrap fails or sets no cookie.
fn start_guest_session() -> Option<String> {
    let response = ureq::post(START_URL)
        .set("user-agent", USER_AGENT)
        .set("origin", ORIGIN)
        .set("referer", "https://copilot.microsoft.com/")
        .set("accept-language", "en-US,en;q=0.9")
        .call()
        .ok()?;

    // Keep each cookie's "name=value", dropping its attributes, and rejoin — the
    // socket only needs the pairs.
    let cookie = response
        .all("set-cookie")
        .iter()
        .filter_map(|entry| entry.split(';').next())
        .collect::<Vec<_>>()
        .join("; ");
    (!cookie.is_empty()).then_some(cookie)
}

/// Solve Copilot's hashcash proof-of-work: the smallest nonce for which
/// `SHA-256(token + nonce)` begins with `difficulty` zero *bits*. `None` if the
/// difficulty is implausibly high (guarding against an unbounded spin).
fn solve_hashcash(token: &str, difficulty: u32) -> Option<u64> {
    if difficulty > MAX_HASHCASH_DIFFICULTY {
        return None;
    }
    let whole_zero_bytes = (difficulty / 8) as usize;
    let leftover_bits = difficulty % 8;
    let leftover_mask: u8 = if leftover_bits > 0 {
        0xffu8 << (8 - leftover_bits)
    } else {
        0
    };

    for nonce in 0u64.. {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hasher.update(nonce.to_string().as_bytes());
        let digest = hasher.finalize();

        let leading_zero = digest[..whole_zero_bytes].iter().all(|byte| *byte == 0);
        let leftover_ok = leftover_bits == 0 || (digest[whole_zero_bytes] & leftover_mask) == 0;
        if leading_zero && leftover_ok {
            return Some(nonce);
        }
    }
    None
}

/// A 21-char id in Copilot's conversation-id alphabet, seeded from random bytes
/// (two v4 UUIDs give 32 bytes; the mapping needn't be cryptographic).
fn new_conversation_id() -> String {
    const ALPHABET: &[u8] = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes[..21]
        .iter()
        .map(|byte| ALPHABET[*byte as usize % ALPHABET.len()] as char)
        .collect()
}

/// Bound each socket read so a quiet server can't hang the worker; the read loop
/// still re-checks the wall-clock deadline between reads.
fn arm_read_timeout(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) {
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

/// Run one anonymous guest chat turn and return the assembled reply text. `None`
/// on any failure (offline, region/consent gate, Turnstile, malformed frames).
fn run_chat(prompt: &str) -> Option<String> {
    let cookie = start_guest_session()?;

    let url = format!("{CHAT_URL}&clientSessionId={}", uuid::Uuid::new_v4());
    let mut request = url.into_client_request().ok()?;
    let headers = request.headers_mut();
    headers.insert("origin", ORIGIN.parse().ok()?);
    headers.insert("user-agent", USER_AGENT.parse().ok()?);
    headers.insert("cookie", cookie.parse().ok()?);

    let (mut socket, _response) = tungstenite::connect(request).ok()?;
    arm_read_timeout(&mut socket);

    // Announce capabilities up front; the prompt itself is sent only once the
    // opening challenge is cleared (mirrors the reference client's ordering).
    send_frame(
        &mut socket,
        &json!({
            "event": "setOptions",
            "supportedFeatures": [],
            "supportedCards": [],
            "supportedActions": [],
        }),
    )?;
    send_frame(
        &mut socket,
        &json!({ "event": "reportLocalConsents", "grantedConsents": [] }),
    )?;

    let send_frame_value = json!({
        "event": "send",
        "conversationId": new_conversation_id(),
        "content": [{ "type": "text", "text": prompt }],
        "mode": "smart",
        "context": {},
    });

    let started = Instant::now();
    let mut reply = String::new();
    while started.elapsed() < NAME_TIMEOUT {
        // A read timeout (or transient error) surfaces here; loop back to
        // re-check the deadline rather than aborting the turn.
        let Ok(message) = socket.read() else {
            continue;
        };
        let Message::Text(text) = message else {
            if matches!(message, Message::Close(_)) {
                break;
            }
            continue;
        };
        let Ok(frame) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        match frame.get("event").and_then(Value::as_str) {
            Some("challenge") => {
                answer_challenge(&mut socket, &frame)?;
                send_frame(&mut socket, &send_frame_value)?;
            }
            Some("appendText") => {
                if let Some(delta) = frame.get("text").and_then(Value::as_str) {
                    reply.push_str(delta);
                }
            }
            // `done` ends the turn; `error` (e.g. Turnstile → `invalid-event`)
            // means the guest path is walled right now — bail to the CLI fallback.
            Some("done" | "error") => break,
            _ => {}
        }
    }

    let _ = socket.close(None);
    let trimmed = reply.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Solve and answer a `hashcash` challenge frame. A `null`/non-hashcash challenge
/// (seen on some guest sessions) needs no proof — the caller just proceeds to
/// send. Returns `None` only when a hashcash we should have solved was refused.
fn answer_challenge(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    frame: &Value,
) -> Option<()> {
    let is_hashcash = frame.get("method").and_then(Value::as_str) == Some("hashcash");
    let Some(parameter) = frame.get("parameter").and_then(Value::as_str) else {
        return Some(());
    };
    if !is_hashcash {
        return Some(());
    }

    let (token, difficulty_text) = parameter.split_once(':')?;
    let difficulty = difficulty_text.parse::<u32>().ok()?;
    let nonce = solve_hashcash(token, difficulty)?;
    send_frame(
        socket,
        &json!({ "event": "challengeResponse", "method": "hashcash", "token": nonce.to_string() }),
    )
}

/// Serialize and write one JSON frame. `None` if serialization or the socket write
/// fails, so the caller aborts the turn.
fn send_frame(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, frame: &Value) -> Option<()> {
    let text = serde_json::to_string(frame).ok()?;
    socket.send(Message::Text(text)).ok()
}

#[cfg(test)]
mod tests {
    use super::{new_conversation_id, solve_hashcash};
    use sha2::{Digest, Sha256};

    const ALPHABET: &str = "useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";

    #[test]
    fn solve_hashcash_finds_a_nonce_with_the_required_leading_zero_bits() {
        let difficulty = 8;
        let nonce = solve_hashcash("abc", difficulty).expect("a nonce exists for 8 bits");

        let mut hasher = Sha256::new();
        hasher.update(b"abc");
        hasher.update(nonce.to_string().as_bytes());
        let digest = hasher.finalize();
        assert_eq!(
            digest[0], 0,
            "8 leading zero bits means the first byte is zero"
        );
    }

    #[test]
    fn solve_hashcash_refuses_an_implausible_difficulty() {
        assert_eq!(solve_hashcash("abc", 64), None);
    }

    #[test]
    fn a_conversation_id_is_21_chars_from_the_alphabet() {
        let id = new_conversation_id();
        assert_eq!(id.chars().count(), 21);
        assert!(id.chars().all(|character| ALPHABET.contains(character)));
    }
}
