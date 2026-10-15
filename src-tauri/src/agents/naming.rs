//! Auto-naming for temp workspaces.
//!
//! A throwaway workspace starts life as `temp-<stamp>`. Once the agent has done
//! real work in it, ADE derives a short, human-readable name and stores it as a
//! *display label* - never a disk rename: the live agent process holds the temp
//! dir as its cwd, which Windows locks against rename. The label surfaces in the
//! topbar and the Recent list.
//!
//! Naming is layered and swappable behind the `Namer` trait. Session naming tries
//! the anonymous Copilot guest chat first, then the session's own agent CLI;
//! project naming (from a file list) uses the agent CLI then a local heuristic:
//!   1. anonymous Copilot (`copilot.rs`) - a `copilot.microsoft.com` guest chat,
//!      no account, cross-platform; session naming only, best-effort (falls
//!      through when offline / region- or Turnstile-gated);
//!   2. the installed agent CLI, one-shot headless (`claude -p …`) - reuses the
//!      user's subscription, no extra auth;
//!   3. a local heuristic (package/Cargo name, README heading, dominant file) -
//!      the always-on fallback for project naming.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::agents::{oneshot_invocation, program};
use crate::util::is_noise_directory;

/// What a namer works from: the workspace's files (relative, `/`-joined) and an
/// optional first task prompt.
struct NameContext {
    files: Vec<String>,
    prompt: Option<String>,
}

/// One name source. Returns a *raw* candidate; the orchestrator sanitizes it.
trait Namer {
    fn suggest(&self, context: &NameContext) -> Option<String>;
}

/// Suggest a name for the workspace at `path`, driven by `agent` (its command).
/// Read-only; returns `None` when nothing sensible can be derived. Runs the
/// (possibly slow) CLI call off the UI thread.
#[tauri::command]
pub async fn project_autoname(path: String, agent: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || autoname(&path, &agent))
        .await
        .ok()
        .flatten()
}

/// Suggest a short display name for a live agent *session* from its recent
/// terminal transcript. Copilot (Windows) first when wired, else the agent CLI
/// one-shot. Returns `None` until there is enough conversation to be meaningful.
/// Reads the transcript from the PTY layer so the caller only passes the id.
#[tauri::command]
pub async fn session_generate_name(
    window: tauri::WebviewWindow,
    id: String,
    agent: String,
    state: tauri::State<'_, crate::pty::PtyState>,
) -> Result<Option<String>, String> {
    let transcript = crate::pty::transcript_of(&state, window.label(), &id)?;
    Ok(
        tauri::async_runtime::spawn_blocking(move || session_name(&transcript, &agent))
            .await
            .ok()
            .flatten(),
    )
}

/// Minimum transcript length before a session name is worth generating.
const MINIMUM_SESSION_NAME_INPUT_LENGTH: usize = 40;

/// How long to let a name source run before giving up - one home for the timeout
/// shared by the agent-CLI invocations and the Copilot chat namer.
pub(crate) const NAME_TIMEOUT: Duration = Duration::from_secs(30);

/// How much of a session's transcript tail a naming prompt carries.
const SESSION_PROMPT_TAIL_BYTES: usize = 4000;
/// How many of a workspace's files a project-naming prompt lists.
const PROMPT_FILE_COUNT: usize = 12;
/// The bounds a name is held to: at most this many words and characters, at
/// least this many characters.
const MAXIMUM_NAME_WORDS: usize = 4;
const MAXIMUM_NAME_LENGTH: usize = 40;
const MINIMUM_NAME_LENGTH: usize = 2;
/// How long a cleanly exited agent's reply may take to finish draining. A
/// descendant that outlived the agent can hold the pipe open; the reply is then
/// abandoned rather than waited on forever.
const REPLY_DRAIN_GRACE: Duration = Duration::from_secs(1);
/// The closing instruction every naming prompt ends with.
const REPLY_INSTRUCTION: &str = "Reply with ONLY the name - no quotes, no explanation.";

fn session_name(transcript: &str, agent: &str) -> Option<String> {
    // A fullscreen-TUI agent (Claude, Codex, opencode) repaints its ENTIRE
    // screen on every tick, so the raw tail is one frame - status bar, spinner,
    // key hints - stamped dozens of times, drowning the actual conversation. That
    // is how a job-search session got named "terminal-loading-animation": the
    // namer saw mostly the loading chrome. Collapse to distinct content lines
    // first so the namer reads what was SAID, not one frame's UI amplified.
    let readable = readable_transcript(transcript);
    if readable.len() < MINIMUM_SESSION_NAME_INPUT_LENGTH {
        return None;
    }

    // Copilot first (anonymous copilot.microsoft.com guest chat - no account,
    // no local CLI, cross-platform), then the session's own agent CLI. Copilot
    // yields `None` whenever it's unavailable - offline, region-gated, or the
    // Cloudflare Turnstile gate on the first message - so naming quietly falls
    // back to `claude -p` / `codex exec` and never blocks on it.
    if let Some(name) =
        crate::agents::copilot::suggest_session_name(&readable).and_then(|raw| sanitize(&raw))
    {
        return Some(name);
    }

    session_name_via_agent(agent, &readable)
}

/// Collapse a fullscreen-TUI transcript to its distinct content lines, in
/// first-seen order. An alt-screen agent repaints the whole screen each tick, so
/// the raw stream is the same frame repeated - keeping each trimmed line once
/// removes that repaint amplification (the static chrome and the spinner appear
/// a single time) and leaves the variety of what was actually said.
fn readable_transcript(transcript: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut lines = Vec::new();
    for line in transcript.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed) {
            lines.push(trimmed);
        }
    }
    lines.join("\n")
}

/// The trailing `maximum_length` bytes of `text`, snapped to a char boundary - most recent
/// context matters most and keeps the prompt bounded.
fn tail(text: &str, maximum_length: usize) -> &str {
    if text.len() <= maximum_length {
        return text;
    }
    let mut start = text.len() - maximum_length;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

pub(crate) fn session_naming_prompt(transcript: &str) -> String {
    let mut prompt = String::from(
        "Below is the recent terminal transcript of a coding-agent session. Ignore the terminal \
         interface itself - spinners, loading/progress indicators, status bars, key hints, token \
         counts, borders. Name the underlying task or topic the user and agent are working on. \
         Suggest a concise, descriptive name in kebab-case (2-4 lowercase words joined by hyphens) \
         capturing what the session is about.\n\n---\n",
    );
    prompt.push_str(tail(transcript, SESSION_PROMPT_TAIL_BYTES));
    prompt.push_str("\n---\n\n");
    prompt.push_str(REPLY_INSTRUCTION);
    prompt
}

fn session_name_via_agent(agent: &str, transcript: &str) -> Option<String> {
    let arguments = oneshot_invocation(agent)?;
    let executable = program(agent)?;
    run_agent_prompt(
        &executable,
        arguments,
        None,
        session_naming_prompt(transcript),
    )
    .and_then(|raw| sanitize(&raw))
}

fn autoname(path: &str, agent: &str) -> Option<String> {
    // Only ever name ADE's own workspaces - never walk a real project's tree.
    if !crate::workspace::is_owned(path) {
        return None;
    }
    let directory = Path::new(path);
    let files = gather_files(directory);
    if files.is_empty() {
        return None;
    }
    let context = NameContext {
        files,
        prompt: None,
    };

    // Layered sources: agent CLI first, then the heuristic. Copilot names only
    // live sessions (it reads a conversation, not a file list).
    let mut namers: Vec<Box<dyn Namer>> = Vec::new();
    if let Some((arguments, executable)) = oneshot_invocation(agent).zip(program(agent)) {
        namers.push(Box::new(AgentCliNamer {
            command: executable,
            arguments,
            cwd: directory.to_path_buf(),
        }));
    }
    namers.push(Box::new(HeuristicNamer {
        directory: directory.to_path_buf(),
    }));

    // First namer whose candidate survives sanitizing wins (lazy - later sources
    // only run if earlier ones yield nothing usable).
    namers
        .iter()
        .filter_map(|namer| namer.suggest(&context))
        .find_map(|raw| sanitize(&raw))
}

/// Collect up to a bounded set of the workspace's files as relative paths,
/// skipping dotfiles and build/dep noise.
fn gather_files(directory: &Path) -> Vec<String> {
    const MAXIMUM_FILES: usize = 40;
    const MAXIMUM_DEPTH: u8 = 2;
    let mut files = Vec::new();
    let mut stack = vec![(directory.to_path_buf(), 0u8)];
    while let Some((current_directory, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current_directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if files.len() >= MAXIMUM_FILES {
                return files;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if is_noise_directory(&name) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                if depth < MAXIMUM_DEPTH {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if let Ok(relative_path) = path.strip_prefix(directory) {
                files.push(relative_path.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    files
}

// ── Agent-CLI namer ─────────────────────────────────────────────────────────

/// Ask the installed agent to name the project via its one-shot headless mode.
struct AgentCliNamer {
    /// The resolved executable - see `agents::program`.
    command: PathBuf,
    arguments: &'static [&'static str],
    cwd: PathBuf,
}

impl Namer for AgentCliNamer {
    fn suggest(&self, context: &NameContext) -> Option<String> {
        run_agent_prompt(
            &self.command,
            self.arguments,
            Some(&self.cwd),
            naming_prompt(context),
        )
    }
}

fn naming_prompt(context: &NameContext) -> String {
    let list = context
        .files
        .iter()
        .take(PROMPT_FILE_COUNT)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    let mut prompt = String::from(
        "Suggest a concise, descriptive project name in kebab-case (2-4 lowercase \
         words joined by hyphens) for a codebase containing these files:\n",
    );
    prompt.push_str(&list);
    if let Some(task) = &context.prompt {
        prompt.push_str("\n\nInitial task: ");
        prompt.push_str(task);
    }
    prompt.push_str("\n\n");
    prompt.push_str(REPLY_INSTRUCTION);
    prompt
}

/// Pull the name out of a CLI reply: prefer a line that is already a bare token,
/// else the last non-empty line (models tend to conclude with the answer).
fn extract_name(output: &str) -> Option<String> {
    let bare = output.lines().map(str::trim).find(|line| {
        !line.is_empty()
            && line.len() <= MAXIMUM_NAME_LENGTH
            && line.chars().all(|character| {
                character.is_ascii_alphanumeric() || character == '-' || character == '_'
            })
    });
    bare.map(str::to_string).or_else(|| {
        output
            .lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_string)
    })
}

/// Run an agent CLI headless: `command arguments… prompt` (optionally in `cwd`),
/// capture its reply within [`NAME_TIMEOUT`], and pull a raw name candidate out.
/// Resolving the executable (`agents::program`) and sanitizing the result are the
/// caller's job - this is the shared invoke-and-extract sequence (DRY).
fn run_agent_prompt(
    command: &Path,
    arguments: &[&str],
    cwd: Option<&Path>,
    prompt: String,
) -> Option<String> {
    let mut process = crate::util::command(command);
    if let Some(directory) = cwd {
        process.current_dir(directory);
    }
    process.args(arguments).arg(prompt);
    let output = run_capture(process, NAME_TIMEOUT)?;
    extract_name(&output)
}

/// Run `command`, capturing stdout and killing it after `timeout`. stdin is
/// closed so a CLI that expects input gets EOF instead of hanging. Returns
/// stdout on a clean exit, else `None`.
///
/// stdout drains on its own thread while the child runs: a child blocked on a
/// full pipe never exits, so reading only after the exit would stall any reply
/// bigger than the pipe buffer until the timeout killed it.
fn run_capture(mut command: Command, timeout: Duration) -> Option<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };

    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reply = String::new();
        let read = stdout.read_to_string(&mut reply).map(|_| reply);
        let _ = sender.send(read);
    });

    let has_succeeded = crate::util::exits_successfully_within(&mut child, timeout);
    if !has_succeeded {
        return None;
    }
    receiver.recv_timeout(REPLY_DRAIN_GRACE).ok()?.ok()
}

// ── Heuristic namer ─────────────────────────────────────────────────────────

/// Offline fallback: derive a name from the project's own metadata or files.
struct HeuristicNamer {
    directory: PathBuf,
}

impl Namer for HeuristicNamer {
    fn suggest(&self, context: &NameContext) -> Option<String> {
        package_name(&self.directory)
            .or_else(|| cargo_name(&self.directory))
            .or_else(|| readme_title(&self.directory))
            .or_else(|| dominant_stem(&context.files))
    }
}

fn read_file(directory: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(directory.join(name)).ok()
}

fn package_name(directory: &Path) -> Option<String> {
    let text = read_file(directory, "package.json")?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let name = json.get("name")?.as_str()?;
    // Drop an npm scope: "@acme/widget" -> "widget".
    Some(name.rsplit('/').next().unwrap_or(name).to_string())
}

fn cargo_name(directory: &Path) -> Option<String> {
    let text = read_file(directory, "Cargo.toml")?;
    // Light scan - the first `name = "…"` (the [package] name) is enough here.
    text.lines()
        .map(str::trim)
        .find_map(|line| {
            let rest = line.strip_prefix("name")?.trim_start();
            Some(rest.strip_prefix('=')?.trim().trim_matches('"').to_string())
        })
        .filter(|name| !name.is_empty())
}

fn readme_title(directory: &Path) -> Option<String> {
    for candidate in ["README.md", "readme.md", "Readme.md", "README"] {
        let Some(text) = read_file(directory, candidate) else {
            continue;
        };
        let title = text
            .lines()
            .map(str::trim)
            .find_map(|line| line.strip_prefix("# "));
        if let Some(title) = title {
            return Some(title.to_string());
        }
    }
    None
}

fn dominant_stem(files: &[String]) -> Option<String> {
    use std::collections::HashMap;
    const NOISE: &[&str] = &[
        "index", "main", "mod", "lib", "app", "readme", "license", "makefile",
    ];
    let mut counts: HashMap<String, usize> = HashMap::new();
    for file in files {
        let stem = Path::new(file)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        if stem.is_empty() || NOISE.contains(&stem.as_str()) {
            continue;
        }
        *counts.entry(stem).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(stem, _)| stem)
}

// ── Sanitizer (shared) ──────────────────────────────────────────────────────

/// Normalize a raw candidate to a safe, short kebab-case label. `None` if nothing
/// usable survives. Shared with `workspace_set_label`, which runs every auto-name
/// through it (a label the user types in PADE is kept as typed instead).
pub(crate) fn sanitize(raw: &str) -> Option<String> {
    let first = raw.trim().lines().next().unwrap_or("").to_lowercase();
    let mut kebab = String::with_capacity(first.len());
    for character in first.chars() {
        if character.is_ascii_alphanumeric() {
            kebab.push(character);
        } else if !kebab.ends_with('-') {
            kebab.push('-');
        }
    }
    // Cap the word count, then the length (all-ASCII by now, so byte == char).
    let name = kebab
        .split('-')
        .filter(|part| !part.is_empty())
        .take(MAXIMUM_NAME_WORDS)
        .collect::<Vec<_>>()
        .join("-");
    let name = name
        .get(..name.len().min(MAXIMUM_NAME_LENGTH))
        .unwrap_or(&name)
        .trim_end_matches('-')
        .to_string();
    (name.len() >= MINIMUM_NAME_LENGTH).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::{dominant_stem, extract_name, run_capture, sanitize};
    use std::time::{Duration, Instant};

    /// A reply bigger than any OS pipe buffer (64 KB on Windows and Linux).
    const OVERSIZED_REPLY_BYTES: usize = 128 * 1024;
    const CAPTURE_TIMEOUT: Duration = Duration::from_secs(20);

    /// A shell line printing more than [`OVERSIZED_REPLY_BYTES`] of text.
    fn oversized_reply_command() -> String {
        if cfg!(windows) {
            "for /L %i in (1,1,2500) do @echo 0123456789012345678901234567890123456789012345678901234567890123"
                .to_string()
        } else {
            format!("yes 0123456789 | head -c {}", OVERSIZED_REPLY_BYTES + 1)
        }
    }

    /// The reader used to start only after the exit, so a reply that filled the
    /// pipe blocked the child until the timeout killed it and nothing came back.
    #[test]
    fn a_reply_bigger_than_the_pipe_buffer_is_captured_without_waiting_out_the_timeout() {
        let started = Instant::now();
        let reply = run_capture(
            crate::util::shell_command(&oversized_reply_command()),
            CAPTURE_TIMEOUT,
        );
        let reply = reply.expect("a clean exit returns its whole reply");
        assert!(reply.len() > OVERSIZED_REPLY_BYTES);
        assert!(started.elapsed() < CAPTURE_TIMEOUT);
    }

    #[test]
    fn sanitize_kebabs_a_titlecase_phrase() {
        assert_eq!(sanitize("Brave Otter!").as_deref(), Some("brave-otter"));
    }

    #[test]
    fn sanitize_keeps_an_already_kebab_name() {
        assert_eq!(sanitize("brave-otter").as_deref(), Some("brave-otter"));
    }

    #[test]
    fn sanitize_uses_only_the_first_line() {
        assert_eq!(
            sanitize("tidy-name\nwith an explanation").as_deref(),
            Some("tidy-name")
        );
    }

    #[test]
    fn sanitize_caps_at_four_words() {
        assert_eq!(
            sanitize("one two three four five").as_deref(),
            Some("one-two-three-four")
        );
    }

    #[test]
    fn sanitize_collapses_separator_runs() {
        assert_eq!(sanitize("hello___world!!").as_deref(), Some("hello-world"));
    }

    #[test]
    fn sanitize_caps_the_length_at_forty() {
        assert_eq!(sanitize(&"a".repeat(60)), Some("a".repeat(40)));
    }

    #[test]
    fn sanitize_rejects_candidates_too_short_to_mean_anything() {
        assert_eq!(sanitize("x"), None);
        assert_eq!(sanitize("!!!"), None);
        assert_eq!(sanitize(""), None);
    }

    #[test]
    fn extract_name_prefers_a_bare_token_line() {
        assert_eq!(
            extract_name("Sure! A good name would be:\n\nbrave-otter\n").as_deref(),
            Some("brave-otter")
        );
    }

    #[test]
    fn extract_name_falls_back_to_the_last_nonempty_line() {
        assert_eq!(
            extract_name("The best name is:\nMy Cool Project\n").as_deref(),
            Some("My Cool Project")
        );
    }

    #[test]
    fn extract_name_yields_nothing_for_blank_output() {
        assert_eq!(extract_name("\n  \n"), None);
    }

    #[test]
    fn readable_transcript_collapses_repeated_repaint_frames() {
        // A fullscreen agent repaints the same frame each spinner tick; only the
        // substantive line should survive once, chrome deduped to a single copy.
        let painted =
            "⠋ Working\nSearch Tel Aviv startups\n⠙ Working\nSearch Tel Aviv startups\n⠹ Working";
        assert_eq!(
            super::readable_transcript(painted),
            "⠋ Working\nSearch Tel Aviv startups\n⠙ Working\n⠹ Working"
        );
    }

    #[test]
    fn dominant_stem_picks_the_most_frequent_meaningful_stem() {
        let files = ["widget.rs", "widget.toml", "readme.md", "index.ts"].map(str::to_string);
        assert_eq!(dominant_stem(&files).as_deref(), Some("widget"));
    }

    #[test]
    fn dominant_stem_ignores_noise_only_file_sets() {
        let files = ["index.ts", "main.rs", "readme.md"].map(str::to_string);
        assert_eq!(dominant_stem(&files), None);
    }
}
