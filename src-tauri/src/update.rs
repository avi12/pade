//! The "install the MSIX" notice for a copy that is not packaged.
//!
//! PADE ships only as a signed MSIX, which Windows keeps up to date through the
//! package's `.appinstaller` feed, so a packaged copy never starts the watch.
//! A published copy that is not packaged (one installed from the exe or MSI
//! PADE used to ship) cannot update itself any more: it reads a small manifest
//! on `main` (`docs/updates.json`, written by `scripts/release.mjs` once a
//! release's files are up) at start and every few hours, and when it names a
//! newer version tells the user to install the package from that release's
//! page. Nothing is downloaded, swapped or elevated. A development build never
//! nags, and the `autoUpdate` preference turns the check - and every
//! connection it makes - off.

mod manifest;
#[cfg(test)]
mod tests;

use self::manifest::{offer_from, Offer, Version};
use serde::Serialize;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

/// The manifest every published copy polls.
const FEED_URL: &str = "https://raw.githubusercontent.com/avi12/pade/main/docs/updates.json";

/// Points the check at a stand-in manifest, for trying the notice without a release.
const FEED_URL_VARIABLE: &str = "PADE_UPDATE_FEED";

/// The frontend-owned preference that turns the check off (opt-out).
const AUTO_UPDATE_PREFERENCE: &str = "autoUpdate";

/// Announced once a newer release is found.
const AVAILABLE_EVENT: &str = "update://available";

/// How often the watch looks for a release; `update_check` looks at once.
const CHECK_EVERY: Duration = Duration::from_hours(6);

/// After a failed check (no network at sign-in, GitHub down).
const RETRY_AFTER: Duration = Duration::from_mins(30);

/// The folder a checkout builds into - an exe under it is a development build.
const BUILD_DIRECTORY: &str = "target";

/// The watch's shared state: the newest release found so far.
#[derive(Default)]
pub struct UpdateState {
    newer: Mutex<Option<Offer>>,
}

impl UpdateState {
    fn newer(&self) -> Option<Offer> {
        self.newer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set_newer(&self, offer: Offer) {
        *self.newer.lock().unwrap_or_else(PoisonError::into_inner) = Some(offer);
    }
}

/// Who keeps this copy up to date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Updater {
    /// Nobody: a published exe that is not packaged. It is told where to get
    /// the package, which then updates through Windows.
    Unpackaged,
    /// Windows, through the package's `.appinstaller` feed - an MSIX install.
    Windows,
    /// Nobody, and no notice: a development build.
    None,
}

impl Updater {
    fn of(is_packaged: bool, is_published: bool) -> Self {
        if is_packaged {
            return Self::Windows;
        }
        if is_published {
            return Self::Unpackaged;
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

/// A newer release, as the frontend shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Newer {
    version: String,
    release_url: String,
}

/// What the frontend shows: the running version, a newer release if one was
/// found, and who updates this copy.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    current: String,
    newer: Option<Newer>,
    updater: Updater,
}

fn status(state: &UpdateState) -> UpdateStatus {
    UpdateStatus {
        current: env!("CARGO_PKG_VERSION").to_string(),
        newer: state.newer().map(|offer| Newer {
            version: offer.version.to_string(),
            release_url: offer.release_url,
        }),
        updater: Updater::running(),
    }
}

/// Whether this copy looks for a newer release to tell the user about.
fn is_noticing() -> bool {
    Updater::running() == Updater::Unpackaged
}

/// A release build that isn't sitting in a checkout's build folder.
fn is_published_executable() -> bool {
    let is_release_build = !cfg!(debug_assertions);
    let is_in_checkout = std::env::current_exe().is_ok_and(|path| {
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

/// Look for a release newer than both the running one and any already found.
/// Answers the release found, if any.
fn check(state: &UpdateState) -> Result<Option<Offer>, String> {
    let Some(current) = Version::running() else {
        return Ok(None);
    };
    let floor = state
        .newer()
        .map_or(current, |offer| offer.version.max(current));
    let manifest = ureq::get(&feed_url())
        .header("User-Agent", &user_agent())
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    let offer = offer_from(&manifest, floor);
    if let Some(offer) = &offer {
        state.set_newer(offer.clone());
    }
    Ok(offer)
}

/// Start the watch: check at start and every few hours while PADE runs.
pub fn init(app: &AppHandle) {
    let state = Arc::new(UpdateState::default());
    app.manage(Arc::clone(&state));
    if !is_noticing() {
        return;
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
            let _ = app.emit(AVAILABLE_EVENT, status(state));
            CHECK_EVERY
        }
        Ok(None) => CHECK_EVERY,
        Err(error) => {
            eprintln!("update check failed: {error}");
            RETRY_AFTER
        }
    }
}

/// The running version and any newer release found.
#[tauri::command]
pub fn update_status(state: State<'_, Arc<UpdateState>>) -> UpdateStatus {
    status(&state)
}

/// Check for a newer release now.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<UpdateStatus, String> {
    let state = Arc::clone(app.state::<Arc<UpdateState>>().inner());
    if !is_noticing() {
        return Ok(status(&state));
    }
    tauri::async_runtime::spawn_blocking(move || {
        if check(&state)?.is_some() {
            let _ = app.emit(AVAILABLE_EVENT, status(&state));
        }
        Ok(status(&state))
    })
    .await
    .map_err(|error| error.to_string())?
}
