//! External IDE integration — open the current project in the editor of choice.
//!
//! ADE is agentic-first, but you still reach for a full IDE sometimes. It
//! detects installed editors (by their CLI launcher) and opens the active
//! project directory in the one you pick.

use std::process::Command;

use serde::Serialize;

use crate::util::is_on_path;

struct IdeDef {
    id: &'static str,
    label: &'static str,
    /// CLI launcher that opens a directory when given its path.
    command: &'static str,
}

const REGISTRY: &[IdeDef] = &[
    IdeDef { id: "vscode", label: "VS Code", command: "code" },
    IdeDef { id: "cursor", label: "Cursor", command: "cursor" },
    IdeDef { id: "webstorm", label: "WebStorm", command: "webstorm" },
    IdeDef { id: "idea", label: "IntelliJ IDEA", command: "idea" },
    IdeDef { id: "pycharm", label: "PyCharm", command: "pycharm" },
    IdeDef { id: "zed", label: "Zed", command: "zed" },
    IdeDef { id: "sublime", label: "Sublime Text", command: "subl" },
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ide {
    id: String,
    label: String,
    command: String,
}

#[tauri::command]
pub fn ide_detect() -> Vec<Ide> {
    REGISTRY
        .iter()
        .filter(|i| is_on_path(i.command))
        .map(|i| Ide { id: i.id.into(), label: i.label.into(), command: i.command.into() })
        .collect()
}

/// Open the current project directory in the given IDE launcher.
#[tauri::command]
pub fn ide_open(command: String) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    // On Windows the JetBrains/VS Code launchers are .cmd shims, so go through
    // the shell to resolve them the way a terminal would.
    let spawn = if cfg!(windows) {
        Command::new("cmd")
            .args(["/C", &command])
            .arg(&cwd)
            .spawn()
    } else {
        Command::new(&command).arg(&cwd).spawn()
    };
    spawn.map(|_| ()).map_err(|e| format!("failed to open {command}: {e}"))
}
