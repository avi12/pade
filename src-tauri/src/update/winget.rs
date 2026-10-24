//! A PADE installed through winget updates through winget.
//!
//! winget keeps its own record of what it installed, so swapping the exe behind
//! its back would leave `winget list` naming a version that isn't there, and the
//! next `winget upgrade` fetching one that already is. So a winget copy asks the
//! same place winget does - the `microsoft/winget-pkgs` repository, where a
//! release appears once its manifest is merged, often a day or two after the
//! GitHub release - and when a newer version is listed, `winget upgrade` installs
//! it the moment PADE has closed (an exe can't be replaced while it runs).

use std::path::{Component, Path};

use serde::Deserialize;

use super::{AfterUpgrade, Version};

/// PADE's identifier in the winget repository.
const PACKAGE_IDENTIFIER: &str = "avi12.PADE";
/// The command winget puts on `PATH` for PADE (the manifest's `Commands`).
const COMMAND_ALIAS: &str = "pade";
/// Where winget-pkgs keeps one folder per published version of PADE.
const VERSIONS_URL: &str =
    "https://api.github.com/repos/microsoft/winget-pkgs/contents/manifests/a/avi12/PADE";
/// winget unpacks a portable package under `…\WinGet\Packages\<id>_<source>\`,
/// per user (`%LOCALAPPDATA%\Microsoft`) or per machine (`%ProgramFiles%`).
const PACKAGES_PATH: [&str; 2] = ["WinGet", "Packages"];
const DIRECTORY_KIND: &str = "dir";

/// Whether `executable` is a copy winget installed.
pub(super) fn is_installed_by_winget(executable: &Path) -> bool {
    let names: Vec<String> = executable
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    names.windows(PACKAGES_PATH.len()).any(|pair| {
        pair.iter()
            .zip(PACKAGES_PATH)
            .all(|(name, expected)| name.eq_ignore_ascii_case(expected))
    })
}

/// One entry of a GitHub folder listing.
#[derive(Deserialize)]
struct ListedEntry {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

/// The newest version a winget-pkgs folder listing names.
fn newest_listed(listing: &str) -> Option<Version> {
    let entries: Vec<ListedEntry> = serde_json::from_str(listing).ok()?;
    entries
        .iter()
        .filter(|entry| entry.kind == DIRECTORY_KIND)
        .filter_map(|entry| Version::parse(&entry.name))
        .max()
}

/// The newest version winget offers, when it is newer than `floor`.
pub(super) fn newer_release(floor: Version, user_agent: &str) -> Result<Option<Version>, String> {
    let listing = ureq::get(VERSIONS_URL)
        .header("User-Agent", user_agent)
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    Ok(newest_listed(&listing).filter(|newest| *newest > floor))
}

/// The PowerShell that has winget upgrade PADE and starts it again when asked.
pub(super) fn upgrade_command(after: AfterUpgrade) -> String {
    let relaunch = match after {
        AfterUpgrade::Stay => String::new(),
        AfterUpgrade::Relaunch => format!("; Start-Process {COMMAND_ALIAS}"),
    };
    format!(
        "winget upgrade --id {PACKAGE_IDENTIFIER} --exact --silent --disable-interactivity \
         --accept-package-agreements --accept-source-agreements{relaunch}"
    )
}

#[cfg(test)]
mod tests {
    use super::{is_installed_by_winget, newest_listed, upgrade_command, AfterUpgrade};
    use crate::update::Version;
    use std::path::Path;

    #[test]
    fn a_copy_under_winget_packages_is_a_winget_install() {
        assert!(is_installed_by_winget(Path::new(
            r"C:\Users\Avi\AppData\Local\Microsoft\WinGet\Packages\avi12.PADE_Microsoft.Winget.Source_8wekyb3d8bbwe\PADE-0.0.3.exe"
        )));
        assert!(is_installed_by_winget(Path::new(
            r"C:\Program Files\WinGet\Packages\avi12.PADE_Microsoft.Winget.Source_8wekyb3d8bbwe\PADE-0.0.3.exe"
        )));
        assert!(!is_installed_by_winget(Path::new(
            r"C:\Users\Avi\Downloads\PADE-0.0.3.exe"
        )));
        assert!(!is_installed_by_winget(Path::new(
            r"C:\Tools\Packages\WinGet\PADE.exe"
        )));
    }

    #[test]
    fn the_newest_listed_version_folder_wins() {
        let listing = r#"[
            {"name": "0.0.2", "type": "dir"},
            {"name": "0.0.10", "type": "dir"},
            {"name": "0.0.9", "type": "dir"},
            {"name": "README.md", "type": "file"}
        ]"#;
        assert_eq!(newest_listed(listing), Version::parse("0.0.10"));
        assert_eq!(newest_listed("[]"), None);
        assert_eq!(newest_listed("not json"), None);
    }

    #[test]
    fn the_upgrade_relaunches_only_when_asked() {
        let staying = upgrade_command(AfterUpgrade::Stay);
        assert!(staying.starts_with("winget upgrade --id avi12.PADE --exact"));
        assert!(!staying.contains("Start-Process"));
        assert!(upgrade_command(AfterUpgrade::Relaunch).ends_with("Start-Process pade"));
    }
}
