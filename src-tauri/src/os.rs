//! OS integrations — reveal a project in the system file manager or a terminal,
//! or open a URL in the default browser.

use std::process::Command;

/// Open a URL in the default browser. Only http(s) schemes are allowed — this is
/// the one seam that hands a string to the OS shell, so we refuse anything that
/// isn't a plain web URL (no `file:`, `javascript:`, custom handlers, etc.).
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    let scheme_ok = url.starts_with("https://") || url.starts_with("http://");
    if !scheme_ok {
        return Err("only http(s) URLs may be opened".into());
    }
    let result = if cfg!(windows) {
        // The empty "" is `start`'s title arg, so the URL isn't mistaken for one.
        Command::new("cmd").args(["/C", "start", "", &url]).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(&url).spawn()
    } else {
        Command::new("xdg-open").arg(&url).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Open `path` in the platform file manager (Explorer / Finder / xdg).
#[tauri::command]
pub fn open_in_explorer(path: String) -> Result<(), String> {
    let result = if cfg!(windows) {
        Command::new("explorer").arg(&path).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(&path).spawn()
    } else {
        Command::new("xdg-open").arg(&path).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Open a terminal rooted at `path`. Prefers Windows Terminal, falling back to
/// the classic console; Terminal.app on macOS; `x-terminal-emulator` on Linux.
#[tauri::command]
pub fn open_in_terminal(path: String) -> Result<(), String> {
    let spawn = if cfg!(windows) {
        Command::new("wt").args(["-d", &path]).spawn().or_else(|_| {
            Command::new("cmd")
                .args(["/C", "start", "cmd", "/K", "cd", "/D", &path])
                .spawn()
        })
    } else if cfg!(target_os = "macos") {
        Command::new("open").args(["-a", "Terminal", &path]).spawn()
    } else {
        Command::new("x-terminal-emulator")
            .current_dir(&path)
            .spawn()
    };
    spawn.map(|_| ()).map_err(|e| e.to_string())
}
