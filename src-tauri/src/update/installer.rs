//! A PADE installed by its MSI updates through the next release's MSI.
//!
//! The MSI puts PADE under Program Files, which a running copy can't write to,
//! and Windows keeps its own record of what the MSI installed. So the new
//! release's MSI is downloaded and checked like the portable exe, then run by
//! `msiexec` once PADE has closed - it asks for elevation itself. The upgrade
//! would otherwise lay the shortcuts down from the installer's defaults, so the
//! shortcuts the user kept or removed are passed along.

use std::path::{Path, PathBuf};

use super::AfterUpgrade;

/// Where the MSI records its folder for every user (`windows/installer.wxs`).
#[cfg(windows)]
const INSTALL_KEY: &str = r"HKLM\Software\avi12\PADE";
#[cfg(windows)]
const INSTALL_DIRECTORY_VALUE: &str = "InstallDir";
/// The shortcut name, the product name the MSI gives it.
const SHORTCUT_FILE: &str = "PADE.lnk";
/// The installer properties behind the Shortcuts page's two checkboxes.
const DESKTOP_SHORTCUT_PROPERTY: &str = "DESKTOPSHORTCUT";
const START_MENU_SHORTCUT_PROPERTY: &str = "STARTMENUSHORTCUT";
/// Starts PADE once the MSI has installed it (Tauri's template).
const AUTO_LAUNCH_PROPERTY: &str = "AUTOLAUNCHAPP";

/// Whether `executable` is the copy PADE's MSI installed.
#[cfg(windows)]
pub(super) fn is_installed_by_msi(executable: &Path) -> bool {
    let Some(folder) = executable.parent() else {
        return false;
    };
    crate::util::registry_string(INSTALL_KEY, INSTALL_DIRECTORY_VALUE)
        .is_some_and(|installed| crate::util::same_path(&installed, &folder.to_string_lossy()))
}

#[cfg(not(windows))]
pub(super) fn is_installed_by_msi(_executable: &Path) -> bool {
    false
}

/// Which of the MSI's shortcuts exist now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Shortcuts {
    is_on_desktop: bool,
    is_in_start_menu: bool,
}

/// A per-machine MSI puts its shortcuts on the all-users desktop and Start menu.
pub(super) fn kept_shortcuts() -> Shortcuts {
    let exists =
        |folder: Option<PathBuf>| folder.is_some_and(|folder| folder.join(SHORTCUT_FILE).exists());
    let desktop = std::env::var_os("PUBLIC").map(|public| PathBuf::from(public).join("Desktop"));
    let start_menu = std::env::var_os("ProgramData").map(|data| {
        PathBuf::from(data)
            .join(r"Microsoft\Windows\Start Menu\Programs")
            .join("PADE")
    });
    Shortcuts {
        is_on_desktop: exists(desktop),
        is_in_start_menu: exists(start_menu),
    }
}

/// An installer property set on (`1`) or off (empty).
fn property(name: &str, is_on: bool) -> String {
    let value = if is_on { "1" } else { "" };
    format!("{name}=\"{value}\"")
}

/// The PowerShell that runs the downloaded MSI without questions, keeping the
/// shortcuts as they were and starting PADE again when asked.
pub(super) fn upgrade_command(
    installer: &Path,
    shortcuts: Shortcuts,
    after: AfterUpgrade,
) -> String {
    let mut arguments = vec![
        "/i".to_string(),
        format!("\"{}\"", installer.display()),
        "/passive".to_string(),
        property(DESKTOP_SHORTCUT_PROPERTY, shortcuts.is_on_desktop),
        property(START_MENU_SHORTCUT_PROPERTY, shortcuts.is_in_start_menu),
    ];
    if after == AfterUpgrade::Relaunch {
        arguments.push(property(AUTO_LAUNCH_PROPERTY, true));
    }
    // One single-quoted PowerShell string, where a quote is written twice.
    let argument_list = arguments.join(" ").replace('\'', "''");
    format!("Start-Process msiexec.exe -Wait -ArgumentList '{argument_list}'")
}

#[cfg(test)]
mod tests {
    use super::{upgrade_command, AfterUpgrade, Shortcuts};
    use std::path::Path;

    #[test]
    fn the_upgrade_keeps_the_shortcuts_and_relaunches_only_when_asked() {
        let installer =
            Path::new(r"C:\Users\O'Neil\AppData\Local\pade\updates\PADE-0.0.4-setup.msi");
        let shortcuts = Shortcuts {
            is_on_desktop: false,
            is_in_start_menu: true,
        };
        let staying = upgrade_command(installer, shortcuts, AfterUpgrade::Stay);
        assert_eq!(
            staying,
            r#"Start-Process msiexec.exe -Wait -ArgumentList '/i "C:\Users\O''Neil\AppData\Local\pade\updates\PADE-0.0.4-setup.msi" /passive DESKTOPSHORTCUT="" STARTMENUSHORTCUT="1"'"#
        );
        assert!(
            upgrade_command(installer, shortcuts, AfterUpgrade::Relaunch)
                .ends_with(r#"AUTOLAUNCHAPP="1"'"#)
        );
    }
}
