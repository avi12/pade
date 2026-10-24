//! Self-update from GitHub releases.
//!
//! The repository hosts a small manifest on `main` (`docs/updates.json`, written
//! by `scripts/release.mjs` once a release's exe is up): the latest version,
//! where its exe is, and the exe's SHA-256. A published PADE reads it at start
//! and every few hours, the way a browser extension polls its update URL, and
//! when it names a newer version downloads the exe in the background and checks
//! it against the digest. Nothing is offered without a digest - a download that
//! cannot be checked is never swapped in for the running exe.
//!
//! A running exe cannot be overwritten but can be renamed, so the swap renames
//! the running exe aside and puts the new one under its name; the next start
//! deletes what was set aside. The swap happens as PADE exits (the next start is
//! the new version) or at once, with a restart, when the user asks. Only a
//! published exe updates itself: a debug build or one run from a checkout's
//! `target` folder never does, and the `autoUpdate` preference turns the check -
//! and every connection it makes - off.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};

/// The manifest every published copy polls.
const FEED_URL: &str = "https://raw.githubusercontent.com/avi12/pade/main/docs/updates.json";
/// Points the check at a stand-in manifest, for trying an update without a release.
const FEED_URL_VARIABLE: &str = "PADE_UPDATE_FEED";
/// The frontend-owned preference that turns the check off (opt-out).
const AUTO_UPDATE_PREFERENCE: &str = "autoUpdate";
/// Announced once a newer release is downloaded and checked.
const READY_EVENT: &str = "update://ready";

/// How often the watch wakes; it only goes to the network every `CHECK_EVERY`,
/// but a requested check is seen within a tick.
const TICK: Duration = Duration::from_secs(60);
const CHECK_EVERY: Duration = Duration::from_hours(6);
/// After a failed check (no network at sign-in, GitHub down).
const RETRY_AFTER: Duration = Duration::from_mins(30);

const UPDATES_DIRECTORY: &str = "updates";
const DOWNLOAD_SUFFIX: &str = ".part";
const STAGED_SUFFIX: &str = ".new";
const RETIRED_MARKER: &str = ".old-";
/// The folder a checkout builds into - an exe under it is never replaced.
const BUILD_DIRECTORY: &str = "target";
const SHA256_HEX_LENGTH: usize = 64;
const HASH_CHUNK_BYTES: usize = 64 * 1024;
const HTTPS_SCHEME: &str = "https://";
const LOOPBACK_PREFIXES: [&str; 2] = ["http://127.0.0.1", "http://localhost"];

/// A release version, `major.minor.patch`, ordered the way releases are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().trim_start_matches('v').split('.');
        let version = Self {
            major: parts.next()?.parse().ok()?,
            minor: parts.next()?.parse().ok()?,
            patch: parts.next()?.parse().ok()?,
        };
        parts.next().is_none().then_some(version)
    }

    fn running() -> Option<Self> {
        Self::parse(env!("CARGO_PKG_VERSION"))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The manifest as the repository hosts it.
#[derive(Deserialize)]
struct Manifest {
    version: String,
    url: String,
    sha256: String,
}

/// A release newer than the running one: the exe to fetch and its digest.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Offer {
    version: Version,
    url: String,
    sha256: String,
}

/// The offer a manifest makes to a copy running `current` - none unless it names
/// a newer version, a full SHA-256 and an https address (or this machine's, for
/// a test's stand-in server).
fn offer_from(manifest: &str, current: Version) -> Option<Offer> {
    let manifest: Manifest = serde_json::from_str(manifest).ok()?;
    let version = Version::parse(&manifest.version)?;
    let has_digest = manifest.sha256.len() == SHA256_HEX_LENGTH
        && manifest
            .sha256
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    let is_trusted_address = manifest.url.starts_with(HTTPS_SCHEME)
        || LOOPBACK_PREFIXES
            .iter()
            .any(|prefix| manifest.url.starts_with(prefix));
    let is_offered = version > current && has_digest && is_trusted_address;
    is_offered.then(|| Offer {
        version,
        url: manifest.url,
        sha256: manifest.sha256.to_ascii_lowercase(),
    })
}

/// A newer release downloaded and checked, waiting to be swapped in.
#[derive(Debug, Clone)]
struct Ready {
    offer: Offer,
    file: PathBuf,
}

/// The watch's shared state: what is ready, and whether a check was asked for.
#[derive(Default)]
pub struct UpdateState {
    ready: Mutex<Option<Ready>>,
    is_check_requested: AtomicBool,
}

impl UpdateState {
    fn ready(&self) -> Option<Ready> {
        self.ready
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set_ready(&self, ready: Ready) {
        *self.ready.lock().unwrap_or_else(PoisonError::into_inner) = Some(ready);
    }
}

/// What the frontend shows: the running version, a ready newer one, and whether
/// this copy updates itself at all.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    current: String,
    ready: Option<String>,
    is_supported: bool,
}

fn status(state: &UpdateState) -> UpdateStatus {
    UpdateStatus {
        current: env!("CARGO_PKG_VERSION").to_string(),
        ready: state.ready().map(|ready| ready.offer.version.to_string()),
        is_supported: is_published_executable(),
    }
}

fn running_executable() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

/// A release build that isn't sitting in a checkout's build folder.
fn is_published_executable() -> bool {
    let is_release_build = !cfg!(debug_assertions);
    let is_in_checkout = running_executable().is_some_and(|path| {
        path.components()
            .any(|component| component.as_os_str() == BUILD_DIRECTORY)
    });
    is_release_build && !is_in_checkout
}

fn is_wanted() -> bool {
    let is_turned_off = crate::workspace::load()
        .prefs
        .passthrough
        .get(AUTO_UPDATE_PREFERENCE)
        == Some(&serde_json::Value::Bool(false));
    !is_turned_off
}

fn feed_url() -> String {
    std::env::var(FEED_URL_VARIABLE)
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| FEED_URL.to_string())
}

fn user_agent() -> String {
    format!("PADE/{}", env!("CARGO_PKG_VERSION"))
}

fn updates_directory() -> Option<PathBuf> {
    Some(crate::util::pade_data_directory()?.join(UPDATES_DIRECTORY))
}

/// The SHA-256 of a file, as lowercase hex, read in chunks.
fn file_sha256(path: &Path) -> std::io::Result<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; HASH_CHUNK_BYTES];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(crate::util::lowercase_hex(&hasher.finalize()))
}

fn is_digest_of(path: &Path, sha256: &str) -> bool {
    file_sha256(path).is_ok_and(|actual| actual == sha256)
}

/// Read the manifest and, when it offers something newer than what is ready,
/// download and check it. Answers the release now ready, if any.
fn check(state: &UpdateState) -> Result<Option<Ready>, String> {
    let Some(current) = Version::running() else {
        return Ok(None);
    };
    let manifest = ureq::get(&feed_url())
        .header("User-Agent", &user_agent())
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    let Some(offer) = offer_from(&manifest, current) else {
        return Ok(None);
    };
    let is_already_ready = state
        .ready()
        .is_some_and(|ready| ready.offer.version >= offer.version);
    if is_already_ready {
        return Ok(None);
    }
    let file = download(&offer)?;
    let ready = Ready { offer, file };
    state.set_ready(ready.clone());
    Ok(Some(ready))
}

/// Fetch the offered exe into PADE's updates folder, checked against its digest
/// before it is kept. A download from an earlier run that still matches is used
/// as it is; other leftovers are removed.
fn download(offer: &Offer) -> Result<PathBuf, String> {
    let directory = updates_directory().ok_or("no local data folder")?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let target = directory.join(format!("PADE-{}.exe", offer.version));
    if is_digest_of(&target, &offer.sha256) {
        return Ok(target);
    }
    // Several PADE windows can run as separate processes; each downloads into
    // its own partial file so two never write one.
    let partial = directory.join(format!(
        "PADE-{}.exe{DOWNLOAD_SUFFIX}-{}",
        offer.version,
        std::process::id()
    ));
    let response = ureq::get(&offer.url)
        .header("User-Agent", &user_agent())
        .call()
        .map_err(|error| error.to_string())?;
    let mut reader = response.into_body().into_reader();
    let mut destination = File::create(&partial).map_err(|error| error.to_string())?;
    std::io::copy(&mut reader, &mut destination).map_err(|error| error.to_string())?;
    drop(destination);
    if !is_digest_of(&partial, &offer.sha256) {
        let _ = std::fs::remove_file(&partial);
        return Err("the downloaded update does not match its digest".into());
    }
    std::fs::rename(&partial, &target).map_err(|error| error.to_string())?;
    delete_other_downloads(&directory, &target);
    Ok(target)
}

fn delete_other_downloads(directory: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path != keep {
            // A file another PADE is still writing stays; it is cleaned next time.
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Swap `downloaded` in for `running`. The new exe is copied beside the running
/// one first, so the swap itself is a rename on one volume and a copy cut short
/// never leaves half an exe under PADE's name; on failure the running exe is
/// put back. A running exe already carrying the digest (another PADE window
/// swapped first) is left as it is.
fn install(downloaded: &Path, running: &Path, sha256: &str) -> Result<(), String> {
    if is_digest_of(running, sha256) {
        return Ok(());
    }
    let staged = suffixed(running, STAGED_SUFFIX);
    std::fs::copy(downloaded, &staged).map_err(|error| error.to_string())?;
    if !is_digest_of(&staged, sha256) {
        let _ = std::fs::remove_file(&staged);
        return Err("the staged update does not match its digest".into());
    }
    let retired = suffixed(
        running,
        &format!("{RETIRED_MARKER}{}", crate::util::now_milliseconds()),
    );
    std::fs::rename(running, &retired).map_err(|error| error.to_string())?;
    if let Err(error) = std::fs::rename(&staged, running) {
        let _ = std::fs::rename(&retired, running);
        return Err(error.to_string());
    }
    Ok(())
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Delete what an earlier swap set aside. After a restart the retired exe no
/// longer runs; one another PADE window still runs stays until a later start.
fn delete_leftovers(running: &Path) {
    let (Some(folder), Some(name)) = (running.parent(), running.file_name()) else {
        return;
    };
    let name = name.to_string_lossy();
    let retired_prefix = format!("{name}{RETIRED_MARKER}");
    let staged_name = format!("{name}{STAGED_SUFFIX}");
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let is_leftover = file_name.starts_with(&retired_prefix) || file_name == staged_name;
        if is_leftover {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Swap in a ready release, if there is one. Called as PADE exits and before a
/// restart into the new version.
pub fn apply_pending(app: &AppHandle) -> Result<(), String> {
    let Some(state) = app.try_state::<Arc<UpdateState>>() else {
        return Ok(());
    };
    let (Some(ready), Some(running)) = (state.ready(), running_executable()) else {
        return Ok(());
    };
    install(&ready.file, &running, &ready.offer.sha256)
}

/// Start the watch: clean up after an earlier swap, then check at start and
/// every few hours while PADE runs.
pub fn init(app: &AppHandle) {
    let state = Arc::new(UpdateState::default());
    app.manage(Arc::clone(&state));
    if !is_published_executable() {
        return;
    }
    if let Some(running) = running_executable() {
        delete_leftovers(&running);
    }
    let emitter = app.clone();
    std::thread::spawn(move || watch(&emitter, &state));
}

fn watch(app: &AppHandle, state: &UpdateState) {
    let mut next_check = Instant::now();
    loop {
        let is_requested = state.is_check_requested.swap(false, Ordering::Relaxed);
        let is_due = is_requested || Instant::now() >= next_check;
        if is_due {
            next_check = Instant::now() + check_once(app, state);
        }
        std::thread::sleep(TICK);
    }
}

/// One scheduled check; answers how long until the next.
fn check_once(app: &AppHandle, state: &UpdateState) -> Duration {
    if !is_wanted() {
        return CHECK_EVERY;
    }
    match check(state) {
        Ok(Some(_)) => {
            let _ = app.emit(READY_EVENT, status(state));
            CHECK_EVERY
        }
        Ok(None) => CHECK_EVERY,
        Err(error) => {
            eprintln!("update check failed: {error}");
            RETRY_AFTER
        }
    }
}

/// The running version and any release ready to swap in.
#[tauri::command]
pub fn update_status(state: State<'_, Arc<UpdateState>>) -> UpdateStatus {
    status(&state)
}

/// Check for a newer release now, waiting for the download to finish.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<UpdateStatus, String> {
    let state = Arc::clone(app.state::<Arc<UpdateState>>().inner());
    if !is_published_executable() {
        return Ok(status(&state));
    }
    tauri::async_runtime::spawn_blocking(move || {
        if check(&state)?.is_some() {
            let _ = app.emit(READY_EVENT, status(&state));
        }
        Ok(status(&state))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Swap the ready release in and restart into it.
#[tauri::command]
pub async fn update_restart(app: AppHandle) -> Result<(), String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || apply_pending(&handle))
        .await
        .map_err(|error| error.to_string())??;
    app.request_restart();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        delete_leftovers, install, offer_from, suffixed, Offer, Version, RETIRED_MARKER,
        STAGED_SUFFIX,
    };
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
        let directory =
            std::env::temp_dir().join(format!("pade-update-{name}-{}", std::process::id()));
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
    fn a_newer_checked_https_release_is_offered() {
        let url = "https://github.com/avi12/pade/releases/download/v0.0.3/PADE-0.0.3.exe";
        assert_eq!(
            offer_from(&manifest("0.0.3", url, &DIGEST.to_uppercase()), RUNNING),
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
        assert!(offer_from(&manifest("0.0.2", url, DIGEST), RUNNING).is_none());
        assert!(offer_from(&manifest("0.0.3", url, "abc"), RUNNING).is_none());
        assert!(offer_from(
            &manifest("0.0.3", "http://example.com/PADE.exe", DIGEST),
            RUNNING
        )
        .is_none());
        assert!(offer_from("not json", RUNNING).is_none());
        // A stand-in server on this machine is accepted, for trying an update.
        assert!(offer_from(
            &manifest("0.0.3", "http://127.0.0.1:8000/PADE.exe", DIGEST),
            RUNNING
        )
        .is_some());
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
}
