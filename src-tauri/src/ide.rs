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
    IdeDef {
        id: "vscode",
        label: "VS Code",
        command: "code",
    },
    IdeDef {
        id: "cursor",
        label: "Cursor",
        command: "cursor",
    },
    IdeDef {
        id: "webstorm",
        label: "WebStorm",
        command: "webstorm",
    },
    IdeDef {
        id: "idea",
        label: "IntelliJ IDEA",
        command: "idea",
    },
    IdeDef {
        id: "pycharm",
        label: "PyCharm",
        command: "pycharm",
    },
    IdeDef {
        id: "goland",
        label: "GoLand",
        command: "goland",
    },
    IdeDef {
        id: "rustrover",
        label: "RustRover",
        command: "rustrover",
    },
    IdeDef {
        id: "androidstudio",
        label: "Android Studio",
        command: "studio",
    },
    IdeDef {
        id: "zed",
        label: "Zed",
        command: "zed",
    },
    IdeDef {
        id: "sublime",
        label: "Sublime Text",
        command: "subl",
    },
];

/// Detected project kind → the IDEs that suit it best, in priority order.
/// Generalist editors are appended to every suggestion list as a fallback.
const PREFERENCES: &[(&str, &[&str])] = &[
    ("android", &["androidstudio", "idea"]),
    ("web", &["webstorm", "vscode", "cursor"]),
    ("python", &["pycharm", "vscode"]),
    ("go", &["goland", "vscode"]),
    ("rust", &["rustrover", "zed", "vscode"]),
    ("java", &["idea"]),
];
const GENERALISTS: &[&str] = &["vscode", "cursor", "zed", "sublime"];

fn lookup(id: &str) -> Option<Ide> {
    REGISTRY.iter().find(|i| i.id == id).map(|i| Ide {
        id: i.id.into(),
        label: i.label.into(),
        command: i.command.into(),
    })
}

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
        .map(|i| Ide {
            id: i.id.into(),
            label: i.label.into(),
            command: i.command.into(),
        })
        .collect()
}

/// Sniff the project kinds present in the current directory from marker files.
fn detect_kinds(cwd: &std::path::Path) -> Vec<&'static str> {
    let has = |name: &str| cwd.join(name).exists();
    let mut kinds = Vec::new();
    // Android is checked first: an Android project is also "web"/"java"-ish, but
    // Android Studio is the right call when the manifest/gradle wrapper is there.
    if has("AndroidManifest.xml") || has("gradlew") || has("settings.gradle") {
        kinds.push("android");
    }
    if has("package.json") || has("tsconfig.json") || has("index.html") {
        kinds.push("web");
    }
    if has("pyproject.toml") || has("requirements.txt") || has("setup.py") {
        kinds.push("python");
    }
    if has("go.mod") {
        kinds.push("go");
    }
    if has("Cargo.toml") {
        kinds.push("rust");
    }
    if has("pom.xml") {
        kinds.push("java");
    }
    kinds
}

/// Installed IDEs ranked for the current project type — best match first, then
/// generalist editors. Lets the UI offer "Open in `WebStorm`" for a web project,
/// "Android Studio" for an Android one, etc., limited to what's installed.
#[tauri::command]
pub fn ide_suggest() -> Result<Vec<Ide>, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let kinds = detect_kinds(&cwd);

    // Preferred ids for the detected kinds, then generalists, deduped in order.
    let preferred = kinds.iter().flat_map(|k| {
        PREFERENCES
            .iter()
            .find(|(kind, _)| kind == k)
            .map_or::<&[&str], _>(&[], |(_, ids)| *ids)
            .iter()
            .copied()
    });

    let mut ordered = Vec::new();
    for id in preferred.chain(GENERALISTS.iter().copied()) {
        let command = lookup(id).map(|i| i.command).unwrap_or_default();
        let is_new_and_installed = !ordered.contains(&id) && is_on_path(&command);
        if is_new_and_installed {
            ordered.push(id);
        }
    }
    Ok(ordered.into_iter().filter_map(lookup).collect())
}

/// Open a project directory in the given IDE launcher. `path` defaults to the
/// current project when omitted (topbar), or names a specific project (picker).
#[tauri::command]
pub fn ide_open(command: String, path: Option<String>) -> Result<(), String> {
    let dir = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    // On Windows the JetBrains/VS Code launchers are .cmd shims, so go through
    // the shell to resolve them the way a terminal would.
    let spawn = if cfg!(windows) {
        Command::new("cmd").args(["/C", &command]).arg(&dir).spawn()
    } else {
        Command::new(&command).arg(&dir).spawn()
    };
    spawn
        .map(|_| ())
        .map_err(|e| format!("failed to open {command}: {e}"))
}
