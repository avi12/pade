//! Agent registry & detection.
//!
//! ADE is agent-agnostic: it discovers which agent CLIs are installed and lets
//! the user launch, switch, and combine them. Adding a backend is one entry in
//! REGISTRY — nothing else in the app hard-codes "claude".

use serde::Serialize;

use crate::util::is_on_path;

struct AgentDef {
    id: &'static str,
    label: &'static str,
    /// The executable to look for and run.
    command: &'static str,
    /// Args that make the CLI answer one prompt non-interactively and exit, with
    /// the prompt appended as the final arg (used for auto-naming). `None` = no
    /// headless mode we can drive.
    oneshot: Option<&'static [&'static str]>,
}

/// Known agent backends, in preferred display order. The plain shell is always
/// offered last as a universal fallback.
const REGISTRY: &[AgentDef] = &[
    AgentDef {
        id: "claude",
        label: "Claude Code",
        command: "claude",
        oneshot: Some(&["-p"]),
    },
    AgentDef {
        id: "codex",
        label: "Codex",
        command: "codex",
        oneshot: Some(&["exec"]),
    },
    AgentDef {
        id: "antigravity",
        label: "Antigravity CLI",
        command: "antigravity",
        oneshot: None,
    },
    AgentDef {
        id: "cursor",
        label: "Cursor CLI",
        command: "cursor-agent",
        oneshot: None,
    },
    AgentDef {
        id: "aider",
        label: "aider",
        command: "aider",
        oneshot: None,
    },
];

/// How to invoke `command` headlessly for a one-shot prompt (auto-naming), if we
/// know a way. Keeps the registry the single source of truth (DRY).
pub fn oneshot_invocation(command: &str) -> Option<&'static [&'static str]> {
    REGISTRY
        .iter()
        .find(|a| a.command == command)
        .and_then(|a| a.oneshot)
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
/// Async + `spawn_blocking`: detection runs `where`/`which` per agent, which is
/// slow enough (~hundreds of ms) that running it as a synchronous command would
/// block Tauri's main thread — and the main thread also drives Windows' window
/// move loop, so a sync detect fired on window-focus stalls dragging. Off-thread
/// it can't.
#[tauri::command]
pub async fn agents_detect() -> Vec<Agent> {
    tauri::async_runtime::spawn_blocking(detect_installed)
        .await
        .unwrap_or_default()
}

fn detect_installed() -> Vec<Agent> {
    let mut found: Vec<Agent> = REGISTRY
        .iter()
        .filter(|a| is_on_path(a.command))
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
