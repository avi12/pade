//! A session's output: decoding the PTY's byte stream, the two bounded buffers
//! kept from it (the ANSI-stripped transcript the session namer reads, and the
//! raw replay history a re-attaching terminal paints from), and the pump thread
//! that feeds both and hands each chunk on to whoever listens.

use std::io::Read;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::registry::SessionOwner;

/// Where a session's output goes once it is in the session's own buffers - in
/// the PTY host, the connected app. A trait so the session engine can run (and
/// be tested) with no transport at all.
pub(crate) trait SessionEvents: Send + Sync {
    /// One decoded chunk, already appended to the history at `sequence`.
    fn output(&self, owner: &str, id: &str, data: &str, sequence: u64);
    /// The session's output ended: its process exited (EOF or a read error).
    fn exited(&self, owner: &str, id: &str);
}

/// A session's replayable output, and how many chunks have been emitted for it.
/// The counter is what makes the handover exact: the frontend can be listening to
/// the live stream while it asks for the history, and `seq` tells it which of the
/// chunks it caught are already inside that history (see `pty_history`).
///
/// `seq` invariants the frontend's splice depends on (Hyrum): 1-based, +1 per
/// *emitted* chunk (empty decodes never reach here), `0` = empty/unknown
/// session, and `data` is the byte-trimmed but seq-complete concatenation of
/// chunks `1..=seq`.
#[derive(Default, Debug)]
pub(crate) struct History {
    /// Backing storage for the replay tail. `start` advances through this string
    /// as old output expires; compacting only occasionally avoids moving a near-
    /// 512 KiB tail on every PTY chunk once a session has been running for a while.
    data: String,
    start: usize,
    sequence: u64,
    /// Is the program currently painting the ALTERNATE screen (a fullscreen TUI, a
    /// pager)? Then this history is a stream of framebuffer edits, not a document,
    /// and replaying a trimmed one paints a torn frame. The frontend needs to know
    /// so it can ask the program to repaint itself instead.
    alternate: bool,
}

impl History {
    /// The replayable tail as the frontend receives it.
    pub(crate) fn snapshot(&self) -> HistorySnapshot {
        HistorySnapshot {
            data: self.data[self.start..].to_string(),
            sequence: self.sequence,
            alternate: self.alternate,
        }
    }
}

/// A session's replayable output and the sequence number of the last chunk in it,
/// plus whether the program is currently painting the alternate screen. The
/// `pty_history` reply shape - and, unchanged, what the PTY host sends the app.
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistorySnapshot {
    data: String,
    #[serde(rename = "seq")]
    sequence: u64,
    alternate: bool,
}

/// A terminal control sequence: the Control Sequence Introducer (`ESC [`)
/// followed by the body, composed so the parts are named rather than one
/// opaque escape string.
macro_rules! control_sequence {
    ($body:literal) => {
        concat!("\x1b[", $body)
    };
}

/// DEC private mode 1049 - the alternate screen. A program sets the mode (`h`)
/// to take the screen over and resets it (`l`) to give it back. Wire constants
/// shared with `Terminal.svelte`, which writes the enter sequence when
/// replaying an alternate-screen history - the two sides must change together.
const ENTER_ALTERNATE_SCREEN: &str = control_sequence!("?1049h");
const LEAVE_ALTERNATE_SCREEN: &str = control_sequence!("?1049l");

/// How much recent output to keep per session for naming (bytes, tail-trimmed).
const TRANSCRIPT_CAPACITY: usize = 16 * 1024;

/// How much raw output to keep per session for replay. Bigger than the transcript
/// because this is the unstripped stream - escape codes and repaints included.
const HISTORY_CAPACITY: usize = 512 * 1024;

/// How many bytes one PTY read takes at most.
const READ_BUFFER_BYTES: usize = 8192;

const ESCAPE: char = '\u{1b}';
const BELL: char = '\u{7}';
/// Everything below the space is a C0 control byte.
const FIRST_PRINTABLE: char = ' ';

/// Strip terminal control sequences so the buffered transcript is plain text:
/// drop ESC-introduced CSI/OSC sequences, carriage returns, and other C0
/// control bytes, keeping printable characters, newlines, and tabs.
fn strip_ansi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut characters = input.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            ESCAPE => match characters.next() {
                Some('[') => skip_control_sequence(&mut characters),
                Some(']') => skip_operating_system_command(&mut characters),
                Some(_) | None => {}
            },
            '\r' => {}
            '\n' | '\t' => output.push(character),
            control if control < FIRST_PRINTABLE => {}
            printable => output.push(printable),
        }
    }
    output
}

/// Skip a CSI body: parameters up to and including a final byte in `@`..=`~`.
fn skip_control_sequence(characters: &mut impl Iterator<Item = char>) {
    for character in characters.by_ref() {
        if ('@'..='~').contains(&character) {
            break;
        }
    }
}

/// Skip an OSC body: up to and including BEL or the ST (`ESC \`) terminator.
fn skip_operating_system_command(characters: &mut std::iter::Peekable<impl Iterator<Item = char>>) {
    while let Some(character) = characters.next() {
        if character == BELL {
            break;
        }
        if character == ESCAPE {
            characters.next_if_eq(&'\\');
            break;
        }
    }
}

/// Append cleaned output to a session's transcript, trimming the front to
/// `TRANSCRIPT_CAPACITY` on a char boundary so it never grows unbounded.
fn append_transcript(transcript: &Mutex<String>, cleaned: &str) {
    if cleaned.is_empty() {
        return;
    }
    let Ok(mut buffer) = transcript.lock() else {
        return;
    };
    buffer.push_str(cleaned);
    if buffer.len() > TRANSCRIPT_CAPACITY {
        let mut cut = buffer.len() - TRANSCRIPT_CAPACITY;
        while cut < buffer.len() && !buffer.is_char_boundary(cut) {
            cut += 1;
        }
        buffer.drain(..cut);
    }
}

/// The screen the program switched to, judging by the last switch sequence in
/// `window`: `Some(true)` for the alternate screen, `Some(false)` for the
/// normal one, `None` when the window contains no switch at all.
fn last_screen_switch(window: &str) -> Option<bool> {
    let entered = window.rfind(ENTER_ALTERNATE_SCREEN);
    let left = window.rfind(LEAVE_ALTERNATE_SCREEN);
    match (entered, left) {
        (Some(enter), Some(leave)) => Some(enter > leave),
        (Some(_), None) => Some(true),
        (None, Some(_)) => Some(false),
        (None, None) => None,
    }
}

/// Append raw output to a session's replay history and take the sequence number of
/// this chunk. Trimming the front to `HISTORY_CAPACITY` cuts at a line break (and never
/// mid-character), so a replay starts on a whole line rather than halfway through
/// an escape sequence.
fn append_history(history: &Mutex<History>, raw: &str) -> u64 {
    let Ok(mut buffer) = history.lock() else {
        return 0;
    };
    buffer.data.push_str(raw);
    buffer.sequence += 1;

    // Track which screen the program is on: the last switch wins; no switch keeps
    // the current screen. Judged over a window reaching back past the chunk
    // boundary, so a switch sequence split across two reads still counts once its
    // tail arrives.
    let lookback = raw.len() + ENTER_ALTERNATE_SCREEN.len() - 1;
    let mut window_start = buffer.data.len().saturating_sub(lookback).max(buffer.start);
    while !buffer.data.is_char_boundary(window_start) {
        window_start += 1;
    }
    if let Some(alternate) = last_screen_switch(&buffer.data[window_start..]) {
        buffer.alternate = alternate;
    }

    if buffer.data.len() - buffer.start > HISTORY_CAPACITY {
        // Walk the cap overflow to a char boundary BEFORE slicing - landing inside
        // a multibyte character would panic - then cut just past the first line
        // break so a replay starts on a whole line. A tail with no line break at
        // all is dropped entirely rather than replayed torn.
        let mut overflow = buffer.start + (buffer.data.len() - buffer.start - HISTORY_CAPACITY);
        while !buffer.data.is_char_boundary(overflow) {
            overflow += 1;
        }
        let cut = buffer.data[overflow..]
            .find('\n')
            .map_or(buffer.data.len(), |line_break| overflow + line_break + 1);
        buffer.start = cut;

        // `String::drain` moves the retained tail. Do that only after a full
        // history window has expired, not once per output chunk.
        if buffer.start >= HISTORY_CAPACITY {
            let start = buffer.start;
            buffer.data.drain(..start);
            buffer.start = 0;
        }
    }

    buffer.sequence
}

/// Decode the completed UTF-8 prefix of `pending` and consume it, leaving an
/// incomplete trailing character in place - a PTY read can split one across two
/// chunks, and decoding the halves separately would turn both into U+FFFD.
/// Genuinely invalid bytes become one U+FFFD each and are consumed.
fn drain_decoded(pending: &mut Vec<u8>) -> String {
    let mut output = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(text) => {
                output.push_str(text);
                pending.clear();
                return output;
            }
            Err(error) => {
                let valid_end = error.valid_up_to();
                output.push_str(&String::from_utf8_lossy(&pending[..valid_end]));
                match error.error_len() {
                    // The chunk ends mid-character: keep the partial bytes for
                    // the next read to complete.
                    None => {
                        pending.drain(..valid_end);
                        return output;
                    }
                    Some(invalid_length) => {
                        output.push('\u{FFFD}');
                        pending.drain(..valid_end + invalid_length);
                    }
                }
            }
        }
    }
}

/// The halves of a session the output pump writes into.
pub(super) struct OutputBuffers {
    pub(super) transcript: Arc<Mutex<String>>,
    pub(super) history: Arc<Mutex<History>>,
}

/// Pump one session's output after it has been published in the registry. A chunk
/// enters replay history before it is handed on, so attach/replay handover
/// remains exact even when the frontend starts listening mid-stream. The owner
/// is read per chunk, so a session moved to another window streams there next.
pub(super) fn spawn_output_pump(
    events: Arc<dyn SessionEvents>,
    owner: SessionOwner,
    id: String,
    mut reader: Box<dyn Read + Send>,
    buffers: OutputBuffers,
) {
    std::thread::spawn(move || {
        let mut buffer = [0_u8; READ_BUFFER_BYTES];
        let mut pending = Vec::new();
        loop {
            match reader.read(&mut buffer) {
                Ok(bytes_read) if bytes_read > 0 => {
                    pending.extend_from_slice(&buffer[..bytes_read]);
                    let data = drain_decoded(&mut pending);
                    if data.is_empty() {
                        continue;
                    }
                    append_transcript(&buffers.transcript, &strip_ansi(&data));
                    let sequence = append_history(&buffers.history, &data);
                    events.output(&owner.label(), &id, &data, sequence);
                }
                // EOF (agent exited) or a read error - end the pump.
                _ => break,
            }
        }
        events.exited(&owner.label(), &id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::assert_empty;

    fn history_with(chunks: &[&str]) -> (Mutex<History>, u64) {
        let history = Mutex::new(History::default());
        let mut sequence = 0;
        for chunk in chunks {
            sequence = append_history(&history, chunk);
        }
        (history, sequence)
    }

    fn snapshot(history: &Mutex<History>) -> (String, bool) {
        let buffer = history.lock().expect("history lock");
        (buffer.data[buffer.start..].to_string(), buffer.alternate)
    }

    #[test]
    fn entering_the_alternate_screen_sets_the_flag() {
        let (history, _) = history_with(&["hello \x1b[?1049h painting"]);
        assert!(snapshot(&history).1);
    }

    #[test]
    fn leaving_the_alternate_screen_clears_the_flag() {
        let (history, _) = history_with(&["\x1b[?1049h tui", "bye \x1b[?1049l prompt"]);
        assert!(!snapshot(&history).1);
    }

    #[test]
    fn the_last_switch_in_a_chunk_wins() {
        let (history, _) = history_with(&["\x1b[?1049h tui \x1b[?1049l back"]);
        assert!(!snapshot(&history).1);

        let (history, _) = history_with(&["\x1b[?1049l \x1b[?1049h tui"]);
        assert!(snapshot(&history).1);
    }

    #[test]
    fn a_chunk_without_a_switch_keeps_the_current_screen() {
        let (history, _) = history_with(&["\x1b[?1049h tui", "more painting"]);
        assert!(snapshot(&history).1);
    }

    #[test]
    fn a_switch_split_across_two_reads_still_counts() {
        let (enter_head, enter_tail) = ENTER_ALTERNATE_SCREEN.split_at(4);
        let (history, _) = history_with(&[enter_head, enter_tail]);
        assert!(snapshot(&history).1, "split enter sequence must be seen");

        let (leave_head, leave_tail) = LEAVE_ALTERNATE_SCREEN.split_at(5);
        let (history, _) = history_with(&["\x1b[?1049h tui", leave_head, leave_tail]);
        assert!(!snapshot(&history).1, "split leave sequence must be seen");
    }

    #[test]
    fn history_trims_to_the_cap_at_a_line_break() {
        let filler = format!("{}\n", "x".repeat(1023));
        let over_capacity = HISTORY_CAPACITY / filler.len() + 2;
        let (history, sequence) = history_with(&vec![filler.as_str(); over_capacity]);
        let (data, _) = snapshot(&history);
        assert!(data.len() <= HISTORY_CAPACITY);
        assert!(data.starts_with('x'), "replay starts on a whole line");
        assert_eq!(
            sequence,
            u64::try_from(over_capacity).expect("a small count")
        );
    }

    #[test]
    fn a_tail_with_no_line_break_is_dropped_entirely() {
        let blob = "y".repeat(HISTORY_CAPACITY + 1);
        let (history, _) = history_with(&[blob.as_str()]);
        assert_eq!(snapshot(&history).0, "");
    }

    #[test]
    fn trimming_never_slices_inside_a_multibyte_character() {
        // Overflow the cap so the cut position lands inside the run of
        // multibyte characters - slicing there would panic before the fix.
        let multibyte = "é".repeat(HISTORY_CAPACITY / 2);
        let (history, _) = history_with(&[multibyte.as_str(), multibyte.as_str()]);
        let (data, _) = snapshot(&history);
        assert!(data.len() <= HISTORY_CAPACITY);
    }

    #[test]
    fn a_snapshot_carries_the_live_tail_and_its_sequence() {
        let (history, _) = history_with(&["one\n", "\x1b[?1049htwo"]);
        let snapshot = history.lock().expect("history lock").snapshot();
        assert_eq!(
            serde_json::to_value(&snapshot).expect("serializable"),
            serde_json::json!({ "data": "one\n\x1b[?1049htwo", "seq": 2, "alternate": true })
        );
    }

    #[test]
    fn drain_decoded_reassembles_a_character_split_across_reads() {
        let bytes = "caf\u{e9} au lait".as_bytes();
        let (head, tail) = bytes.split_at(4); // splits the two-byte é
        let mut pending = Vec::new();

        pending.extend_from_slice(head);
        let first = drain_decoded(&mut pending);
        assert_eq!(first, "caf");
        assert!(!pending.is_empty(), "partial character is held back");

        pending.extend_from_slice(tail);
        let second = drain_decoded(&mut pending);
        assert_eq!(second, "\u{e9} au lait");
        assert_empty(&pending);
    }

    #[test]
    fn drain_decoded_replaces_genuinely_invalid_bytes() {
        let mut pending = vec![b'o', b'k', 0xFF, b'!'];
        assert_eq!(drain_decoded(&mut pending), "ok\u{FFFD}!");
        assert_empty(&pending);
    }

    #[test]
    fn strip_ansi_drops_control_and_operating_system_sequences() {
        let painted =
            "\x1b[1;32mgreen\x1b[0m\r\n\x1b]0;title\x07tab\there\x1b]8;;url\x1b\\link\x1bMx\x01";
        assert_eq!(strip_ansi(painted), "green\ntab\therelinkx");
    }

    #[test]
    fn strip_ansi_tolerates_a_sequence_cut_off_at_the_end() {
        assert_eq!(strip_ansi("text\x1b"), "text");
        assert_eq!(strip_ansi("text\x1b[12"), "text");
        assert_eq!(strip_ansi("text\x1b]0;unterminated"), "text");
    }

    #[test]
    fn transcript_trims_on_a_char_boundary() {
        let transcript = Mutex::new(String::new());
        let multibyte = "日".repeat(TRANSCRIPT_CAPACITY / 3 + 10);
        append_transcript(&transcript, &multibyte);
        let tail = transcript.lock().expect("transcript lock").clone();
        assert!(tail.len() <= TRANSCRIPT_CAPACITY);
        assert!(tail.chars().all(|ch| ch == '日'));
    }
}
