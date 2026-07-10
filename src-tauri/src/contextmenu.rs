//! Windows Explorer context-menu integration — "Open in PADE" on folders.
//!
//! Registers per-user under `HKCU\Software\Classes` (no admin needed) for both
//! right-clicking a folder and right-clicking inside a folder's empty space
//! (Background). The command points at the current executable and passes the
//! folder as `%V`, which `launch_context` opens as the project. Toggled from the
//! project picker. Windows-only; other platforms get inert stubs so `lib.rs`
//! compiles everywhere.

#[cfg(windows)]
const ROOTS: &[&str] = &[
    r"HKCU\Software\Classes\Directory\shell\PADE",
    r"HKCU\Software\Classes\Directory\Background\shell\PADE",
];

#[cfg(windows)]
fn exe_path() -> Result<String, String> {
    std::env::current_exe()
        .map_err(|e| e.to_string())?
        .to_str()
        .map(String::from)
        .ok_or_else(|| "executable path is not valid UTF-8".to_string())
}

#[cfg(windows)]
fn reg(args: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new("reg")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
}

/// Add the "Open in PADE" folder + folder-background context-menu entries,
/// pointing at the running executable.
#[cfg(windows)]
#[tauri::command]
pub fn context_menu_register() -> Result<(), String> {
    let exe = exe_path()?;
    let command = format!("\"{exe}\" \"%V\"");
    for root in ROOTS {
        reg(&["add", root, "/ve", "/d", "Open in PADE", "/f"])?;
        reg(&["add", root, "/v", "Icon", "/d", &exe, "/f"])?;
        let command_key = format!("{root}\\command");
        reg(&["add", &command_key, "/ve", "/d", &command, "/f"])?;
    }
    Ok(())
}

/// Remove the context-menu entries.
#[cfg(windows)]
#[tauri::command]
pub fn context_menu_unregister() -> Result<(), String> {
    for root in ROOTS {
        reg(&["delete", root, "/f"])?;
    }
    Ok(())
}

/// Whether the "Open in PADE" entry is currently registered.
#[cfg(windows)]
#[tauri::command]
pub fn context_menu_status() -> bool {
    std::process::Command::new("reg")
        .args(["query", ROOTS[0]])
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(not(windows))]
#[tauri::command]
pub fn context_menu_register() -> Result<(), String> {
    Err("the Explorer context menu is Windows-only".into())
}

#[cfg(not(windows))]
#[tauri::command]
pub fn context_menu_unregister() -> Result<(), String> {
    Err("the Explorer context menu is Windows-only".into())
}

#[cfg(not(windows))]
#[tauri::command]
pub fn context_menu_status() -> bool {
    false
}
