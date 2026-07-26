//! Auto-naming for temp workspaces.
//!
//! A throwaway workspace starts life as `temp-<stamp>`. Once the agent has done
//! real work in it, ADE derives a short, human-readable name and stores it as a
//! *display label* — never a disk rename: the live agent process holds the temp
//! dir as its cwd, which Windows locks against rename. The label surfaces in the
//! topbar and the Recent list.
//!
//! Naming is layered and swappable behind the `Namer` trait:
//!   1. the installed agent CLI, one-shot headless (`claude -p …`) — the primary,
//!      cross-platform path; reuses the user's subscription, no extra auth;
//!   2. Copilot on Windows (`copilot.rs`) — optional, currently a stub;
//!   3. a local heuristic (package/Cargo name, README heading, dominant file) —
//!      the always-on fallback.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::agents::{oneshot_invocation, program};

/// What a namer works from: the workspace's files (relative, `/`-joined) and an
/// optional first task prompt.
pub(crate) struct NameContext {
    pub files: Vec<String>,
    pub prompt: Option<String>,
}

/// One name source. Returns a *raw* candidate; the orchestrator sanitizes it.
pub(crate) trait Namer {
    fn suggest(&self, context: &NameContext) -> Option<String>;
}

/// Directories never worth scanning for a name (build output, VCS, deps).
const SKIPPED_DIRECTORIES: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".git",
    ".svelte-kit",
    ".vite",
];

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

/// How long to let an agent CLI run before giving up on a name — one home for the
/// timeout shared by the project-namer and session-namer invocations.
const NAME_TIMEOUT: Duration = Duration::from_secs(30);

fn session_name(transcript: &str, agent: &str) -> Option<String> {
    // A fullscreen-TUI agent (Claude, Codex, opencode) repaints its ENTIRE
    // screen on every tick, so the raw tail is one frame — status bar, spinner,
    // key hints — stamped dozens of times, drowning the actual conversation. That
    // is how a job-search session got named "terminal-loading-animation": the
    // namer saw mostly the loading chrome. Collapse to distinct content lines
    // first so the namer reads what was SAID, not one frame's UI amplified.
    let readable = readable_transcript(transcript);
    if readable.len() < MINIMUM_SESSION_NAME_INPUT_LENGTH {
        return None;
    }

    #[cfg(windows)]
    if let Some(name) = crate::copilot::CopilotNamer
        .suggest_session(&readable)
        .and_then(|raw| sanitize(&raw))
    {
        return Some(name);
    }

    session_name_via_agent(agent, &readable)
}

/// Collapse a fullscreen-TUI transcript to its distinct content lines, in
/// first-seen order. An alt-screen agent repaints the whole screen each tick, so
/// the raw stream is the same frame repeated — keeping each trimmed line once
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

/// The trailing `max` bytes of `text`, snapped to a char boundary — most recent
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

fn session_naming_prompt(transcript: &str) -> String {
    let mut prompt = String::from(
        "Below is the recent terminal transcript of a coding-agent session. Ignore the terminal \
         interface itself — spinners, loading/progress indicators, status bars, key hints, token \
         counts, borders. Name the underlying task or topic the user and agent are working on. \
         Suggest a concise, descriptive name in kebab-case (2-4 lowercase words joined by hyphens) \
         capturing what the session is about.\n\n---\n",
    );
    prompt.push_str(tail(transcript, 4000));
    prompt.push_str("\n---\n\nReply with ONLY the name — no quotes, no explanation.");
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
    // Only ever name ADE's own workspaces — never walk a real project's tree.
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

    // Layered sources: agent CLI first, then (on Windows) Copilot, then heuristic.
    let mut namers: Vec<Box<dyn Namer>> = Vec::new();
    if let Some((arguments, executable)) = oneshot_invocation(agent).zip(program(agent)) {
        namers.push(Box::new(AgentCliNamer {
            command: executable,
            args: arguments,
            cwd: directory.to_path_buf(),
        }));
    }
    #[cfg(windows)]
    namers.push(Box::new(crate::copilot::CopilotNamer));
    namers.push(Box::new(HeuristicNamer {
        directory: directory.to_path_buf(),
    }));

    // First namer whose candidate survives sanitizing wins (lazy — later sources
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
            let is_noise = name.starts_with('.') || SKIPPED_DIRECTORIES.contains(&name.as_ref());
            if is_noise {
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
    /// The resolved executable — see `agents::program`.
    command: PathBuf,
    args: &'static [&'static str],
    cwd: PathBuf,
}

impl Namer for AgentCliNamer {
    fn suggest(&self, context: &NameContext) -> Option<String> {
        run_agent_prompt(
            &self.command,
            self.args,
            Some(&self.cwd),
            naming_prompt(context),
        )
    }
}

fn naming_prompt(context: &NameContext) -> String {
    let list = context
        .files
        .iter()
        .take(12)
        .cloned()
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
    prompt.push_str("\n\nReply with ONLY the name — no quotes, no explanation.");
    prompt
}

/// Pull the name out of a CLI reply: prefer a line that is already a bare token,
/// else the last non-empty line (models tend to conclude with the answer).
fn extract_name(output: &str) -> Option<String> {
    let bare = output.lines().map(str::trim).find(|line| {
        !line.is_empty()
            && line.len() <= 40
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

/// Run an agent CLI headless: `command args… prompt` (optionally in `cwd`),
/// capture its reply within [`NAME_TIMEOUT`], and pull a raw name candidate out.
/// Resolving the executable (`agents::program`) and sanitizing the result are the
/// caller's job — this is the shared invoke-and-extract sequence (DRY).
fn run_agent_prompt(
    command: &Path,
    args: &[&str],
    cwd: Option<&Path>,
    prompt: String,
) -> Option<String> {
    let mut process = crate::util::command(command);
    if let Some(directory) = cwd {
        process.current_dir(directory);
    }
    process.args(args).arg(prompt);
    let output = run_capture(process, NAME_TIMEOUT)?;
    extract_name(&output)
}

/// Run `cmd`, capturing stdout and killing it after `timeout`. stdin is closed so
/// a CLI that expects input gets EOF instead of hanging. Returns stdout on a
/// clean exit, else `None`.
fn run_capture(mut command: Command, timeout: Duration) -> Option<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                break;
            }
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => return None,
        }
    }
    let mut buffer = String::new();
    child.stdout.take()?.read_to_string(&mut buffer).ok()?;
    Some(buffer)
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
    // Light scan — the first `name = "…"` (the [package] name) is enough here.
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
/// usable survives. Shared with `workspace_set_label` so hand-set and derived
/// names go through the same gate.
pub(crate) fn sanitize(raw: &str) -> Option<String> {
    let first = raw.trim().lines().next().unwrap_or("").to_lowercase();
    let mut kebab = String::with_capacity(first.len());
    for ch in first.chars() {
        if ch.is_ascii_alphanumeric() {
            kebab.push(ch);
        } else if !kebab.ends_with('-') {
            kebab.push('-');
        }
    }
    // At most 4 words, then cap length (all-ASCII by now, so byte == char).
    let name = kebab
        .trim_matches('-')
        .split('-')
        .filter(|part| !part.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("-");
    let name = name
        .get(..name.len().min(40))
        .unwrap_or(&name)
        .trim_end_matches('-')
        .to_string();
    (name.len() >= 2).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::{dominant_stem, extract_name, sanitize};

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
