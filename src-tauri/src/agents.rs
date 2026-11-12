//! Agent registry & detection.
//!
//! ADE is agent-agnostic: it discovers which agent CLIs are installed and lets
//! the user launch, switch, and combine them. Adding a backend is one entry in
//! REGISTRY - nothing else in the app hard-codes "claude".
//!
//! The submodules hold everything else that speaks to a running agent: naming
//! a session or project (`naming`, with `copilot` as a name source), reopening
//! its conversation (`resume`), reading its live model and effort
//! (`running_model`), its context fill (`session_context`) and the tokens it
//! and its subagents spent lately (`token_burn`).

pub(crate) mod conversation_watch;
pub(crate) mod conversations;
pub(crate) mod copilot;
mod definitions;
pub(crate) mod keybindings;
pub(crate) mod naming;
pub(crate) mod parsed_files;
pub(crate) mod resume;
pub(crate) mod running_model;
pub(crate) mod session_context;
pub(crate) mod storage;
pub(crate) mod summary;
pub(crate) mod token_burn;

use std::path::PathBuf;

use serde::Serialize;

use crate::terminal::theming::ThemeConfig;
use crate::util::{find_in, search_directories};
use definitions::{AgentDefinition, REGISTRY};

/// Canonical registry ids - the one authoritative spelling of each agent's id
/// (`session.agent.id`). The REGISTRY entries below read from these, as do the
/// other modules that must name an agent (`usage`, `config`), so the vocabulary
/// has a single home and can never drift between them.
pub const ID_CLAUDE: &str = "claude";
pub const ID_CODEX: &str = "codex";
pub const ID_OPENCODE: &str = "opencode";
pub const ID_COPILOT: &str = "copilot";
pub const ID_GROK: &str = "grok";
pub const ID_ANTIGRAVITY: &str = "antigravity";
pub const ID_CURSOR: &str = "cursor";
pub const ID_AIDER: &str = "aider";
pub const ID_MUSE: &str = "muse";
pub const ID_QWEN: &str = "qwen";

/// The variable that relocates Codex's whole home (config, sessions, auth).
const CODEX_HOME_VARIABLE: &str = "CODEX_HOME";

/// Codex's home folder, as Codex itself resolves it: `$CODEX_HOME`, else
/// `~/.codex`. The one lookup every module reading Codex's files goes through,
/// so a relocated home is honoured everywhere or nowhere.
pub(crate) fn codex_home() -> Option<PathBuf> {
    resolve_codex_home(
        std::env::var_os(CODEX_HOME_VARIABLE),
        crate::util::home_directory(),
    )
}

/// `~/.claude/projects`, where Claude Code keeps one folder of transcripts per
/// project (named by `encode_project`). The one lookup every module reading
/// Claude's transcripts goes through.
pub(crate) fn claude_projects_directory() -> Option<PathBuf> {
    Some(claude_directory()?.join("projects"))
}

/// `~/.claude`, Claude Code's per-user folder (settings, keybindings,
/// transcripts) - the one place its location is spelled.
pub(crate) fn claude_directory() -> Option<PathBuf> {
    Some(crate::util::home_directory()?.join(".claude"))
}

/// The resolution rule behind [`codex_home`], over its two inputs.
fn resolve_codex_home(
    configured: Option<std::ffi::OsString>,
    user_home: Option<PathBuf>,
) -> Option<PathBuf> {
    configured
        .map(PathBuf::from)
        .or_else(|| Some(user_home?.join(".codex")))
}

/// How a CLI takes the conversation id it should pin a session to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionIdFlag {
    /// The flag and id joined with `=`: `--resume=<id>` - for a flag whose value
    /// is optional, where a separate id would be read as a prompt.
    Joined(&'static str),
    /// One flag names a new conversation and another resumes a recorded one,
    /// each followed by the id: Claude's `--session-id <id>` refuses an id it
    /// already recorded ("Session ID … is already in use", verified on
    /// 2.1.302), so a respawn of a recorded conversation needs `--resume <id>`.
    CreateOrResume {
        create: &'static str,
        resume: &'static str,
    },
}

/// Whether the agent has already recorded the conversation a session is
/// pinned to - it decides between creating and resuming it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationRecord {
    Unrecorded,
    Recorded,
}

impl SessionIdFlag {
    /// The arguments that pin a session to `conversation_id`.
    pub fn arguments(self, conversation_id: &str, record: ConversationRecord) -> Vec<String> {
        match self {
            Self::Joined(flag) => vec![format!("{flag}={conversation_id}")],
            Self::CreateOrResume { create, resume } => {
                let flag = match record {
                    ConversationRecord::Unrecorded => create,
                    ConversationRecord::Recorded => resume,
                };
                vec![flag.to_string(), conversation_id.to_string()]
            }
        }
    }
}

/// The platform shell a session falls back to when no agent is named - the
/// one spelling both the spawn fallback (`pty.rs`) and the detected
/// "Terminal (shell)" entry read.
pub(crate) const DEFAULT_SHELL: &str = if cfg!(windows) {
    "powershell.exe"
} else {
    "bash"
};
/// The registry id and label of the always-offered shell entry.
const SHELL_ID: &str = "shell";
const SHELL_LABEL: &str = "Terminal (shell)";

/// The registry entry for an executable, if ADE knows it. One lookup (DRY) behind
/// every per-agent question.
fn definition(command: &str) -> Option<&'static AgentDefinition> {
    REGISTRY.iter().find(|agent| agent.command == command)
}

/// The executable the registry launches the agent `id` with (`cursor` runs
/// `cursor-agent`), so a hint telling the user which CLI to run names the real
/// one. `None` for an id the registry doesn't know.
pub(crate) fn command_for_id(id: &str) -> Option<&'static str> {
    REGISTRY
        .iter()
        .find(|agent| agent.id == id)
        .map(|agent| agent.command)
}

/// Every executable name `command` could be installed under: its own name first,
/// then the registry's aliases (just the name itself for a shell or a command ADE
/// doesn't know, e.g. a task runner).
fn installed_names(command: &str) -> Vec<&str> {
    let mut names = vec![command];
    if let Some(definition) = definition(command) {
        names.extend(definition.aliases);
    }
    names
}

/// The executable to actually run for `command`, or `None` if it isn't installed.
///
/// Everything that needs a real program goes through here - detection, spawning a
/// session, running a headless one-shot - so an agent is found and launched by the
/// same rules, and an install ADE can *see* is always one it can *run*. Resolving
/// to an absolute path (rather than handing a bare name to the child process) is
/// what lets a session start from a directory that only the live PATH knows about.
pub fn program(command: &str) -> Option<PathBuf> {
    find_in(&search_directories(), &installed_names(command))
}

/// How to invoke `command` headlessly for a one-shot prompt (auto-naming), if we
/// know a way. Keeps the registry the single source of truth (DRY).
pub fn oneshot_invocation(command: &str) -> Option<&'static [&'static str]> {
    definition(command).and_then(|agent| agent.oneshot)
}

/// Every agent that can answer a headless prompt, `preferred` first when it can,
/// then the rest in registry order - the chain a Copilot failure falls back
/// along, so a session whose own CLI has no headless mode (Copilot CLI, Cursor)
/// still gets an answer from another installed LLM.
pub fn headless_agents(preferred: &str) -> impl Iterator<Item = &'static str> + '_ {
    let headless = || {
        REGISTRY
            .iter()
            .filter(|agent| agent.oneshot.is_some())
            .map(|agent| agent.command)
    };
    headless()
        .filter(move |command| *command == preferred)
        .chain(headless().filter(move |command| *command != preferred))
}

/// Environment variables to set when spawning `command` in a PTY. Empty for an
/// unknown command or a plain shell, so `pty.rs` stays agent-agnostic.
pub fn spawn_environment(command: &str) -> &'static [(&'static str, &'static str)] {
    definition(command).map_or(&[], |agent| agent.environment)
}

/// Args to launch an interactive session of `command` with - the CLI's
/// skip-every-permission ("yolo") flag(s), so ADE runs it autonomously. Empty for
/// an unknown command or a plain shell (which has nothing to bypass), so `pty.rs`
/// stays agent-agnostic.
pub fn session_arguments(command: &str) -> &'static [&'static str] {
    definition(command).map_or(&[], |agent| agent.session_arguments)
}

/// The flag that pins `command`'s session to a caller-chosen conversation id
/// (and resumes that exact conversation on a later spawn with the same id) -
/// e.g. `--session-id` for Claude. `None` for a CLI with no equivalent, in which
/// case ADE can't target a specific conversation and won't try. See
/// `session_id_flag` on the registry entry.
pub fn session_id_flag(command: &str) -> Option<SessionIdFlag> {
    definition(command).and_then(|agent| agent.session_id_flag)
}

/// The arguments that pin `command`'s session to `conversation_id`, creating
/// the conversation or resuming the one already recorded under it. `None` for
/// a CLI with no session-id flag.
pub fn session_id_arguments(command: &str, conversation_id: &str) -> Option<Vec<String>> {
    let flag = session_id_flag(command)?;
    let is_resumable = matches!(flag, SessionIdFlag::CreateOrResume { .. });
    let record = if is_resumable && is_claude_conversation_recorded(conversation_id) {
        ConversationRecord::Recorded
    } else {
        ConversationRecord::Unrecorded
    };
    Some(flag.arguments(conversation_id, record))
}

/// Whether Claude has a transcript for `conversation_id` under any project.
/// Searched by file name rather than by the project's encoded folder, so it
/// holds whatever folder name Claude derived from the working directory.
fn is_claude_conversation_recorded(conversation_id: &str) -> bool {
    let Some(projects) = claude_projects_directory() else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir(projects) else {
        return false;
    };
    let transcript = format!("{conversation_id}.jsonl");
    entries
        .flatten()
        .any(|entry| entry.path().join(&transcript).is_file())
}

/// How `command`'s own UI theme is forced to ADE's scheme, if the registry
/// knows a mechanism (`theming.rs` interprets it). `None` for an unknown
/// command or a CLI with no theme setting.
pub fn theme_config(command: &str) -> Option<&'static ThemeConfig> {
    definition(command).and_then(|agent| agent.theme_config.as_ref())
}

/// Every registered agent's command, in registry order - so a cross-agent sweep
/// (theming's live-theme publish) walks the registry instead of naming agents.
pub fn commands() -> impl Iterator<Item = &'static str> {
    REGISTRY.iter().map(|agent| agent.command)
}

/// Whether `command`'s input composer box follows the *detected* terminal
/// background (rather than a config/env theme), and so needs ADE's Windows
/// light-console workaround. `pty.rs` gates the `cmd /c color F0 & …` prefix on
/// this plus `cfg!(windows)` and a light scheme. `false` for an unknown command
/// or a CLI that themes its box some other way.
pub fn needs_light_console_fix(command: &str) -> bool {
    definition(command).is_some_and(|agent| agent.needs_light_console_fix)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    id: String,
    label: String,
    command: String,
    /// Carried to the frontend because the TERMINAL is what has to act on it -
    /// the right-to-left overlay stands down for an agent that has already
    /// reordered its own output.
    reorders_bidi: bool,
}

/// Every installed agent backend. The shell fallback is appended so the list is
/// never empty (there is always something to launch).
///
/// Async + `spawn_blocking`: detection reads the live PATH and stats a few hundred
/// candidate files, which is slow enough that running it synchronously would block
/// Tauri's main thread - and the main thread also drives Windows' window move loop,
/// so a sync detect fired on window-focus stalls dragging. Off-thread it can't.
#[tauri::command]
pub async fn agents_detect() -> Vec<Agent> {
    tauri::async_runtime::spawn_blocking(detect_installed)
        .await
        .unwrap_or_default()
}

fn detect_installed() -> Vec<Agent> {
    // One search path for the whole sweep - it costs a registry read to build.
    let directories = search_directories();
    let mut found: Vec<Agent> = REGISTRY
        .iter()
        .filter(|agent| find_in(&directories, &installed_names(agent.command)).is_some())
        .map(|agent| Agent {
            id: agent.id.into(),
            label: agent.label.into(),
            command: agent.command.into(),
            reorders_bidi: agent.reorders_bidi,
        })
        .collect();

    found.push(Agent {
        id: SHELL_ID.into(),
        label: SHELL_LABEL.into(),
        command: DEFAULT_SHELL.into(),
        // A shell writes whatever the program it runs writes: logical order.
        reorders_bidi: false,
    });
    found
}

#[cfg(test)]
mod tests;
