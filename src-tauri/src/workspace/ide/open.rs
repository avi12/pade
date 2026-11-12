//! Opening a project or a file in an editor: the launcher arguments and the detached spawn.

use super::editor_registry::OpenStyle;
use super::editor_registry::REGISTRY;
use super::editors::editor_family;
use super::editors::executable_basename;

/// The jump-to-line style for a launcher command, or `None` for an unknown one.
/// An added editor's command is its executable path, so fall back to matching
/// the family by basename.
pub(super) fn open_style(command: &str) -> Option<OpenStyle> {
    if let Some(editor) = REGISTRY.iter().find(|editor| editor.command == command) {
        return Some(editor.style);
    }
    editor_family(&executable_basename(command)).and_then(|family| family.style)
}

/// The launcher arguments for opening `target` - jumping to `line` when one is
/// given and the launcher's style is known (otherwise the path is passed as-is,
/// which every editor accepts). With a `workspace`, the project folder rides
/// along for every family whose CLI has a workspace+file form, so the file
/// opens *inside its project's window* - not a bare single-file window; a
/// family with no such form keeps the plain-file behavior.
pub(super) fn open_arguments(
    command: &str,
    target: String,
    line: Option<u32>,
    workspace: Option<&str>,
) -> Vec<String> {
    match (open_style(command), workspace, line) {
        // VS Code family: `code <folder> -g <file>[:<line>]` - the folder names
        // the workspace (a window already showing it is focused, else one
        // opens) and `-g`/`--goto` jumps to the file:line inside it (VS Code
        // CLI docs, "Opening files and folders"). Deliberately no `-r` here:
        // combined with a folder, `-r` re-points the last-active window at the
        // folder instead of finding the project's own window.
        (Some(OpenStyle::VsCode), Some(folder), Some(line_number)) => {
            vec![
                folder.to_owned(),
                "-g".to_owned(),
                format!("{target}:{line_number}"),
            ]
        }
        (Some(OpenStyle::VsCode), Some(folder), None) => {
            vec![folder.to_owned(), "-g".to_owned(), target]
        }
        (Some(OpenStyle::VsCode), None, Some(line_number)) => {
            vec![
                "-r".to_owned(),
                "-g".to_owned(),
                format!("{target}:{line_number}"),
            ]
        }
        // JetBrains: `idea <projectDir> [--line <n>] <file>`. The docs
        // ("Open files from the command line") only promise the single-path
        // form `idea64.exe [--line <number>] <path>`; the leading project
        // directory rides the launcher's undocumented-but-real multi-path
        // handling (CommandLineProcessor opens each path, a directory as a
        // project), which puts the file in that project's window instead of
        // one guessed from the file's ancestors. Smoke-test per launcher
        // (Toolbox shims included) when adding a new JetBrains family entry.
        (Some(OpenStyle::JetBrains), Some(folder), Some(line_number)) => vec![
            folder.to_owned(),
            "--line".to_owned(),
            line_number.to_string(),
            target,
        ],
        (Some(OpenStyle::JetBrains), None, Some(line_number)) => {
            vec!["--line".to_owned(), line_number.to_string(), target]
        }
        // Zed / Sublime open every listed path in one window, so the folder
        // becomes (or joins) the workspace and the colon suffix jumps inside it
        // (`zed <dir> <file>:<line>`, `subl <dir> <file>:<line>`).
        (Some(OpenStyle::PathColon), Some(folder), Some(line_number)) => {
            vec![folder.to_owned(), format!("{target}:{line_number}")]
        }
        (Some(OpenStyle::PathColon), None, Some(line_number)) => {
            vec![format!("{target}:{line_number}")]
        }
        // A line-less folder-carrying open shares one shape across JetBrains
        // and Zed/Sublime: the workspace folder, then the bare file.
        (Some(OpenStyle::JetBrains | OpenStyle::PathColon), Some(folder), None) => {
            vec![folder.to_owned(), target]
        }
        // Visual Studio opens the file in the running instance; its CLI has no
        // trustworthy folder+file form, and no line jump either.
        (Some(OpenStyle::VisualStudio), _, _) => vec!["/Edit".to_owned(), target],
        _ => vec![target],
    }
}

/// Spawn a GUI-IDE launcher with prepared arguments, **detached** so the editor
/// outlives ADE (requirement: closing ADE must not kill the IDE - see
/// [`crate::util::spawn_detached`] for the job/console reasoning). On Windows the
/// JetBrains/VS Code launchers are `.cmd` shims, so go through `cmd /C` to
/// resolve them the way a terminal would; an added editor's command is an
/// absolute executable path, so spawn it directly. Both paths go through the one
/// detached-spawn helper (DRY) so the editor breaks out of ADE's job either way.
/// (Console editors - Neovim/Vim/Helix - never reach here; they run inside a
/// PADE terminal tab and are meant to stay tied to ADE.)
pub(super) fn spawn_launcher(command: &str, arguments: &[String]) -> Result<(), String> {
    let is_path = command.contains('/') || command.contains('\\');
    let spawn = if cfg!(windows) && !is_path {
        let mut shell_arguments = Vec::with_capacity(arguments.len() + 2);
        shell_arguments.push(crate::util::COMMAND_SHELL_RUN_FLAG.to_owned());
        shell_arguments.push(command.to_owned());
        shell_arguments.extend_from_slice(arguments);
        crate::util::spawn_detached(crate::util::COMMAND_SHELL, &shell_arguments)
    } else {
        crate::util::spawn_detached(command, arguments)
    };
    spawn
        .map(|_| ())
        .map_err(|error| format!("failed to open {command}: {error}"))
}

/// Open a path in the given IDE launcher. `path` defaults to the current project
/// directory when omitted (topbar); a `line` (only meaningful with a file path)
/// jumps the editor to that line, phrased per the launcher's own CLI.
#[tauri::command]
pub async fn ide_open(
    command: String,
    path: Option<String>,
    line: Option<u32>,
) -> Result<(), String> {
    let has_path = path.is_some();
    let target = match path {
        Some(path) => path,
        None => std::env::current_dir()
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .into_owned(),
    };
    // A line only applies when we were handed a file to jump into.
    let arguments = open_arguments(&command, target, line.filter(|_| has_path), None);
    spawn_launcher(&command, &arguments)
}

/// Reveal a file in the given editor, jumping to `line` when given, **inside
/// the window that has `project` open**. Every family with a workspace+file CLI
/// form gets the project folder alongside the file (VS Code `<dir> -g
/// file:line`, `JetBrains` `<dir> --line n file`, Zed/Sublime `<dir>
/// file:line`), so a cold editor opens the whole workspace at the right line -
/// never a bare single-file window - and a running one focuses the project's
/// own window. A family with no such form (Visual Studio, Notepad++, …)
/// receives the file alone, as before.
#[tauri::command]
pub async fn ide_open_file(
    command: String,
    project: String,
    file: String,
    line: Option<u32>,
) -> Result<(), String> {
    let workspace = (!project.trim().is_empty()).then_some(project.as_str());
    let arguments = open_arguments(&command, file, line, workspace);
    spawn_launcher(&command, &arguments)
}
