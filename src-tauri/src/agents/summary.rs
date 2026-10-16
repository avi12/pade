//! A short plain-language summary of what a live agent session has done and is
//! doing now, for the agents dashboard.
//!
//! It reads the same rolling PTY transcript session naming reads, collapsed to
//! its distinct lines, and asks the same sources in the same order: the
//! anonymous Copilot guest chat first, then the session's own agent CLI.

use crate::agents::naming::{readable_transcript, run_agent_reply, tail};
use crate::agents::{copilot, oneshot_invocation, program};

/// Below this much readable conversation there is nothing worth summarizing.
const MINIMUM_SUMMARY_INPUT_LENGTH: usize = 80;
/// How much of the transcript tail a summary prompt carries - more than naming,
/// since a summary covers what was done, not just the topic.
const SUMMARY_PROMPT_TAIL_BYTES: usize = 6000;
/// A reply is cut to this many characters, so one chatty answer can't flood a
/// dashboard card.
const MAXIMUM_SUMMARY_LENGTH: usize = 280;
/// A reply shorter than this is no summary.
const MINIMUM_SUMMARY_LENGTH: usize = 8;
const ELLIPSIS: char = '…';

/// Summarize the session `id` from its recent transcript, driven by `agent` (its
/// command). `None` until there is enough conversation, or when no source
/// answers. Only the window that owns the session may ask.
#[tauri::command]
pub async fn session_summarize(
    window: tauri::WebviewWindow,
    id: String,
    agent: String,
    state: tauri::State<'_, crate::terminal::pty::PtyState>,
) -> Result<Option<String>, String> {
    let transcript = crate::terminal::pty::transcript_of(&state, window.label(), &id)?;
    Ok(
        tauri::async_runtime::spawn_blocking(move || session_summary(&transcript, &agent))
            .await
            .ok()
            .flatten(),
    )
}

fn session_summary(transcript: &str, agent: &str) -> Option<String> {
    let readable = readable_transcript(transcript);
    if readable.len() < MINIMUM_SUMMARY_INPUT_LENGTH {
        return None;
    }

    let prompt = summary_prompt(&readable);
    copilot::run_chat(&prompt)
        .and_then(|raw| tidy_summary(&raw))
        .or_else(|| {
            let arguments = oneshot_invocation(agent)?;
            let executable = program(agent)?;
            run_agent_reply(&executable, arguments, None, prompt).and_then(|raw| tidy_summary(&raw))
        })
}

fn summary_prompt(transcript: &str) -> String {
    let mut prompt = String::from(
        "Below is the recent terminal transcript of a coding-agent session. Ignore the terminal \
         interface itself - spinners, progress indicators, status bars, key hints, token counts, \
         borders. In one or two short, plain sentences, say what the agent has done so far and \
         what it is doing now - or, if it has stopped, whether it finished or is waiting on the \
         user and for what. Write for someone glancing at a dashboard: no file paths unless they \
         matter, no markdown.\n\n---\n",
    );
    prompt.push_str(tail(transcript, SUMMARY_PROMPT_TAIL_BYTES));
    prompt.push_str("\n---\n\nReply with ONLY the summary.");
    prompt
}

/// Shape a raw reply into one dashboard line: whitespace collapsed, wrapping
/// quotes and leading list or heading marks dropped, cut at
/// [`MAXIMUM_SUMMARY_LENGTH`] characters. `None` when nothing usable is left.
fn tidy_summary(raw: &str) -> Option<String> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed
        .trim_start_matches(['-', '*', '#', ' '])
        .trim_matches(['"', '\'', '`'])
        .trim();
    if trimmed.chars().count() < MINIMUM_SUMMARY_LENGTH {
        return None;
    }

    if trimmed.chars().count() <= MAXIMUM_SUMMARY_LENGTH {
        return Some(trimmed.to_string());
    }

    let cut: String = trimmed.chars().take(MAXIMUM_SUMMARY_LENGTH - 1).collect();
    Some(format!("{}{ELLIPSIS}", cut.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::{summary_prompt, tidy_summary, MAXIMUM_SUMMARY_LENGTH};

    #[test]
    fn a_reply_becomes_one_line_without_its_wrapping() {
        assert_eq!(
            tidy_summary("  \"Fixed the toast wording.\n\nNow running the tests.\"  ").as_deref(),
            Some("Fixed the toast wording. Now running the tests.")
        );
        assert_eq!(
            tidy_summary("- Added the dashboard window").as_deref(),
            Some("Added the dashboard window")
        );
    }

    #[test]
    fn a_long_reply_is_cut_with_an_ellipsis() {
        let summary = tidy_summary(&"word ".repeat(200)).unwrap_or_default();
        assert_eq!(summary.chars().count(), MAXIMUM_SUMMARY_LENGTH);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn an_empty_reply_is_no_summary() {
        assert_eq!(tidy_summary("  ok "), None);
    }

    #[test]
    fn the_prompt_carries_only_the_transcript_tail() {
        let transcript = format!("{}LATEST", "x".repeat(10_000));
        let prompt = summary_prompt(&transcript);
        assert!(prompt.contains("LATEST"));
        assert!(prompt.len() < 7_000);
    }
}
