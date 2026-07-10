//! External IDE integration — open the current project in the editor of choice.
//!
//! ADE is agentic-first, but you still reach for a full IDE sometimes. It
//! detects installed editors (by their CLI launcher) and opens the active
//! project directory in the one you pick.

use std::process::Command;

use serde::Serialize;

use crate::util::is_on_path;

/// How a launcher wants a jump-to-line request shaped on the command line —
/// verified against each editor's official CLI docs.
#[derive(Clone, Copy)]
enum OpenStyle {
    /// VS Code family: `-r -g file:line` — reuse the already-open window (so it
    /// navigates in place rather than spawning a new one) and go to the line.
    VsCode,
    /// `JetBrains` family: `--line <n> file` — flags first, path last. The
    /// launcher hands the file to the running IDE instance when one is up.
    JetBrains,
    /// Zed / Sublime: a `file:line` colon suffix on the path; the CLI routes to
    /// the running editor instance.
    PathColon,
}

struct IdeDef {
    id: &'static str,
    label: &'static str,
    /// CLI launcher that opens a path (file or directory) when given it.
    command: &'static str,
    /// How this launcher phrases a jump-to-line.
    style: OpenStyle,
}

const REGISTRY: &[IdeDef] = &[
    IdeDef {
        id: "vscode",
        label: "VS Code",
        command: "code",
        style: OpenStyle::VsCode,
    },
    IdeDef {
        id: "cursor",
        label: "Cursor",
        command: "cursor",
        style: OpenStyle::VsCode,
    },
    IdeDef {
        id: "webstorm",
        label: "WebStorm",
        command: "webstorm",
        style: OpenStyle::JetBrains,
    },
    IdeDef {
        id: "idea",
        label: "IntelliJ IDEA",
        command: "idea",
        style: OpenStyle::JetBrains,
    },
    IdeDef {
        id: "pycharm",
        label: "PyCharm",
        command: "pycharm",
        style: OpenStyle::JetBrains,
    },
    IdeDef {
        id: "goland",
        label: "GoLand",
        command: "goland",
        style: OpenStyle::JetBrains,
    },
    IdeDef {
        id: "rustrover",
        label: "RustRover",
        command: "rustrover",
        style: OpenStyle::JetBrains,
    },
    IdeDef {
        id: "androidstudio",
        label: "Android Studio",
        command: "studio",
        style: OpenStyle::JetBrains,
    },
    IdeDef {
        id: "zed",
        label: "Zed",
        command: "zed",
        style: OpenStyle::PathColon,
    },
    IdeDef {
        id: "sublime",
        label: "Sublime Text",
        command: "subl",
        style: OpenStyle::PathColon,
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

/// The single best-matching project kind for `cwd` — the highest-priority marker
/// present (Android before web/java, etc.), or `None` for an unrecognised project.
fn primary_kind(cwd: &std::path::Path) -> Option<&'static str> {
    detect_kinds(cwd).first().copied()
}

/// The current project's primary kind (e.g. `"rust"`, `"web"`), or `None` when no
/// marker file is recognised. Drives the editor-rules UI in settings.
#[tauri::command]
pub fn ide_project_kind() -> Option<String> {
    std::env::current_dir()
        .ok()
        .as_deref()
        .and_then(primary_kind)
        .map(str::to_string)
}

/// An id is worth suggesting only if its launcher is actually installed.
fn is_installed(id: &str) -> bool {
    lookup(id).is_some_and(|i| is_on_path(&i.command))
}

/// Installed IDEs ranked for the current project, best match first. The
/// editor-rules engine takes precedence: a user rule for the project's primary
/// kind is offered first, then the configured fallback, then the built-in
/// auto-ranking (kind preferences + generalists). Deduped, installed-only.
#[tauri::command]
pub fn ide_suggest() -> Result<Vec<Ide>, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let kinds = detect_kinds(&cwd);
    let prefs = crate::workspace::load().prefs;

    // 1) Explicit rule for the primary kind, 2) fallback, 3) auto-ranking.
    let rule = primary_kind(&cwd).and_then(|k| prefs.ide_rules.get(k).cloned());
    let configured = rule.into_iter().chain(prefs.ide_fallback);

    // Preferred ids for the detected kinds, then generalists, deduped in order.
    let preferred = kinds.iter().flat_map(|k| {
        PREFERENCES
            .iter()
            .find(|(kind, _)| kind == k)
            .map_or::<&[&str], _>(&[], |(_, ids)| *ids)
            .iter()
            .copied()
    });

    let mut ordered: Vec<String> = Vec::new();
    let auto = preferred
        .chain(GENERALISTS.iter().copied())
        .map(str::to_string);
    for id in configured.chain(auto) {
        let is_new_and_installed = !ordered.contains(&id) && is_installed(&id);
        if is_new_and_installed {
            ordered.push(id);
        }
    }
    Ok(ordered.iter().filter_map(|id| lookup(id)).collect())
}

/// The jump-to-line style for a launcher command, or `None` for an unknown one.
fn open_style(command: &str) -> Option<OpenStyle> {
    REGISTRY
        .iter()
        .find(|i| i.command == command)
        .map(|i| i.style)
}

/// The launcher arguments for opening `target` — jumping to `line` when one is
/// given and the launcher's style is known (otherwise the path is passed as-is,
/// which every editor accepts).
fn open_args(command: &str, target: String, line: Option<u32>) -> Vec<String> {
    match (line, open_style(command)) {
        (Some(n), Some(OpenStyle::VsCode)) => {
            vec!["-r".to_owned(), "-g".to_owned(), format!("{target}:{n}")]
        }
        (Some(n), Some(OpenStyle::JetBrains)) => vec!["--line".to_owned(), n.to_string(), target],
        (Some(n), Some(OpenStyle::PathColon)) => vec![format!("{target}:{n}")],
        _ => vec![target],
    }
}

/// Open a path in the given IDE launcher. `path` defaults to the current project
/// directory when omitted (topbar); a `line` (only meaningful with a file path)
/// jumps the editor to that line, phrased per the launcher's own CLI.
#[tauri::command]
pub fn ide_open(command: String, path: Option<String>, line: Option<u32>) -> Result<(), String> {
    let has_path = path.is_some();
    let target = match path {
        Some(p) => p,
        None => std::env::current_dir()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned(),
    };
    // A line only applies when we were handed a file to jump into.
    let args = open_args(&command, target, line.filter(|_| has_path));

    // On Windows the JetBrains/VS Code launchers are .cmd shims, so go through
    // the shell to resolve them the way a terminal would.
    let spawn = if cfg!(windows) {
        Command::new("cmd")
            .arg("/C")
            .arg(&command)
            .args(&args)
            .spawn()
    } else {
        Command::new(&command).args(&args).spawn()
    };
    spawn
        .map(|_| ())
        .map_err(|e| format!("failed to open {command}: {e}"))
}
