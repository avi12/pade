//! Editor detection and the user-added editors: the `Ide` shape, its lookup and the add/remove commands.

use super::editor_registry::OpenStyle;
use super::editor_registry::REGISTRY;
use crate::util::is_on_path;
use serde::Serialize;

/// Resolve an editor id against the registry, else against the caller's
/// already-loaded user-added editors - passed in so a suggestion pass over
/// many candidate ids reads the settings file once, not once per id.
pub(super) fn lookup(id: &str, added: &[Ide]) -> Option<Ide> {
    if let Some(editor) = REGISTRY.iter().find(|editor| editor.id == id) {
        return Some(Ide {
            id: editor.id.into(),
            label: editor.label.into(),
            command: editor.command.into(),
            terminal: false,
            chosen: false,
        });
    }
    // User-added editors are first-class too - resolve them by their stored id.
    added.iter().find(|editor| editor.id == id).cloned()
}

/// A launchable editor family PADE recognises. Keyed off an executable's
/// lowercased basename so the "Add editor…" flow and jump-to-line launching of
/// an added editor share one authoritative table (DRY). `style` is `None` for
/// editors with no line-jump CLI (the path is passed as-is). `terminal` marks
/// console editors (Neovim, Vim, Helix) that PADE opens inside a terminal tab
/// rather than spawning as a detached window.
pub(super) struct Family {
    label: &'static str,
    pub(super) style: Option<OpenStyle>,
    pub(super) terminal: bool,
}

pub(super) fn editor_family(basename: &str) -> Option<Family> {
    // (label, jump-to-line style, runs-in-a-terminal)
    let (label, style, terminal) = match basename {
        "code" => ("VS Code", Some(OpenStyle::VsCode), false),
        "code - insiders" => ("VS Code Insiders", Some(OpenStyle::VsCode), false),
        "cursor" => ("Cursor", Some(OpenStyle::VsCode), false),
        "antigravity" => ("Antigravity", Some(OpenStyle::VsCode), false),
        "windsurf" => ("Windsurf", Some(OpenStyle::VsCode), false),
        "codium" | "vscodium" => ("VSCodium", Some(OpenStyle::VsCode), false),
        "zed" => ("Zed", Some(OpenStyle::PathColon), false),
        "sublime_text" | "subl" => ("Sublime Text", Some(OpenStyle::PathColon), false),
        "notepad++" => ("Notepad++", None, false),
        "gvim" => ("GVim", None, false),
        "nvim" => ("Neovim", None, true),
        "vim" | "vi" => ("Vim", None, true),
        "hx" => ("Helix", None, true),
        "webstorm" | "webstorm64" => ("WebStorm", Some(OpenStyle::JetBrains), false),
        "idea" | "idea64" => ("IntelliJ IDEA", Some(OpenStyle::JetBrains), false),
        "pycharm" | "pycharm64" => ("PyCharm", Some(OpenStyle::JetBrains), false),
        "goland" | "goland64" => ("GoLand", Some(OpenStyle::JetBrains), false),
        "rider" | "rider64" => ("Rider", Some(OpenStyle::JetBrains), false),
        "clion" | "clion64" => ("CLion", Some(OpenStyle::JetBrains), false),
        "phpstorm" | "phpstorm64" => ("PhpStorm", Some(OpenStyle::JetBrains), false),
        "rubymine" | "rubymine64" => ("RubyMine", Some(OpenStyle::JetBrains), false),
        "rustrover" | "rustrover64" => ("RustRover", Some(OpenStyle::JetBrains), false),
        _ => return None,
    };
    Some(Family {
        label,
        style,
        terminal,
    })
}

/// The launcher extensions stripped from an executable's name to find its family.
pub(super) const LAUNCHER_EXTENSIONS: [&str; 5] = [".exe", ".cmd", ".bat", ".sh", ".app"];

/// The id prefix every user-added editor carries (`added-code`), which tells
/// it apart from a registry editor id.
pub(super) const ADDED_EDITOR_ID_PREFIX: &str = "added-";

/// The switcher icon key for a web project written in TypeScript.
pub(super) const TYPESCRIPT_ICON_KEY: &str = "typescript";

/// The last segment of an executable path, split on either separator.
pub(super) fn executable_file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// An executable path's lowercased basename with a known launcher extension
/// stripped (`Code.exe` → `code`, `notepad++.exe` → `notepad++`).
pub(super) fn executable_basename(path: &str) -> String {
    let file = executable_file_name(path).to_lowercase();
    LAUNCHER_EXTENSIONS
        .iter()
        .find_map(|extension| file.strip_suffix(extension))
        .map_or_else(|| file.clone(), str::to_string)
}

/// The user-added editors as `Ide`s (command = the stored executable path). A
/// console editor (Neovim, Vim, Helix) is flagged `terminal` so the UI opens it
/// in a PADE terminal tab instead of spawning it as a detached window.
pub(super) fn added_editor_ides(added: Vec<crate::workspace::AddedEditor>) -> Vec<Ide> {
    added
        .into_iter()
        .map(|editor| Ide {
            terminal: editor_family(&executable_basename(&editor.path))
                .is_some_and(|family| family.terminal),
            id: editor.id,
            label: editor.label,
            command: editor.path,
            chosen: false,
        })
        .collect()
}

/// [`added_editor_ides`] over freshly-loaded settings, for callers with no
/// settings in hand. Anything iterating candidate ids should load once and use
/// [`added_editor_ides`] instead.
pub(super) fn added_editors() -> Vec<Ide> {
    added_editor_ides(crate::workspace::load().prefs.added_editors)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Ide {
    pub(super) id: String,
    label: String,
    pub(super) command: String,
    /// A console editor PADE runs inside a terminal tab (never a detached window).
    terminal: bool,
    /// True when this editor leads [`ide_suggest`]'s ranking because of an
    /// explicit per-project pick ([`ide_choose_editor`]) - the UI badges it as
    /// the user's choice rather than the auto-detected best fit.
    pub(super) chosen: bool,
}

// `async` + `spawn_blocking`: detection probes every PATH/package-manager
// directory (and `reg` on Windows) - filesystem-and-child-process work that a
// synchronous command would run on the MAIN thread, freezing the window.
#[tauri::command]
pub(crate) async fn ide_detect() -> Vec<Ide> {
    tauri::async_runtime::spawn_blocking(installed_ides)
        .await
        .unwrap_or_default()
}

/// The installed-editor roster behind [`ide_detect`], synchronous so in-process
/// callers ([`kind_options`]) reuse it without an async hop.
pub(super) fn installed_ides() -> Vec<Ide> {
    REGISTRY
        .iter()
        .filter(|editor| is_on_path(editor.command))
        .map(|editor| Ide {
            id: editor.id.into(),
            label: editor.label.into(),
            command: editor.command.into(),
            terminal: false,
            chosen: false,
        })
        .chain(added_editors())
        .collect()
}

/// Add an editor by the full path to its executable. Validates the basename
/// against the launchable families ({@link family}) - an unsupported executable
/// (e.g. `WinRAR.exe`) is rejected with a message the UI shows inline. On
/// success the editor is persisted and appears in every editor menu.
#[tauri::command]
pub(crate) fn ide_add_editor(path: String) -> Result<crate::workspace::Settings, String> {
    let path = path.trim().to_string();
    if path.is_empty() {
        return Err("Enter the full path to an editor executable".to_string());
    }
    let basename = executable_basename(&path);
    let Some(family) = editor_family(&basename) else {
        return Err(format!(
            "\u{201c}{}\u{201d} isn\u{2019}t a supported editor. PADE can launch \
             VS Code and its forks (Cursor, Antigravity, Windsurf, VSCodium), Zed, \
             Sublime Text, Neovim and the JetBrains IDEs",
            executable_file_name(&path)
        ));
    };
    crate::workspace::add_editor(crate::workspace::AddedEditor {
        id: format!("{ADDED_EDITOR_ID_PREFIX}{basename}"),
        label: family.label.to_string(),
        path,
    })
}

/// Remove a user-added editor by its id (an `added-…` id from [`ide_add_editor`]).
/// Auto-detected editors carry no stored entry, so removing one is a no-op. Returns
/// the refreshed settings so every editor menu drops it at once.
#[tauri::command]
pub(crate) fn ide_remove_editor(id: String) -> Result<crate::workspace::Settings, String> {
    crate::workspace::remove_editor(&id)
}
