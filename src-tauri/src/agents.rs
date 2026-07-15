//! Agent registry & detection.
//!
//! ADE is agent-agnostic: it discovers which agent CLIs are installed and lets
//! the user launch, switch, and combine them. Adding a backend is one entry in
//! REGISTRY — nothing else in the app hard-codes "claude".

use std::path::PathBuf;

use serde::Serialize;

use crate::util::{find_in, search_dirs};

struct AgentDef {
    id: &'static str,
    label: &'static str,
    /// The executable to look for and run — the name the CLI's own docs use, and
    /// the one ADE stores and shows.
    command: &'static str,
    /// Other executable names the same CLI is installed under. Installers don't
    /// agree: npm, Homebrew and cargo all drop a plain `codex`, but winget's
    /// portable package unpacks the vendor's release binary under its raw
    /// target-triple name and never creates a `codex.exe` at all. Without the
    /// alias, a real install is invisible.
    aliases: &'static [&'static str],
    /// Args that make the CLI answer one prompt non-interactively and exit, with
    /// the prompt appended as the final arg (used for auto-naming). `None` = no
    /// headless mode we can drive.
    oneshot: Option<&'static [&'static str]>,
    /// Environment the CLI needs to render the way ADE embeds it. Empty for most.
    env: &'static [(&'static str, &'static str)],
}

/// Known agent backends, in preferred display order. The plain shell is always
/// offered last as a universal fallback.
const REGISTRY: &[AgentDef] = &[
    AgentDef {
        id: "claude",
        label: "Claude Code",
        command: "claude",
        aliases: &[],
        oneshot: Some(&["-p"]),
        // Claude Code's fullscreen renderer: it takes over the terminal's ALTERNATE
        // screen and owns every row of it, which is what buys the polished TUI —
        // flicker-free output, mouse support, selection that copies itself. ADE wants
        // that, and `Terminal.svelte` knows how to host it (a resize there waits for
        // the drag to settle and then moves the grid and the agent together; see
        // docs/terminal-rendering.md).
        //
        // Forced by env, not by the `tui` setting, so it does not depend on — and
        // cannot be undone by — whatever the user's own Claude config happens to say.
        env: &[("CLAUDE_CODE_NO_FLICKER", "1")],
    },
    AgentDef {
        id: "codex",
        label: "Codex",
        command: "codex",
        // OpenAI publishes Codex as bare release binaries named by target triple,
        // and winget's package installs one verbatim — so on a winget machine the
        // only `codex` there is answers to `codex-x86_64-pc-windows-msvc`.
        aliases: &[
            "codex-x86_64-pc-windows-msvc",
            "codex-aarch64-pc-windows-msvc",
            "codex-aarch64-apple-darwin",
            "codex-x86_64-apple-darwin",
            "codex-aarch64-unknown-linux-musl",
            "codex-x86_64-unknown-linux-musl",
        ],
        oneshot: Some(&["exec"]),
        env: &[],
    },
    AgentDef {
        id: "grok",
        label: "Grok CLI",
        command: "grok",
        aliases: &[],
        // xAI's Grok Build answers a single prompt with `-p <PROMPT>`, the same
        // shape as Claude. `--no-auto-update` goes first because a one-shot naming
        // run is exactly the headless, automated case the CLI's own docs say to
        // pass it for — it skips the background update check that would otherwise
        // risk blowing NAME_TIMEOUT before the name comes back.
        oneshot: Some(&["--no-auto-update", "-p"]),
        env: &[],
    },
    AgentDef {
        id: "antigravity",
        label: "Antigravity CLI",
        command: "antigravity",
        aliases: &[],
        oneshot: None,
        env: &[],
    },
    AgentDef {
        id: "cursor",
        label: "Cursor CLI",
        command: "cursor-agent",
        aliases: &[],
        oneshot: None,
        env: &[],
    },
    AgentDef {
        id: "aider",
        label: "aider",
        command: "aider",
        aliases: &[],
        oneshot: None,
        env: &[],
    },
];

/// The registry entry for an executable, if ADE knows it. One lookup (DRY) behind
/// every per-agent question.
fn definition(command: &str) -> Option<&'static AgentDef> {
    REGISTRY.iter().find(|a| a.command == command)
}

/// Every executable name `command` could be installed under: its own name first,
/// then the registry's aliases (just the name itself for a shell or a command ADE
/// doesn't know, e.g. a task runner).
fn installed_names(command: &str) -> Vec<&str> {
    let mut names = vec![command];
    if let Some(def) = definition(command) {
        names.extend(def.aliases);
    }
    names
}

/// The executable to actually run for `command`, or `None` if it isn't installed.
///
/// Everything that needs a real program goes through here — detection, spawning a
/// session, running a headless one-shot — so an agent is found and launched by the
/// same rules, and an install ADE can *see* is always one it can *run*. Resolving
/// to an absolute path (rather than handing a bare name to the child process) is
/// what lets a session start from a directory that only the live PATH knows about.
pub fn program(command: &str) -> Option<PathBuf> {
    find_in(&search_dirs(), &installed_names(command))
}

/// How to invoke `command` headlessly for a one-shot prompt (auto-naming), if we
/// know a way. Keeps the registry the single source of truth (DRY).
pub fn oneshot_invocation(command: &str) -> Option<&'static [&'static str]> {
    definition(command).and_then(|a| a.oneshot)
}

/// Environment variables to set when spawning `command` in a PTY. Empty for an
/// unknown command or a plain shell, so `pty.rs` stays agent-agnostic.
pub fn spawn_env(command: &str) -> &'static [(&'static str, &'static str)] {
    definition(command).map_or(&[], |a| a.env)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    id: String,
    label: String,
    command: String,
}

/// Every installed agent backend. The shell fallback is appended so the list is
/// never empty (there is always something to launch).
///
/// Async + `spawn_blocking`: detection reads the live PATH and stats a few hundred
/// candidate files, which is slow enough that running it synchronously would block
/// Tauri's main thread — and the main thread also drives Windows' window move loop,
/// so a sync detect fired on window-focus stalls dragging. Off-thread it can't.
#[tauri::command]
pub async fn agents_detect() -> Vec<Agent> {
    tauri::async_runtime::spawn_blocking(detect_installed)
        .await
        .unwrap_or_default()
}

fn detect_installed() -> Vec<Agent> {
    // One search path for the whole sweep — it costs a registry read to build.
    let dirs = search_dirs();
    let mut found: Vec<Agent> = REGISTRY
        .iter()
        .filter(|a| find_in(&dirs, &installed_names(a.command)).is_some())
        .map(|a| Agent {
            id: a.id.into(),
            label: a.label.into(),
            command: a.command.into(),
        })
        .collect();

    let shell = if cfg!(windows) {
        "powershell.exe"
    } else {
        "bash"
    };
    found.push(Agent {
        id: "shell".into(),
        label: "Terminal (shell)".into(),
        command: shell.into(),
    });
    found
}

#[cfg(test)]
mod tests {
    use super::{installed_names, oneshot_invocation, spawn_env};

    #[test]
    fn installed_names_lead_with_the_canonical_command() {
        let names = installed_names("codex");
        assert_eq!(names.first(), Some(&"codex"));
        assert!(names.contains(&"codex-x86_64-pc-windows-msvc"));
    }

    /// A command ADE has no entry for — a task runner, an editor — resolves under
    /// its own name and carries no agent baggage.
    #[test]
    fn an_unknown_command_is_only_ever_itself() {
        assert_eq!(installed_names("pnpm"), vec!["pnpm"]);
        assert!(spawn_env("pnpm").is_empty());
        assert!(oneshot_invocation("pnpm").is_none());
    }

    /// Per-agent knowledge stays keyed by the canonical command, never by the
    /// executable an installer happened to lay down.
    #[test]
    fn agent_knowledge_is_keyed_by_the_canonical_command() {
        assert_eq!(oneshot_invocation("codex"), Some(&["exec"][..]));
        assert!(oneshot_invocation("codex-x86_64-pc-windows-msvc").is_none());
        assert_eq!(
            oneshot_invocation("grok"),
            Some(&["--no-auto-update", "-p"][..])
        );
        assert_eq!(spawn_env("claude"), &[("CLAUDE_CODE_NO_FLICKER", "1")]);
    }
}
