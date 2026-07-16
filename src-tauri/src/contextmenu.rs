//! Windows Explorer "Open in PADE" integration — for BOTH right-click menus.
//!
//! - **Legacy menu** (`HKCU\Software\Classes\Directory\…\shell\PADE`): the classic
//!   menu, still shown on Windows 10 and under Windows 11's "Show more options".
//!   Per-user, no admin, plain registry keys pointing at the running executable
//!   with `%V` (the folder).
//! - **Modern menu** (Windows 11's first-shown menu): a *packaged* `IExplorerCommand`
//!   COM handler, which Windows only loads with package identity. Registered via a
//!   sparse MSIX manifest — see the [`modern`] submodule and the
//!   `contextmenu-handler` crate.
//!
//! Both open the folder through `launch_context`. The toggle in the project picker
//! turns them on/off together; the modern step can fail (e.g. Developer Mode off)
//! while the legacy step still succeeds, and that is surfaced to the UI.
//! Windows-only; other platforms get inert stubs so `lib.rs` compiles everywhere.

#[cfg(windows)]
mod modern;

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
    let out = crate::util::command("reg")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
}

/// Add the legacy folder + folder-background registry entries.
#[cfg(windows)]
fn register_legacy() -> Result<(), String> {
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

/// Remove the legacy registry entries.
#[cfg(windows)]
fn unregister_legacy() -> Result<(), String> {
    for root in ROOTS {
        reg(&["delete", root, "/f"])?;
    }
    Ok(())
}

/// Whether the legacy registry entry exists.
#[cfg(windows)]
fn legacy_registered() -> bool {
    crate::util::command("reg")
        .args(["query", ROOTS[0]])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Register "Open in PADE" for both menus. The legacy keys go in first (they always
/// work, no Developer Mode needed); then the modern packaged handler, whose failure
/// is returned to the UI but leaves the legacy menu in place.
#[cfg(windows)]
#[tauri::command]
pub fn context_menu_register() -> Result<(), String> {
    register_legacy()?;
    modern::register()
}

/// Remove both menus. Both steps run; the first error (if any) is returned.
#[cfg(windows)]
#[tauri::command]
pub fn context_menu_unregister() -> Result<(), String> {
    let modern = modern::unregister();
    let legacy = unregister_legacy();
    modern.and(legacy)
}

/// Whether "Open in PADE" is currently registered — in either menu. The legacy key
/// is checked first (a fast registry query); only if it is absent do we ask about
/// the modern package (a `PowerShell` call). Register/unregister keep the two in
/// lockstep, so this faithfully mirrors the toggle.
#[cfg(windows)]
#[tauri::command]
pub fn context_menu_status() -> bool {
    legacy_registered() || modern::is_registered()
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
