//! OS integrations - reveal a project in the system file manager or a terminal,
//! open a URL in the default browser, or restart PADE.

use crate::util;
use std::path::Path;
use std::process::Command;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// Restart the process so launch-only preferences, including `WebView2` browser
/// arguments, take effect. The normal exit event still tears down child PTYs.
#[tauri::command]
pub fn restart(app: AppHandle) {
    app.request_restart();
}

/// Open a URL in the default browser. Only http(s) schemes are allowed - this is
/// the one seam that hands a string to the OS, so we refuse anything that isn't a
/// plain web URL (no `file:`, `javascript:`, custom handlers, etc.).
///
/// Hands the URL to the platform opener (`ShellExecute` / `open` / `xdg-open`) via
/// the Tauri opener plugin, which passes it through untouched. Shelling out to
/// `cmd /C start` instead would let a `&` in the URL - every OAuth sign-in link
/// has several (`?code=true&client_id=…`) - split it into commands, so the browser
/// only ever received the URL up to the first `&`.
#[tauri::command]
pub async fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    let is_web_url = WEB_URL_SCHEMES.iter().any(|scheme| url.starts_with(scheme));
    if !is_web_url {
        return Err("only http(s) URLs may be opened".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// The only URL prefixes [`open_url`] hands to the OS.
const WEB_URL_SCHEMES: [&str; 2] = ["https://", "http://"];

/// The platform file manager's launcher: Explorer, Finder (via `open`), or the
/// desktop's default through `xdg-open`.
const FILE_MANAGER: &str = if cfg!(windows) {
    "explorer"
} else if cfg!(target_os = "macos") {
    "open"
} else {
    "xdg-open"
};

/// Open `path` in the platform file manager.
#[tauri::command]
pub async fn open_in_explorer(path: String) -> Result<(), String> {
    Command::new(FILE_MANAGER)
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Show `path` selected in the platform file manager without opening it - the
/// safe way to surface a binary, since an `.exe` handed to Explorer would run.
#[tauri::command]
pub async fn reveal_in_explorer(path: String) -> Result<(), String> {
    let target = Path::new(&path);
    if !target.exists() {
        return Err(format!("{path} no longer exists"));
    }
    reveal_command(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn reveal_command(target: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = util::command(FILE_MANAGER);
    // Explorer parses `/select,` itself and wants the path quoted after the
    // comma, which std's argument quoting cannot express - hence the raw arg.
    // It also rejects forward slashes there.
    let windows_path = target.display().to_string().replace('/', "\\");
    command.raw_arg(format!("/select,\"{windows_path}\""));
    command
}

#[cfg(target_os = "macos")]
fn reveal_command(target: &Path) -> Command {
    let mut command = util::command(FILE_MANAGER);
    command.arg("-R").arg(target);
    command
}

/// Linux file managers share no "select this file" flag, so open its folder.
#[cfg(all(unix, not(target_os = "macos")))]
fn reveal_command(target: &Path) -> Command {
    let mut command = util::command(FILE_MANAGER);
    command.arg(target.parent().unwrap_or(target));
    command
}

/// Open a terminal rooted at `path`. Prefers Windows Terminal, falling back to
/// the classic console; Terminal.app on macOS; `x-terminal-emulator` on Linux.
#[tauri::command]
pub async fn open_in_terminal(path: String) -> Result<(), String> {
    let directory = std::fs::canonicalize(&path).map_err(|e| e.to_string())?;
    if !directory.is_dir() {
        return Err("terminal path must be an existing directory".into());
    }
    let spawn = if cfg!(windows) {
        Command::new("wt")
            .arg("-d")
            .arg(&directory)
            .spawn()
            .or_else(|_| {
                Command::new("cmd")
                    .arg("/K")
                    .current_dir(&directory)
                    .spawn()
            })
    } else if cfg!(target_os = "macos") {
        Command::new("open")
            .args(["-a", "Terminal"])
            .arg(&directory)
            .spawn()
    } else {
        Command::new("x-terminal-emulator")
            .current_dir(&directory)
            .spawn()
    };
    spawn.map(|_| ()).map_err(|e| e.to_string())
}
