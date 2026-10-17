//! A short plain-language summary of what a live agent session has done and is
//! doing now, for the agents dashboard.
//!
//! It reads the same rolling PTY transcript session naming reads, collapsed to
//! its distinct lines, and asks the same sources in the same order: the
//! anonymous Copilot guest chat first, then the session's own agent CLI.

use crate::agents::naming::{readable_transcript, run_agent_reply, tail};
use crate::agents::session_context::session_links;
use crate::agents::{copilot, oneshot_invocation, program};

/// Below this much readable conversation there is nothing worth summarizing.
const MINIMUM_SUMMARY_INPUT_LENGTH: usize = 80;
/// How much of the transcript tail a summary prompt carries - more than naming,
/// since a summary covers what was done, not just the topic.
const SUMMARY_PROMPT_TAIL_BYTES: usize = 6000;
/// A reply is cut to this many characters (Markdown included), so one chatty
/// answer can't flood a dashboard card's two lines.
const MAXIMUM_SUMMARY_LENGTH: usize = 160;
/// Where a sentence ends - a reply cut short ends on the last whole one.
const SENTENCE_END: &str = ". ";
/// A reply shorter than this is no summary.
const MINIMUM_SUMMARY_LENGTH: usize = 8;
const ELLIPSIS: char = '…';
/// A list item or heading mark a reply may open with - each followed by a
/// space, so a reply that opens in **bold** keeps its asterisks.
const LINE_MARKERS: &[&str] = &["### ", "## ", "# ", "- ", "* "];

/// Summarize the session `id` from its recent transcript, driven by `agent` (its
/// command). `conversation_id` is the conversation the session is pinned to:
/// the web addresses its own logs show it worked with go to the model, so the
/// summary can link whatever it mentions. `None` until there is enough
/// conversation, or when no source answers. Only the window that owns the
/// session may ask.
#[tauri::command]
pub async fn session_summarize(
    window: tauri::WebviewWindow,
    id: String,
    agent: String,
    conversation_id: Option<String>,
    state: tauri::State<'_, crate::terminal::pty::PtyState>,
) -> Result<Option<String>, String> {
    let transcript = crate::terminal::pty::transcript_of(&state, window.label(), &id)?;
    Ok(tauri::async_runtime::spawn_blocking(move || {
        let links = conversation_id
            .as_deref()
            .map(session_links)
            .unwrap_or_default();
        session_summary(&transcript, &agent, &links)
    })
    .await
    .ok()
    .flatten())
}

fn session_summary(transcript: &str, agent: &str, links: &[String]) -> Option<String> {
    let readable = readable_transcript(transcript);
    if readable.len() < MINIMUM_SUMMARY_INPUT_LENGTH {
        return None;
    }

    let prompt = summary_prompt(&readable, links);
    copilot::run_chat(&prompt)
        .and_then(|raw| tidy_summary(&raw))
        .or_else(|| {
            let arguments = oneshot_invocation(agent)?;
            let executable = program(agent)?;
            run_agent_reply(&executable, arguments, None, prompt).and_then(|raw| tidy_summary(&raw))
        })
}

fn summary_prompt(transcript: &str, links: &[String]) -> String {
    let mut prompt = String::from(
        "Below is the recent terminal transcript of a coding-agent session. Ignore the terminal \
         interface itself - spinners, progress indicators, status bars, key hints, token counts, \
         borders. In ONE short sentence of at most 15 words, say what the agent has done and \
         what it is doing now - or, if it has stopped, whether it finished or is waiting on the \
         user and for what. Write for someone glancing at a dashboard, where it must fit two \
         short lines - leave out detail rather than run long. You may use inline Markdown: \
         **bold** for the one key point, `code` for a file, command or identifier, and a link \
         always as Markdown with a few words of text - [PR #1278](https://...) - never a bare \
         address. No headings, lists or line breaks.",
    );
    prompt.push_str(&links_note(links));
    prompt.push_str("\n\n---\n");
    prompt.push_str(tail(transcript, SUMMARY_PROMPT_TAIL_BYTES));
    prompt.push_str("\n---\n\nReply with ONLY the summary.");
    prompt
}

/// How many of the session's links the prompt carries, newest first.
const PROMPT_LINKS: usize = 15;

/// What the prompt tells the model about links: anything the summary mentions
/// that one of the session's own addresses points to is linked with that exact
/// address - whatever it is - and no address is ever made up.
fn links_note(links: &[String]) -> String {
    let rule = " Only link an address that appears in the transcript or in the list below, \
         copied exactly - never build or guess one.";
    if links.is_empty() {
        return rule.to_string();
    }

    let mut note = format!(
        "{rule} Addresses this session has worked with, newest first - whenever the summary \
         mentions something one of them points to (a pull request, an issue, a run, a page), \
         link it as [a few words](that exact address) - never print the address itself:"
    );
    for link in links.iter().take(PROMPT_LINKS) {
        note.push_str("\n- ");
        note.push_str(link);
    }
    note
}

/// Shape a raw reply into one dashboard line: whitespace collapsed, a wrapping
/// pair of quotes and a leading list or heading mark dropped (inline Markdown -
/// bold, code, links - kept, the dashboard renders it), cut at
/// [`MAXIMUM_SUMMARY_LENGTH`] characters. `None` when nothing usable is left.
fn tidy_summary(raw: &str) -> Option<String> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let unmarked = LINE_MARKERS
        .iter()
        .find_map(|marker| collapsed.strip_prefix(marker))
        .unwrap_or(&collapsed);
    let trimmed = unmarked
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(unmarked)
        .trim();
    if trimmed.chars().count() < MINIMUM_SUMMARY_LENGTH {
        return None;
    }

    let Some(cut_at) = after_visible_characters(trimmed, MAXIMUM_SUMMARY_LENGTH - 1) else {
        return Some(trimmed.to_string());
    };

    // Over-long: keep the whole sentences that fit, rather than stopping
    // mid-thought; only one long sentence is cut with an ellipsis.
    let cut = &trimmed[..cut_at];
    if let Some(end) = cut.rfind(SENTENCE_END) {
        return Some(cut[..=end].to_string());
    }
    Some(format!("{}{ELLIPSIS}", cut.trim_end()))
}

/// The byte index in `text` just after its first `budget` visible characters -
/// a Markdown link's `(address)` never shows, so it never counts. `None` when
/// the text shows no more than that.
fn after_visible_characters(text: &str, budget: usize) -> Option<usize> {
    let mut shown = 0;
    let mut is_in_address = false;
    let mut previous = None;
    for (index, character) in text.char_indices() {
        let opens_address = character == '(' && previous == Some(']');
        previous = Some(character);
        if is_in_address {
            is_in_address = character != ')';
            continue;
        }
        if opens_address {
            is_in_address = true;
            continue;
        }
        if shown == budget {
            return Some(index);
        }
        shown += 1;
    }
    None
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
    fn a_links_address_does_not_count_toward_the_length() {
        let address = format!("https://github.com/a/b/pull/1278?{}", "x".repeat(200));
        let reply = format!("Relaunched the app and answered with the [PR link]({address}).");
        assert_eq!(tidy_summary(&reply).as_deref(), Some(reply.as_str()));
    }

    #[test]
    fn an_over_long_reply_keeps_its_whole_sentences() {
        let reply = format!("Fixed the toast. {}", "Then it kept going ".repeat(20));
        assert_eq!(tidy_summary(&reply).as_deref(), Some("Fixed the toast."));
    }

    #[test]
    fn inline_markdown_survives_the_tidy() {
        let reply = "**Done**: merged [PR #12](https://github.com/a/b/pull/12) after `pnpm test`";
        assert_eq!(tidy_summary(reply).as_deref(), Some(reply));
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
        let prompt = summary_prompt(&transcript, &[]);
        assert!(prompt.contains("LATEST"));
        assert!(prompt.len() < 7_000);
    }

    #[test]
    fn the_prompt_hands_the_model_the_sessions_links_to_use() {
        let links = vec![
            "https://github.com/a/b/pull/1278".to_string(),
            "https://ci.example.com/run/9".to_string(),
        ];
        let prompt = summary_prompt("transcript", &links);
        assert!(prompt.contains(
            "
- https://github.com/a/b/pull/1278
- https://ci.example.com/run/9"
        ));
        assert!(prompt.contains("never build or guess one"));
        assert!(!summary_prompt("transcript", &[]).contains("newest first"));
    }
}
