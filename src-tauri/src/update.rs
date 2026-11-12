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
//!
//! A copy installed through winget updates through winget instead (`winget`
//! submodule), so winget's own record of what it installed stays true. A copy
//! installed by PADE's MSI downloads the release's MSI the same checked way and
//! runs it once PADE has closed (`installer` submodule). A copy installed from
//! the release's `.msix` is Windows' to update, through the package's
//! `.appinstaller` feed, so the watch never starts there at all.

mod download;
mod installer;
mod manifest;
mod swap;
#[cfg(test)]
mod tests;
mod winget;

use self::download::download;
use self::manifest::{offer_from, Package, Version};
use self::swap::{delete_leftovers, install};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

/// The manifest every published copy polls.
const FEED_URL: &str = "https://raw.githubusercontent.com/avi12/pade/main/docs/updates.json";

/// Points the check at a stand-in manifest, for trying an update without a release.
const FEED_URL_VARIABLE: &str = "PADE_UPDATE_FEED";

/// The frontend-owned preference that turns the check off (opt-out).
const AUTO_UPDATE_PREFERENCE: &str = "autoUpdate";

/// Announced once a newer release is downloaded and checked.
const READY_EVENT: &str = "update://ready";

/// How often the watch looks for a release; `update_check` looks at once.
const CHECK_EVERY: Duration = Duration::from_hours(6);

/// After a failed check (no network at sign-in, GitHub down).
const RETRY_AFTER: Duration = Duration::from_mins(30);

const UPDATES_DIRECTORY: &str = "updates";

/// The folder a checkout builds into - an exe under it is never replaced.
const BUILD_DIRECTORY: &str = "target";

/// How a ready release reaches this copy.
#[derive(Debug, Clone)]
enum Delivery {
    /// A portable copy: the downloaded, checked exe swapped in for the running one.
    Download { file: PathBuf, sha256: String },
    /// An MSI install: the downloaded, checked MSI, run once PADE has closed.
    Installer { file: PathBuf },
    /// A winget install: `winget upgrade`, once PADE has closed.
    Winget,
}

impl Delivery {
    /// Whether the new version is installed by another program once this
    /// process has ended, rather than swapped in by PADE itself.
    fn is_installed_after_exit(&self) -> bool {
        matches!(self, Self::Installer { .. } | Self::Winget)
    }
}

/// What happens once the new version is installed after PADE has closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterUpgrade {
    /// PADE was closing anyway; the next start is the new version.
    Stay,
    /// The user asked to restart into the new version.
    Relaunch,
}

/// Hand an upgrade to a hidden PowerShell that outlives this process: it
/// waits for PADE to end, then runs `upgrade`.
fn upgrade_after_exit(upgrade: &str) -> Result<(), String> {
    let script = format!(
        "Wait-Process -Id {} -ErrorAction SilentlyContinue; {upgrade}",
        std::process::id()
    );
    crate::util::command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .spawn()
        .map(drop)
        .map_err(|error| error.to_string())
}

/// A newer release ready to install.
#[derive(Debug, Clone)]
struct Ready {
    version: Version,
    delivery: Delivery,
}

/// The watch's shared state: what is ready, and whether it was already handed
/// over (a restart applies it, then exits - which must not apply it again).
#[derive(Default)]
pub struct UpdateState {
    ready: Mutex<Option<Ready>>,
    is_applied: AtomicBool,
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

/// Who keeps this copy up to date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Updater {
    /// PADE itself: a published portable, MSI or winget copy.
    Pade,
    /// Windows, through the package's `.appinstaller` feed - an MSIX install,
    /// whose read-only folder PADE could not swap an exe in anyway.
    Windows,
    /// Nobody: a development build.
    None,
}

impl Updater {
    fn of(is_packaged: bool, is_published: bool) -> Self {
        if is_packaged {
            return Self::Windows;
        }
        if is_published {
            return Self::Pade;
        }
        Self::None
    }

    fn running() -> Self {
        Self::of(
            crate::platform::package::is_packaged(),
            is_published_executable(),
        )
    }
}

/// What the frontend shows: the running version, a ready newer one, and who
/// updates this copy.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    current: String,
    ready: Option<String>,
    updater: Updater,
}

fn status(state: &UpdateState) -> UpdateStatus {
    UpdateStatus {
        current: env!("CARGO_PKG_VERSION").to_string(),
        ready: state.ready().map(|ready| ready.version.to_string()),
        updater: Updater::running(),
    }
}

/// Whether PADE checks for, downloads and installs its own releases.
fn is_self_updating() -> bool {
    Updater::running() == Updater::Pade
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

fn is_winget_install() -> bool {
    running_executable().is_some_and(|path| winget::is_installed_by_winget(&path))
}

/// The release file this copy installs from.
fn package() -> Package {
    let is_msi_install =
        running_executable().is_some_and(|path| installer::is_installed_by_msi(&path));
    if is_msi_install {
        Package::Installer
    } else {
        Package::Executable
    }
}

/// Look for a release newer than both the running one and any already ready:
/// winget's repository for a winget install, the manifest otherwise (downloading
/// and checking the exe, or the MSI for an MSI install). Answers the release now
/// ready, if any.
fn check(state: &UpdateState) -> Result<Option<Ready>, String> {
    let Some(current) = Version::running() else {
        return Ok(None);
    };
    let floor = state
        .ready()
        .map_or(current, |ready| ready.version.max(current));
    let ready = if is_winget_install() {
        winget::newer_release(floor, &user_agent())?.map(|version| Ready {
            version,
            delivery: Delivery::Winget,
        })
    } else {
        downloaded_release(floor, package())?
    };
    if let Some(ready) = &ready {
        state.set_ready(ready.clone());
    }
    Ok(ready)
}

/// The manifest's release, its `package` downloaded and checked, when it is
/// newer than `floor`.
fn downloaded_release(floor: Version, package: Package) -> Result<Option<Ready>, String> {
    let manifest = ureq::get(&feed_url())
        .header("User-Agent", &user_agent())
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    let Some(offer) = offer_from(&manifest, floor, package) else {
        return Ok(None);
    };
    let file = download(&offer, &package.file_name(offer.version))?;
    let delivery = match package {
        Package::Executable => Delivery::Download {
            file,
            sha256: offer.sha256,
        },
        Package::Installer => Delivery::Installer { file },
    };
    Ok(Some(Ready {
        version: offer.version,
        delivery,
    }))
}

/// Install a ready release, once: swap the exe in for a portable copy, or run
/// the MSI or winget once this process has ended (then start PADE again when
/// `after` asks).
fn apply(state: &UpdateState, after: AfterUpgrade) -> Result<(), String> {
    let Some(ready) = state.ready() else {
        return Ok(());
    };
    if state.is_applied.load(Ordering::Acquire) {
        return Ok(());
    }
    match &ready.delivery {
        Delivery::Download { file, sha256 } => {
            let running = running_executable().ok_or("cannot find PADE's own exe")?;
            install(file, &running, sha256)?;
        }
        Delivery::Installer { file } => upgrade_after_exit(&installer::upgrade_command(
            file,
            installer::kept_shortcuts(),
            after,
        ))?,
        Delivery::Winget => upgrade_after_exit(&winget::upgrade_command(after))?,
    }
    state.is_applied.store(true, Ordering::Release);
    Ok(())
}

/// Install a ready release as PADE exits, so the next start is the new version.
pub fn apply_pending(app: &AppHandle) -> Result<(), String> {
    let Some(state) = app.try_state::<Arc<UpdateState>>() else {
        return Ok(());
    };
    apply(&state, AfterUpgrade::Stay)
}

/// Start the watch: clean up after an earlier swap, then check at start and
/// every few hours while PADE runs.
pub fn init(app: &AppHandle) {
    let state = Arc::new(UpdateState::default());
    app.manage(Arc::clone(&state));
    if !is_self_updating() {
        return;
    }
    if let Some(running) = running_executable() {
        delete_leftovers(&running);
    }
    let emitter = app.clone();
    std::thread::spawn(move || watch(&emitter, &state));
}

fn watch(app: &AppHandle, state: &UpdateState) {
    loop {
        std::thread::sleep(check_once(app, state));
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
    if !is_self_updating() {
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

/// Install the ready release and restart into it. A portable copy swaps its exe
/// and restarts; an MSI or winget install exits so the installer can replace
/// it, and the upgrade starts PADE again.
#[tauri::command]
pub async fn update_restart(app: AppHandle) -> Result<(), String> {
    let state = Arc::clone(app.state::<Arc<UpdateState>>().inner());
    let is_installed_after_exit = state
        .ready()
        .is_some_and(|ready| ready.delivery.is_installed_after_exit());
    tauri::async_runtime::spawn_blocking(move || apply(&state, AfterUpgrade::Relaunch))
        .await
        .map_err(|error| error.to_string())??;
    if is_installed_after_exit {
        app.exit(0);
    } else {
        app.request_restart();
    }
    Ok(())
}
