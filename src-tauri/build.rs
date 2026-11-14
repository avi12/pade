//! Embeds the context-menu handler DLL when the release build names one.

use std::path::PathBuf;

/// The release build points this at the built `contextmenu_handler.dll`
/// (`scripts/release.mjs`), so the one exe carries the Windows 11
/// menu's handler and writes it out beside itself when the menu is turned on.
const CONTEXT_MENU_HANDLER_VARIABLE: &str = "PADE_CONTEXT_MENU_HANDLER";
const EMBEDDED_HANDLER_FILE: &str = "contextmenu_handler.dll";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo::rustc-check-cfg=cfg(embedded_context_menu_handler)");
    println!("cargo::rerun-if-env-changed={CONTEXT_MENU_HANDLER_VARIABLE}");
    if let Some(handler) = std::env::var_os(CONTEXT_MENU_HANDLER_VARIABLE) {
        let handler = PathBuf::from(handler);
        println!("cargo::rerun-if-changed={}", handler.display());
        let output = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("cargo sets OUT_DIR")?);
        std::fs::copy(&handler, output.join(EMBEDDED_HANDLER_FILE)).map_err(|error| {
            format!("the context-menu handler named by {CONTEXT_MENU_HANDLER_VARIABLE} is unreadable: {error}")
        })?;
        println!("cargo::rustc-cfg=embedded_context_menu_handler");
    }
    tauri_build::build();
    Ok(())
}
