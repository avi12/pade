//! The Windows 11 **modern** context menu — registering PADE's sparse
//! (external-location) MSIX package so File Explorer loads the
//! `contextmenu_handler.dll` COM verb (`IExplorerCommand`). Dev-mode, unsigned,
//! per-user; additive to the legacy registry menu in the parent module.
//!
//! Register/unregister shell out to `Add-AppxPackage -Register … -ExternalLocation`
//! / `Remove-AppxPackage` — the documented way to (de)register a loose manifest,
//! and the same "no extra dependency" posture as the rest of the codebase (we do
//! not pull in the `WinRT` `PackageManager`). See the runbook in
//! `docs/handoff-windows11-context-menu.md`.

use std::path::{Path, PathBuf};
use std::process::Output;

use crate::util::command;

/// Package identity `Name` — must equal `<Identity Name>` in `AppxManifest.xml`.
/// Used to query and to remove the package.
const PACKAGE_NAME: &str = "PADE.ContextMenu";

/// The sparse manifest, embedded from its authoritative home in the handler crate.
/// [`EXECUTABLE_PLACEHOLDER`] is filled with the running exe's name before writing.
const MANIFEST_TEMPLATE: &str = include_str!("../../contextmenu-handler/AppxManifest.xml");

/// The token the template carries where the packaged executable's file name goes.
const EXECUTABLE_PLACEHOLDER: &str = "{{EXECUTABLE}}";

/// The handler DLL that must sit beside the executable — produced by
/// `cargo build -p contextmenu-handler` into the shared `target/` dir.
const HANDLER_DLL: &str = "contextmenu_handler.dll";

/// Logos the manifest references (relative to the external location). Embedded from
/// the app icons and written into `Assets\` at register time: `(file name, bytes)`.
/// The paths here must match the `Logo` / `Square*Logo` attributes in the manifest.
const ASSETS: &[(&str, &[u8])] = &[
    ("StoreLogo.png", include_bytes!("../../icons/StoreLogo.png")),
    (
        "Square150x150Logo.png",
        include_bytes!("../../icons/Square150x150Logo.png"),
    ),
    (
        "Square44x44Logo.png",
        include_bytes!("../../icons/Square44x44Logo.png"),
    ),
];

/// Register the sparse package so the modern menu appears. Assumes the caller has
/// already applied the legacy keys; a failure here (typically Developer Mode off)
/// is returned with a clear, user-facing message and leaves the legacy menu intact.
pub fn register() -> Result<(), String> {
    let location = external_location()?;
    let handler = location.join(HANDLER_DLL);
    if !handler.is_file() {
        return Err(format!(
            "the modern-menu handler is not next to PADE (expected {}). \
             Build it once with `cargo build -p contextmenu-handler` \
             (its DLL lands beside pade.exe), then try again.",
            handler.display()
        ));
    }
    materialize(&location)?;
    let script = format!(
        "Add-AppxPackage -Register {} -ExternalLocation {}",
        quote(&location.join("AppxManifest.xml")),
        quote(&location),
    );
    interpret(&powershell(&script)?)
}

/// Remove the sparse package and clean up the files we wrote. Idempotent — removing
/// a package that was never registered is a no-op success.
pub fn unregister() -> Result<(), String> {
    let script =
        format!("Get-AppxPackage -Name '{PACKAGE_NAME}' | Remove-AppxPackage -ErrorAction Stop");
    let result = interpret(&powershell(&script)?);
    if let Ok(location) = external_location() {
        let _ = std::fs::remove_file(location.join("AppxManifest.xml"));
        let _ = std::fs::remove_dir_all(location.join("Assets"));
    }
    result
}

/// Whether the sparse package is currently registered for this user.
pub fn is_registered() -> bool {
    let script =
        format!("if (Get-AppxPackage -Name '{PACKAGE_NAME}') {{ exit 0 }} else {{ exit 1 }}");
    powershell(&script).is_ok_and(|output| output.status.success())
}

/// The external-location directory: where PADE's own executable lives, and where
/// the handler DLL, the written manifest and the `Assets\` logos must all sit.
fn external_location() -> Result<PathBuf, String> {
    std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "cannot determine the executable's directory".to_string())
}

/// The running executable's file name (e.g. `pade.exe`), for the manifest's
/// `Application@Executable` — the file the sparse package's identity is granted to.
fn executable_name() -> Result<String, String> {
    std::env::current_exe()
        .map_err(|e| e.to_string())?
        .file_name()
        .and_then(|name| name.to_str())
        .map(String::from)
        .ok_or_else(|| "the executable file name is not valid UTF-8".to_string())
}

/// Write the filled-in manifest and the logo assets into the external location.
fn materialize(location: &Path) -> Result<(), String> {
    let manifest = MANIFEST_TEMPLATE.replace(EXECUTABLE_PLACEHOLDER, &executable_name()?);
    std::fs::write(location.join("AppxManifest.xml"), manifest).map_err(|e| e.to_string())?;
    let assets = location.join("Assets");
    std::fs::create_dir_all(&assets).map_err(|e| e.to_string())?;
    for (name, bytes) in ASSETS {
        std::fs::write(assets.join(name), bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Run a `PowerShell` one-liner with no profile, capturing its output. Goes through
/// `util::command` so no console window flashes.
fn powershell(script: &str) -> Result<Output, String> {
    command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|e| e.to_string())
}

/// Turn a captured `PowerShell` result into `Ok`/a user-facing `Err`, translating the
/// Developer-Mode-off failure (`0x80073CFF`) into actionable guidance.
fn interpret(output: &Output) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    let mut message = String::from_utf8_lossy(&output.stderr).into_owned();
    message.push_str(&String::from_utf8_lossy(&output.stdout));
    let developer_mode_off =
        message.contains("0x80073CFF") || message.to_lowercase().contains("developer");
    if developer_mode_off {
        return Err("The modern Windows 11 menu needs Developer Mode turned on \
             (Settings → System → For developers → Developer Mode). \
             The legacy right-click menu was still added."
            .to_string());
    }
    Err(format!(
        "registering the modern context menu failed: {}",
        message.trim()
    ))
}

/// Single-quote a path for a `PowerShell` command (doubling any embedded quote), so a
/// path with spaces is passed as one literal argument.
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}
