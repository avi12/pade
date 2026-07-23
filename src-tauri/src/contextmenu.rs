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
//! Both open the folder through `launch_context`. The project-picker toggle turns the
//! version-appropriate menu on and off: on Windows 11 the packaged handler is
//! registered once and thereafter shown/hidden via a flag it reads on every menu build
//! (no Explorer restart, à la `PowerToys`); on Windows 10 the legacy keys are added and
//! removed directly. Registering the modern package can fail (Developer Mode off),
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

/// The show/hide flag the modern handler reads on every menu build (see the
/// `contextmenu-handler` crate). Flipping it hides/shows "Open in PADE" at once —
/// even for a handler Explorer still has cached — with no Explorer restart, the way
/// `PowerToys` toggles its context menus. `1`/absent = shown, `0` = hidden.
#[cfg(windows)]
const MENU_FLAG_KEY: &str = r"HKCU\Software\PADE";
#[cfg(windows)]
const MENU_FLAG_VALUE: &str = "ContextMenu";

#[cfg(windows)]
fn set_menu_shown(shown: bool) -> Result<(), String> {
    reg(&[
        "add",
        MENU_FLAG_KEY,
        "/v",
        MENU_FLAG_VALUE,
        "/t",
        "REG_DWORD",
        "/d",
        if shown { "1" } else { "0" },
        "/f",
    ])
}

/// Read the show/hide flag the same way the handler does (see the `contextmenu-handler`
/// crate): an explicit `0` means hidden; absent, unreadable, or any other value means
/// shown. Lets `context_menu_status` report a registered-but-hidden package as off.
#[cfg(windows)]
fn menu_shown() -> bool {
    let Ok(output) = crate::util::command("reg")
        .args(["query", MENU_FLAG_KEY, "/v", MENU_FLAG_VALUE])
        .output()
    else {
        return true;
    };
    if !output.status.success() {
        return true;
    }
    // The value line ends in the DWORD as hex ("… REG_DWORD    0x0"); only 0 is hidden.
    let is_hidden = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next_back()
        .and_then(|token| token.strip_prefix("0x"))
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .is_some_and(|value| value == 0);
    !is_hidden
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

/// Turn "Open in PADE" on for the one menu that fits this Windows version.
///
/// - **Windows 11**: register the sparse package **once** (skipped when it is already
///   registered — re-enabling is then just a flag flip, never a redeploy), clear any
///   legacy leftover, and set the show/hide flag to shown. Mirrors `PowerToys`, which
///   registers its packaged handler once and thereafter only toggles a flag.
/// - **Windows 10 and older**: add the legacy registry keys (there is no packaged
///   handler to host the verb).
#[cfg(windows)]
#[tauri::command]
pub async fn context_menu_register() -> Result<(), String> {
    if !is_windows_11() {
        return register_legacy();
    }
    let _ = unregister_legacy();
    if !modern::is_registered() {
        modern::register()?;
    }
    set_menu_shown(true)
}

/// Turn "Open in PADE" off.
///
/// - **Windows 11**: only flip the handler's show/hide flag to hidden. The sparse
///   package stays registered, so the handler self-hides on the next menu build with no
///   Explorer restart and no redeploy — exactly how `PowerToys`' File Locksmith /
///   `PowerRename` keep their handler registered and just hide it. Fully removing the
///   package is reserved for a future uninstall path (`modern::unregister`).
/// - **Windows 10 and older**: remove the legacy registry keys outright (no
///   self-hiding handler exists there).
#[cfg(windows)]
#[tauri::command]
pub async fn context_menu_unregister() -> Result<(), String> {
    if !is_windows_11() {
        return unregister_legacy();
    }
    set_menu_shown(false)
}

/// Whether "Open in PADE" is currently on.
///
/// - **Windows 11**: the sparse package is registered **and** the show/hide flag is not
///   off (a registered-but-hidden package reads as off, matching the toggle).
/// - **Windows 10 and older**: whether the legacy keys exist.
#[cfg(windows)]
#[tauri::command]
pub async fn context_menu_status() -> bool {
    if !is_windows_11() {
        return legacy_registered();
    }
    modern::is_registered() && menu_shown()
}

#[cfg(not(windows))]
#[tauri::command]
pub async fn context_menu_register() -> Result<(), String> {
    Err("the Explorer context menu is Windows-only".into())
}

#[cfg(not(windows))]
#[tauri::command]
pub async fn context_menu_unregister() -> Result<(), String> {
    Err("the Explorer context menu is Windows-only".into())
}

#[cfg(not(windows))]
#[tauri::command]
pub async fn context_menu_status() -> bool {
    false
}
