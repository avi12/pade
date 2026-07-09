//! Agent registry & detection.
//!
//! ADE is agent-agnostic: it discovers which agent CLIs are installed and lets
//! the user launch, switch, and combine them. Adding a backend is one entry in
//! REGISTRY — nothing else in the app hard-codes "claude".

use std::process::Command;

use serde::Serialize;

struct AgentDef {
    id: &'static str,
    label: &'static str,
    /// The executable to look for and run.
    command: &'static str,
}

/// Known agent backends, in preferred display order. The plain shell is always
/// offered last as a universal fallback.
const REGISTRY: &[AgentDef] = &[
    AgentDef { id: "claude", label: "Claude Code", command: "claude" },
    AgentDef { id: "codex", label: "Codex", command: "codex" },
    AgentDef { id: "antigravity", label: "Antigravity CLI", command: "antigravity" },
    AgentDef { id: "cursor", label: "Cursor CLI", command: "cursor-agent" },
    AgentDef { id: "aider", label: "aider", command: "aider" },
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    id: String,
    label: String,
    command: String,
}

/// Is `command` resolvable on PATH? Uses the platform's own resolver so shims
/// (.cmd/.ps1 on Windows) are found the same way a shell would find them.
fn is_installed(command: &str) -> bool {
    let (finder, args): (&str, [&str; 1]) = if cfg!(windows) {
        ("where", [command])
    } else {
        ("which", [command])
    };
    Command::new(finder)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Every installed agent backend. The shell fallback is appended so the list is
/// never empty (there is always something to launch).
#[tauri::command]
pub fn agents_detect() -> Vec<Agent> {
    let mut found: Vec<Agent> = REGISTRY
        .iter()
        .filter(|a| is_installed(a.command))
        .map(|a| Agent {
            id: a.id.into(),
            label: a.label.into(),
            command: a.command.into(),
        })
        .collect();

    let shell = if cfg!(windows) { "powershell.exe" } else { "bash" };
    found.push(Agent {
        id: "shell".into(),
        label: "Terminal (shell)".into(),
        command: shell.into(),
    });
    found
}
