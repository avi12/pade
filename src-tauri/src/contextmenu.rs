//! Windows Explorer "Open in PADE" integration, tailored per Windows version so the
//! entry appears in exactly one right-click menu — never duplicated.
//!
//! - **Windows 11** (build 22000+): only the **modern** menu — a *packaged*
//!   `IExplorerCommand` COM handler shown in Explorer's first-level menu (see the
//!   [`modern`] submodule and the `contextmenu-handler` crate). The legacy keys are
//!   skipped so the entry doesn't *also* show under "Show more options".
//! - **Windows 10 and older**: only the **legacy** menu
//!   (`HKCU\Software\Classes\Directory\…\shell\PADE`) — the classic per-user
//!   registry entry pointing at the exe with `%V` (the folder), since there's no
//!   modern menu to host the verb (and it needs no Developer Mode).
//!
//! Both open the folder through `launch_context`. The project-picker toggle turns
//! the version-appropriate menu on; unregister removes either (and any leftover from
//! an earlier build that added both). The modern step can fail (Developer Mode off),
//! which is surfaced to the UI. Windows-only; other platforms get inert stubs so
//! `lib.rs` compiles everywhere.

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

/// Windows 11 is build 22000 or newer. Read the build from the registry — no extra
/// dependency, matching how the rest of this module talks to Windows. Anything we
/// can't read (or a lower build) is treated as Windows 10, whose only menu is legacy.
#[cfg(windows)]
fn is_windows_11() -> bool {
    let Ok(output) = crate::util::command("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "/v",
            "CurrentBuildNumber",
        ])
        .output()
    else {
        return false;
    };
    // The value line is "CurrentBuildNumber  REG_SZ  22631" — the lone numeric token.
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .filter_map(|token| token.parse::<u32>().ok())
        .next_back()
        .is_some_and(|build| build >= 22000)
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

/// Remove the legacy registry entries. A no-op when they're absent (e.g. on a
/// Windows 11 install that only ever registered the modern menu), so unregister can
/// safely clear both menus without erroring on the one that was never there.
#[cfg(windows)]
fn unregister_legacy() -> Result<(), String> {
    if !legacy_registered() {
        return Ok(());
    }
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

/// Register "Open in PADE" in the one menu that fits this Windows version: the modern
/// packaged handler on Windows 11 (its only first-level menu), or the legacy registry
/// keys on Windows 10 and older. Registering both would duplicate the entry on 11.
#[cfg(windows)]
#[tauri::command]
pub fn context_menu_register() -> Result<(), String> {
    if is_windows_11() {
        modern::register()
    } else {
        register_legacy()
    }
}

/// Remove both menus — whichever this version uses, plus any leftover from an earlier
/// build that added both. Each step is a no-op when its menu is absent; the first
/// real error (if any) is returned.
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
