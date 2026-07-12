//! External IDE integration — open the current project in the editor of choice.
//!
//! ADE is agentic-first, but you still reach for a full IDE sometimes. It
//! detects installed editors (by their CLI launcher) and opens the active
//! project directory in the one you pick.

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
    /// Visual Studio: `/Edit file` opens the file in the running instance. There
    /// is no reliable CLI to also jump to a line (combining `/Edit` with a
    /// `/Command "Edit.Goto"` doesn't navigate), so the line is dropped.
    VisualStudio,
}

struct IdeDef {
    id: &'static str,
    label: &'static str,
    /// CLI launcher that opens a path (file or directory) when given it.
    command: &'static str,
    /// How this launcher phrases a jump-to-line.
    style: OpenStyle,
    /// `JetBrains` `jetbrains://<tool>/…` protocol tool id, when this IDE is a
    /// `JetBrains` one. The CLI can't reliably route a file to the *correct open
    /// project window* (it drops it into the last-active one); the protocol
    /// targets a project by name, so opening a file lands in the right window.
    protocol: Option<&'static str>,
}

const REGISTRY: &[IdeDef] = &[
    IdeDef {
        id: "vscode",
        label: "VS Code",
        command: "code",
        style: OpenStyle::VsCode,
        protocol: None,
    },
    IdeDef {
        id: "cursor",
        label: "Cursor",
        command: "cursor",
        style: OpenStyle::VsCode,
        protocol: None,
    },
    IdeDef {
        id: "webstorm",
        label: "WebStorm",
        command: "webstorm",
        style: OpenStyle::JetBrains,
        protocol: Some("webstorm"),
    },
    IdeDef {
        id: "idea",
        label: "IntelliJ IDEA",
        command: "idea",
        style: OpenStyle::JetBrains,
        protocol: Some("idea"),
    },
    IdeDef {
        id: "pycharm",
        label: "PyCharm",
        command: "pycharm",
        style: OpenStyle::JetBrains,
        protocol: Some("pycharm"),
    },
    IdeDef {
        id: "goland",
        label: "GoLand",
        command: "goland",
        style: OpenStyle::JetBrains,
        protocol: Some("goland"),
    },
    IdeDef {
        id: "rustrover",
        label: "RustRover",
        command: "rustrover",
        style: OpenStyle::JetBrains,
        protocol: Some("rustrover"),
    },
    IdeDef {
        id: "androidstudio",
        label: "Android Studio",
        command: "studio",
        style: OpenStyle::JetBrains,
        protocol: Some("studio"),
    },
    IdeDef {
        id: "zed",
        label: "Zed",
        command: "zed",
        style: OpenStyle::PathColon,
        protocol: None,
    },
    IdeDef {
        id: "sublime",
        label: "Sublime Text",
        command: "subl",
        style: OpenStyle::PathColon,
        protocol: None,
    },
    IdeDef {
        id: "visualstudio",
        label: "Visual Studio",
        command: "devenv",
        style: OpenStyle::VisualStudio,
        protocol: None,
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
    ("dotnet", &["visualstudio", "vscode"]),
];
const GENERALISTS: &[&str] = &["vscode", "cursor", "zed", "sublime"];

fn lookup(id: &str) -> Option<Ide> {
    if let Some(i) = REGISTRY.iter().find(|i| i.id == id) {
        return Some(Ide {
            id: i.id.into(),
            label: i.label.into(),
            command: i.command.into(),
            terminal: false,
        });
    }
    // User-added editors are first-class too — resolve them by their stored id.
    added_editors().into_iter().find(|e| e.id == id)
}

/// A launchable editor family PADE recognises. Keyed off an executable's
/// lowercased basename so the "Add editor…" flow and jump-to-line launching of
/// an added editor share one authoritative table (DRY). `style` is `None` for
/// editors with no line-jump CLI (the path is passed as-is); `protocol` is the
/// `JetBrains` tool id for `JetBrains` IDEs. `terminal` marks console editors
/// (Neovim, Vim, Helix) that PADE opens inside a terminal tab rather than
/// spawning as a detached window.
struct Family {
    label: &'static str,
    style: Option<OpenStyle>,
    protocol: Option<&'static str>,
    terminal: bool,
}

fn family(basename: &str) -> Option<Family> {
    // (label, jump-to-line style, JetBrains protocol, runs-in-a-terminal)
    let (label, style, protocol, terminal) = match basename {
        "code" => ("VS Code", Some(OpenStyle::VsCode), None, false),
        "code - insiders" => ("VS Code Insiders", Some(OpenStyle::VsCode), None, false),
        "cursor" => ("Cursor", Some(OpenStyle::VsCode), None, false),
        "zed" => ("Zed", Some(OpenStyle::PathColon), None, false),
        "sublime_text" | "subl" => ("Sublime Text", Some(OpenStyle::PathColon), None, false),
        "notepad++" => ("Notepad++", None, None, false),
        "gvim" => ("GVim", None, None, false),
        "nvim" => ("Neovim", None, None, true),
        "vim" | "vi" => ("Vim", None, None, true),
        "hx" => ("Helix", None, None, true),
        "webstorm" | "webstorm64" => (
            "WebStorm",
            Some(OpenStyle::JetBrains),
            Some("webstorm"),
            false,
        ),
        "idea" | "idea64" => (
            "IntelliJ IDEA",
            Some(OpenStyle::JetBrains),
            Some("idea"),
            false,
        ),
        "pycharm" | "pycharm64" => (
            "PyCharm",
            Some(OpenStyle::JetBrains),
            Some("pycharm"),
            false,
        ),
        "goland" | "goland64" => ("GoLand", Some(OpenStyle::JetBrains), Some("goland"), false),
        "rider" | "rider64" => ("Rider", Some(OpenStyle::JetBrains), Some("rider"), false),
        "clion" | "clion64" => ("CLion", Some(OpenStyle::JetBrains), Some("clion"), false),
        "phpstorm" | "phpstorm64" => (
            "PhpStorm",
            Some(OpenStyle::JetBrains),
            Some("phpstorm"),
            false,
        ),
        "rubymine" | "rubymine64" => (
            "RubyMine",
            Some(OpenStyle::JetBrains),
            Some("rubymine"),
            false,
        ),
        "rustrover" | "rustrover64" => (
            "RustRover",
            Some(OpenStyle::JetBrains),
            Some("rustrover"),
            false,
        ),
        _ => return None,
    };
    Some(Family {
        label,
        style,
        protocol,
        terminal,
    })
}

/// An executable path's lowercased basename with a known launcher extension
/// stripped (`Code.exe` → `code`, `notepad++.exe` → `notepad++`).
fn exe_basename(path: &str) -> String {
    let file = path
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_lowercase();
    for ext in [".exe", ".cmd", ".bat", ".sh", ".app"] {
        if let Some(stripped) = file.strip_suffix(ext) {
            return stripped.to_string();
        }
    }
    file
}

/// The user-added editors as `Ide`s (command = the stored executable path). A
/// console editor (Neovim, Vim, Helix) is flagged `terminal` so the UI opens it
/// in a PADE terminal tab instead of spawning it as a detached window.
fn added_editors() -> Vec<Ide> {
    crate::workspace::load()
        .prefs
        .added_editors
        .into_iter()
        .map(|e| Ide {
            terminal: family(&exe_basename(&e.path)).is_some_and(|f| f.terminal),
            id: e.id,
            label: e.label,
            command: e.path,
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ide {
    id: String,
    label: String,
    command: String,
    /// A console editor PADE runs inside a terminal tab (never a detached window).
    terminal: bool,
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
            terminal: false,
        })
        .chain(added_editors())
        .collect()
}

/// Add an editor by the full path to its executable. Validates the basename
/// against the launchable families ({@link family}) — an unsupported executable
/// (e.g. `WinRAR.exe`) is rejected with a message the UI shows inline. On
/// success the editor is persisted and appears in every editor menu.
#[tauri::command]
pub fn ide_add_editor(path: String) -> Result<crate::workspace::Settings, String> {
    let path = path.trim().to_string();
    if path.is_empty() {
        return Err("Enter the full path to an editor executable.".to_string());
    }
    let file = path
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(&path)
        .to_string();
    let Some(fam) = family(&exe_basename(&path)) else {
        return Err(format!(
            "\u{201c}{file}\u{201d} isn\u{2019}t a supported editor. PADE can launch \
             VS Code, Cursor, Zed, Sublime Text, Neovim and the JetBrains IDEs."
        ));
    };
    crate::workspace::add_editor(crate::workspace::AddedEditor {
        id: format!("added-{}", exe_basename(&path)),
        label: fam.label.to_string(),
        path,
    })
}

/// Whether any direct child of `dir` has the given extension (case-insensitive).
fn has_ext(dir: &std::path::Path, ext: &str) -> bool {
    std::fs::read_dir(dir).ok().is_some_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(ext))
        })
    })
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
    // .NET / C++ — Visual Studio's home turf. Solution/project files or a
    // dotnet marker; scanned by extension since names vary per project.
    if has("global.json") || has_ext(cwd, "sln") || has_ext(cwd, "csproj") {
        kinds.push("dotnet");
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

/// An id is worth suggesting only if it's actually launchable — a registry
/// launcher on PATH, or a user-added editor (its stored path is the launcher).
fn is_installed(id: &str) -> bool {
    if id.starts_with("added-") {
        return added_editors().iter().any(|e| e.id == id);
    }
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
/// An added editor's command is its executable path, so fall back to matching
/// the family by basename.
fn open_style(command: &str) -> Option<OpenStyle> {
    if let Some(i) = REGISTRY.iter().find(|i| i.command == command) {
        return Some(i.style);
    }
    family(&exe_basename(command)).and_then(|f| f.style)
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
        // Visual Studio opens the file in the running instance; no line jump.
        (_, Some(OpenStyle::VisualStudio)) => vec!["/Edit".to_owned(), target],
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
    // the shell to resolve them the way a terminal would. An added editor's
    // command is an absolute executable path, so spawn it directly instead.
    let is_path = command.contains('/') || command.contains('\\');
    let spawn = if cfg!(windows) && !is_path {
        crate::util::command("cmd")
            .arg("/C")
            .arg(&command)
            .args(&args)
            .spawn()
    } else {
        crate::util::command(&command).args(&args).spawn()
    };
    spawn
        .map(|_| ())
        .map_err(|e| format!("failed to open {command}: {e}"))
}

/// The `JetBrains` protocol tool id for a launcher command, if it is a
/// `JetBrains` IDE (else `None`, meaning "use the CLI").
fn protocol_id(command: &str) -> Option<&'static str> {
    if let Some(i) = REGISTRY.iter().find(|i| i.command == command) {
        return i.protocol;
    }
    family(&exe_basename(command)).and_then(|f| f.protocol)
}

/// The project's display name — its root folder's basename, which is how the
/// `JetBrains` protocol identifies an open project.
fn project_name(project: &str) -> String {
    project
        .replace('\\', "/")
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(project)
        .to_string()
}

/// `file` expressed relative to `project` (forward slashes), or the absolute
/// path when it isn't under the project. Matching is case-insensitive on `Windows`.
fn relative_path(project: &str, file: &str) -> String {
    let root = project.replace('\\', "/");
    let root = root.trim_end_matches('/');
    let path = file.replace('\\', "/");
    let under = if cfg!(windows) {
        path.to_lowercase().starts_with(&root.to_lowercase())
    } else {
        path.starts_with(root)
    };
    if under {
        path.get(root.len()..)
            .unwrap_or(&path)
            .trim_start_matches('/')
            .to_string()
    } else {
        path
    }
}

/// Open a file in the IDE so it lands in the window that already has `project`
/// open, jumping to `line` when given. A `JetBrains` IDE uses its `jetbrains://`
/// scheme (targets the project by name, unlike the CLI); other editors use the
/// CLI open (`VS Code` reuses its window with `-r`). `project` is the root dir.
#[tauri::command]
pub fn ide_open_file(
    command: String,
    project: String,
    file: String,
    line: Option<u32>,
) -> Result<(), String> {
    if let Some(tool) = protocol_id(&command) {
        let mut path = relative_path(&project, &file);
        if let Some(number) = line {
            path.push(':');
            path.push_str(&number.to_string());
        }
        // Keep `/` and `:` literal so the path and its `:line` suffix parse.
        let url = format!(
            "jetbrains://{tool}/navigate/reference?project={}&path={}",
            crate::util::percent_encode(&project_name(&project), b""),
            crate::util::percent_encode(&path, b"/:")
        );
        return crate::os::open_url(url);
    }

    // VS Code family and the rest: the CLI open handles jump-to-line and reuses
    // the running window (`-r`), which is a single-project-window model.
    ide_open(command, Some(file), line)
}

#[cfg(test)]
mod tests {
    use super::{
        exe_basename, family, open_args, open_style, project_name, relative_path, OpenStyle,
    };

    #[test]
    fn exe_basename_strips_extension_and_lowercases() {
        assert_eq!(exe_basename("C:\\Program Files\\Code.exe"), "code");
        assert_eq!(exe_basename("/usr/bin/nvim"), "nvim");
        assert_eq!(exe_basename("notepad++.exe"), "notepad++");
    }

    #[test]
    fn family_maps_a_jetbrains_editor_to_its_protocol() {
        let webstorm = family("webstorm64").expect("supported");
        assert!(matches!(webstorm.style, Some(OpenStyle::JetBrains)));
        assert_eq!(webstorm.protocol, Some("webstorm"));
    }

    #[test]
    fn family_rejects_a_non_editor_executable() {
        assert!(family("winrar").is_none());
    }

    #[test]
    fn console_editors_are_flagged_terminal_but_gui_ones_are_not() {
        assert!(family("nvim").expect("supported").terminal);
        assert!(family("vi").expect("supported").terminal);
        assert!(!family("code").expect("supported").terminal);
        assert!(!family("gvim").expect("supported").terminal);
    }

    #[test]
    fn open_style_resolves_an_added_editor_by_its_path() {
        assert!(matches!(
            open_style("C:\\Users\\me\\AppData\\Local\\Programs\\cursor\\Cursor.exe"),
            Some(OpenStyle::VsCode)
        ));
    }

    #[test]
    fn project_name_is_the_root_basename() {
        assert_eq!(project_name("C:\\repositories\\avi\\pade"), "pade");
        assert_eq!(project_name("/home/me/proj/"), "proj");
    }

    #[test]
    fn relative_path_strips_the_project_root() {
        assert_eq!(
            relative_path("C:\\repos\\pade", "C:\\repos\\pade\\src\\App.svelte"),
            "src/App.svelte"
        );
    }

    #[test]
    fn relative_path_keeps_an_outside_file_absolute() {
        assert_eq!(
            relative_path("C:/repos/pade", "D:/other/file.ts"),
            "D:/other/file.ts"
        );
    }

    #[test]
    fn vscode_style_reuses_the_window_and_jumps_to_the_line() {
        assert_eq!(
            open_args("code", "C:/p/file.ts".to_string(), Some(12)),
            ["-r", "-g", "C:/p/file.ts:12"]
        );
    }

    #[test]
    fn jetbrains_style_passes_the_line_flag_before_the_path() {
        assert_eq!(
            open_args("webstorm", "C:/p/file.ts".to_string(), Some(7)),
            ["--line", "7", "C:/p/file.ts"]
        );
    }

    #[test]
    fn visual_studio_edits_the_file_and_drops_the_line() {
        assert_eq!(
            open_args("devenv", "C:/p/file.cs".to_string(), Some(42)),
            ["/Edit", "C:/p/file.cs"]
        );
    }

    #[test]
    fn path_colon_style_suffixes_the_line_onto_the_path() {
        assert_eq!(
            open_args("zed", "C:/p/file.ts".to_string(), Some(3)),
            ["C:/p/file.ts:3"]
        );
    }

    #[test]
    fn a_bare_path_passes_through_without_a_line() {
        assert_eq!(open_args("code", "C:/p".to_string(), None), ["C:/p"]);
    }

    #[test]
    fn an_unknown_launcher_gets_the_plain_path_even_with_a_line() {
        assert_eq!(
            open_args("notepad", "C:/p/file.ts".to_string(), Some(9)),
            ["C:/p/file.ts"]
        );
    }
}
