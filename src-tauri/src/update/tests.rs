use super::manifest::{offer_from, Offer, Package, Version};
use super::swap::{delete_leftovers, install, suffixed, RETIRED_MARKER, STAGED_SUFFIX};
use super::Updater;

use std::path::PathBuf;

const RUNNING: Version = Version {
    major: 0,
    minor: 0,
    patch: 2,
};
const DIGEST: &str = "9698e2321e12f7c348f0be4c315a4fab8509a1b5d6babe81ae3d1f707ffdc3e6";

fn manifest(version: &str, url: &str, sha256: &str) -> String {
    format!(r#"{{"version":"{version}","url":"{url}","sha256":"{sha256}"}}"#)
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("pade-update-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("scratch folder");
    directory
}

fn digest_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    crate::util::lowercase_hex(&Sha256::digest(bytes))
}

#[test]
fn versions_order_the_way_releases_do() {
    let parsed = |text| Version::parse(text).expect("a version");
    assert!(parsed("0.0.10") > parsed("0.0.9"));
    assert!(parsed("0.1.0") > parsed("0.0.99"));
    assert_eq!(parsed("v1.2.3"), parsed("1.2.3"));
    assert!(Version::parse("1.2").is_none());
    assert!(Version::parse("1.2.3.4").is_none());
    assert_eq!(parsed("1.2.3").to_string(), "1.2.3");
}

#[test]
fn a_packaged_copy_is_updated_by_windows_whatever_its_build() {
    assert_eq!(Updater::of(true, true), Updater::Windows);
    assert_eq!(Updater::of(true, false), Updater::Windows);
    assert_eq!(Updater::of(false, true), Updater::Pade);
    assert_eq!(Updater::of(false, false), Updater::None);
}

#[test]
fn a_newer_checked_https_release_is_offered() {
    let url = "https://github.com/avi12/pade/releases/download/v0.0.3/PADE-0.0.3.exe";
    assert_eq!(
        offer_from(
            &manifest("0.0.3", url, &DIGEST.to_uppercase()),
            RUNNING,
            Package::Executable
        ),
        Some(Offer {
            version: Version::parse("0.0.3").expect("a version"),
            url: url.to_string(),
            sha256: DIGEST.to_string(),
        })
    );
}

#[test]
fn nothing_is_offered_without_a_newer_version_a_digest_or_a_safe_address() {
    let url = "https://example.com/PADE.exe";
    assert!(offer_from(
        &manifest("0.0.2", url, DIGEST),
        RUNNING,
        Package::Executable
    )
    .is_none());
    assert!(offer_from(&manifest("0.0.3", url, "abc"), RUNNING, Package::Executable).is_none());
    assert!(offer_from(
        &manifest("0.0.3", "http://example.com/PADE.exe", DIGEST),
        RUNNING,
        Package::Executable
    )
    .is_none());
    assert!(offer_from("not json", RUNNING, Package::Executable).is_none());
    // A stand-in server on this machine is accepted, for trying an update.
    assert!(offer_from(
        &manifest("0.0.3", "http://127.0.0.1:8000/PADE.exe", DIGEST),
        RUNNING,
        Package::Executable
    )
    .is_some());
}

#[test]
fn an_msi_install_is_offered_the_installer_and_only_when_one_is_published() {
    let installer_url =
        "https://github.com/avi12/pade/releases/download/v0.0.3/PADE-0.0.3-setup.msi";
    let with_installer = format!(
        r#"{{"version":"0.0.3","url":"https://example.com/PADE.exe","sha256":"{DIGEST}","installer":{{"url":"{installer_url}","sha256":"{DIGEST}"}}}}"#
    );
    assert_eq!(
        offer_from(&with_installer, RUNNING, Package::Installer).map(|offer| offer.url),
        Some(installer_url.to_string())
    );
    let without_installer = manifest("0.0.3", "https://example.com/PADE.exe", DIGEST);
    assert!(offer_from(&without_installer, RUNNING, Package::Installer).is_none());
    assert!(offer_from(&without_installer, RUNNING, Package::Executable).is_some());
}

#[test]
fn install_swaps_the_new_exe_in_and_sets_the_old_one_aside() {
    let folder = scratch("install");
    let running = folder.join("pade.exe");
    let downloaded = folder.join("PADE-0.0.3.exe");
    std::fs::write(&running, b"old").expect("running exe");
    std::fs::write(&downloaded, b"new").expect("downloaded exe");

    install(&downloaded, &running, &digest_of(b"new")).expect("installed");

    assert_eq!(std::fs::read(&running).expect("swapped exe"), b"new");
    let retired = std::fs::read_dir(&folder)
        .expect("folder")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().contains(RETIRED_MARKER))
        .count();
    assert_eq!(retired, 1);

    delete_leftovers(&running);
    let remaining: Vec<String> = std::fs::read_dir(&folder)
        .expect("folder")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!remaining.iter().any(|name| name.contains(RETIRED_MARKER)));
    assert!(!suffixed(&running, STAGED_SUFFIX).exists());
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn install_refuses_a_download_that_does_not_match_and_keeps_the_running_exe() {
    let folder = scratch("mismatch");
    let running = folder.join("pade.exe");
    let downloaded = folder.join("PADE-0.0.3.exe");
    std::fs::write(&running, b"old").expect("running exe");
    std::fs::write(&downloaded, b"tampered").expect("downloaded exe");

    assert!(install(&downloaded, &running, &digest_of(b"new")).is_err());
    assert_eq!(std::fs::read(&running).expect("running exe"), b"old");
    assert!(!suffixed(&running, STAGED_SUFFIX).exists());
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn an_exe_already_carrying_the_release_is_left_alone() {
    let folder = scratch("already");
    let running = folder.join("pade.exe");
    let downloaded = folder.join("PADE-0.0.3.exe");
    std::fs::write(&running, b"new").expect("running exe");
    std::fs::write(&downloaded, b"new").expect("downloaded exe");

    install(&downloaded, &running, &digest_of(b"new")).expect("nothing to do");
    let entries = std::fs::read_dir(&folder).expect("folder").count();
    assert_eq!(entries, 2);
    let _ = std::fs::remove_dir_all(folder);
}
