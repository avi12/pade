//! The settings store: the persisted `Settings` shapes, the config directory, the locked read-modify-write `update_settings` and the change broadcast.

use super::roots::canonical_dedup;
use super::settings_sync;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::sync::OnceLock;
use tauri::AppHandle;
use tauri::Emitter;

pub(super) static SETTINGS_REPOSITORY: SettingsRepository = SettingsRepository {
    lock: Mutex::new(()),
    path: None,
};

pub(super) static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Broadcast that the persisted settings moved, so every window re-reads them -
/// the one home for the event name the frontend `workspace.onChanged` listens on.
pub(super) const SETTINGS_CHANGED_EVENT: &str = "settings://changed";

/// The handle the settings broadcast emits through. Settings are process-wide but
/// each webview holds its own copy, so a mutation made in one window has to reach
/// the others; `update_settings` is not a command and has no window of its own to
/// emit from, hence this one handle, planted at startup by [`init`].
pub(super) static APP: OnceLock<AppHandle> = OnceLock::new();

/// Hand this module the app handle it broadcasts settings changes through, and
/// start passing on the settings writes other PADE processes make.
pub(crate) fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let watched = settings_path().and_then(|path| {
        settings_sync::watch(&path, broadcast_settings_changed).map_err(|error| error.to_string())
    });
    if let Err(error) = watched {
        eprintln!("[pade] settings from other PADE instances won't sync live: {error}");
    }
}

/// PADE's directory under the OS config base, and the `ade` one it replaced.
pub(super) const CONFIG_DIRECTORY: &str = "pade";

pub(super) const LEGACY_CONFIG_DIRECTORY: &str = "ade";

/// The settings file inside [`CONFIG_DIRECTORY`].
pub(super) const SETTINGS_FILE: &str = "settings.json";

/// The folder inside [`CONFIG_DIRECTORY`] every temp workspace is created in,
/// and the name prefix each one carries.
pub(super) const WORKSPACES_DIRECTORY: &str = "workspaces";

pub(super) const TEMP_WORKSPACE_PREFIX: &str = "temp-";

/// What marks a directory as a git checkout.
pub(super) const GIT_DIRECTORY: &str = ".git";

/// Files/dirs that mark a directory as a project worth listing.
pub(super) const PROJECT_MARKERS: &[&str] = &[
    GIT_DIRECTORY,
    ".hg",
    ".jj",
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "CLAUDE.md",
    "AGENTS.md",
];

/// An editor the user located by executable path - first-class alongside the
/// PATH-detected ones. `command` for launching is the absolute `path`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AddedEditor {
    /// Stable, unique id in the merged editor list (e.g. `added-code`).
    pub id: String,
    /// Human label ("VS Code").
    pub label: String,
    /// Absolute path to the executable PADE launches.
    pub path: String,
}

/// The preferences ADE's own Rust code reads. Every other, frontend-owned
/// preference (theme, fonts, `uiScale`, diff style, start mode, auto-name /
/// auto-handoff, …) is defined once in the TS zod `Prefs` schema and round-trips
/// verbatim through `passthrough` - so a new UI-only pref never means editing
/// this struct, and Rust never duplicates a type the frontend already owns.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Prefs {
    /// Editor-rules engine: project-kind → IDE id. When a project's primary kind
    /// matches a key here, that IDE is suggested first (if installed).
    #[serde(default)]
    pub ide_rules: BTreeMap<String, String>,
    /// IDE id used when no `ide_rules` entry matches the project kind.
    #[serde(default)]
    pub ide_fallback: Option<String>,
    /// Explicit per-project editor picks - canonical project path → IDE id. A
    /// pick from the workspace's editor menu lands here and outranks every
    /// suggestion rule for that project (`ide_suggest` puts it first).
    #[serde(default)]
    pub ide_project_choices: BTreeMap<String, String>,
    /// Editors the user located by executable path (not auto-detected on PATH).
    /// Merged into the detected editor list so they show up in every menu.
    #[serde(default)]
    pub added_editors: Vec<AddedEditor>,
    /// Delete a temp workspace once it has gone unused for this many days;
    /// absent keeps every temp workspace until the user removes it.
    #[serde(default)]
    pub temp_cleanup_days: Option<u32>,
    /// Flag an agent chat once it grows past this many megabytes, for the user
    /// to tick for deletion; absent sets no size cap (`agents::storage`).
    #[serde(default)]
    pub chat_size_limit_megabytes: Option<u32>,
    /// Flag the oldest chats of each project in turn once all agent chats
    /// together pass this many megabytes; absent sets no total.
    #[serde(default)]
    pub chat_storage_limit_megabytes: Option<u32>,
    /// Frontend-owned preferences Rust never acts on, kept verbatim so they
    /// survive a load/save round-trip. `flatten` captures every key not named
    /// above; the TS zod schema is their single source of truth.
    #[serde(flatten)]
    pub passthrough: BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Settings {
    /// Root directories the user has added to browse for projects.
    pub roots: Vec<String>,
    /// Master agent applied to every project without an explicit override.
    pub default_agent: Option<String>,
    /// Per-project agent overrides, keyed by absolute project path.
    pub project_agents: BTreeMap<String, String>,
    /// Recently opened projects (incl. temp workspaces), most-recent first.
    #[serde(default)]
    pub recent_projects: Vec<String>,
    /// Projects the user pinned in the switcher, so they sit above the recents
    /// and survive falling out of the recent history. Keyed by absolute path.
    #[serde(default)]
    pub pinned_projects: Vec<String>,
    /// Paths ADE created (temp workspaces, and where they were moved to). Only
    /// these may be renamed/moved/deleted - never a real project the user owns.
    #[serde(default)]
    pub owned_workspaces: Vec<String>,
    /// Friendly display names for workspaces, keyed by absolute path - auto-derived
    /// for temp workspaces, or set by the user in PADE (see [`LabelSource`]), and
    /// shown instead of the `temp-<stamp>` folder name. A label never touches the
    /// directory on disk (the live agent locks its cwd).
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// When each project was last opened or left (Unix milliseconds), keyed by
    /// canonical path - the switcher's "last accessed" time, and what the
    /// unused-temp cleanup measures a temp workspace's idleness against.
    #[serde(default)]
    pub workspace_last_used: BTreeMap<String, u64>,
    /// Appearance & editor preferences.
    #[serde(default)]
    pub prefs: Prefs,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchContext {
    /// True when the launch directory already looks like a project.
    pub(super) has_project: bool,
    pub(super) cwd: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectEntry {
    pub(super) name: String,
    pub(super) path: String,
    pub(super) is_git: bool,
}

pub(super) fn config_base() -> Result<PathBuf, String> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }
    .ok_or_else(|| "no config directory".to_string())
}

/// PADE's config directory, created on disk if missing.
pub(crate) fn ensure_config_directory() -> Result<PathBuf, String> {
    let directory = config_base()?.join(CONFIG_DIRECTORY);
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

/// One-time migration from the old `ade` config dir to `pade`: move the whole
/// directory (settings, workspaces, worktrees) and rewrite absolute paths stored
/// in the history so recents/temp workspaces keep working. Idempotent.
pub(crate) fn migrate_from_ade() {
    let Ok(base) = config_base() else { return };
    let old = base.join(LEGACY_CONFIG_DIRECTORY);
    let new = base.join(CONFIG_DIRECTORY);
    let is_already_migrated = new.exists();
    if is_already_migrated || !old.exists() {
        return;
    }
    if std::fs::rename(&old, &new).is_err() {
        return;
    }

    // Rewrite paths that pointed inside the old dir (temp workspaces, worktrees).
    let old_text = old.to_string_lossy();
    let new_text = new.to_string_lossy();
    let migrate_all = |paths: &mut Vec<String>| {
        for path in paths.iter_mut() {
            *path = path.replace(old_text.as_ref(), &new_text);
        }
    };
    let _ = update_settings(|settings| {
        migrate_all(&mut settings.recent_projects);
        migrate_all(&mut settings.owned_workspaces);
        migrate_all(&mut settings.roots);
        Ok(())
    });
}

pub(super) fn settings_path() -> Result<PathBuf, String> {
    Ok(ensure_config_directory()?.join(SETTINGS_FILE))
}

pub(super) fn is_project(directory: &Path) -> bool {
    PROJECT_MARKERS
        .iter()
        .any(|marker| directory.join(marker).exists())
}

pub(super) struct SettingsRepository {
    pub(super) lock: Mutex<()>,
    pub(super) path: Option<PathBuf>,
}

/// The settings after a mutation, plus the file text it wrote - `None` when the
/// mutation moved nothing and so wrote nothing.
pub(super) struct Update {
    pub(super) settings: Settings,
    pub(super) written: Option<String>,
}

impl SettingsRepository {
    fn path(&self) -> Result<PathBuf, String> {
        self.path.clone().map_or_else(settings_path, Ok)
    }

    fn load_unlocked(&self) -> Settings {
        let mut settings: Settings = self
            .path()
            .and_then(|path| std::fs::read_to_string(path).map_err(|error| error.to_string()))
            .and_then(|contents| serde_json::from_str(&contents).map_err(|error| error.to_string()))
            .unwrap_or_default();
        settings.recent_projects = canonical_dedup(&settings.recent_projects);
        settings.pinned_projects = canonical_dedup(&settings.pinned_projects);
        settings
    }

    pub(super) fn load(&self) -> Settings {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.load_unlocked()
    }

    /// Applies `mutate` and persists the result, reporting whether the settings
    /// actually moved - a mutation that changes nothing (the recurring prune of a
    /// list with nothing to forget) must not announce a change, or two windows
    /// refreshing off each other's announcements would never settle.
    pub(super) fn update(
        &self,
        mutate: impl FnOnce(&mut Settings) -> Result<(), String>,
    ) -> Result<Update, String> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = self.load_unlocked();
        let mut settings = before.clone();
        mutate(&mut settings)?;
        let written = if settings == before {
            None
        } else {
            Some(self.save_unlocked(&settings)?)
        };

        Ok(Update { settings, written })
    }

    /// Persists `settings` atomically, answering the text it wrote.
    fn save_unlocked(&self, settings: &Settings) -> Result<String, String> {
        let path = self.path()?;
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = path.with_extension(format!("json.tmp-{}-{sequence}", std::process::id()));
        let json = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
        let mut file = std::fs::File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(json.as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;

        #[cfg(windows)]
        if path.exists() {
            std::fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
        std::fs::rename(&temporary, path).map_err(|error| error.to_string())?;
        Ok(json)
    }
}

pub(crate) fn load() -> Settings {
    SETTINGS_REPOSITORY.load()
}

/// App-wide notify that the shared settings moved, so every window re-reads them
/// instead of holding a copy that is already wrong. Payload-free: each window
/// re-reads through its own `settings_get`, which keeps one adoption path (and
/// its ordering guard) for local and remote changes alike. Silent with no handle
/// planted - a unit test has no running app to emit through.
pub(super) fn broadcast_settings_changed() {
    let Some(app) = APP.get() else { return };
    let _ = app.emit(SETTINGS_CHANGED_EVENT, ());
}

/// Every settings mutation funnels through here, so this is also the one place
/// that can tell the *other* windows their copy went stale - a project deleted
/// from one window's switcher stays listed in the next window's until it does.
/// What it wrote is remembered as announced, so the settings-file watcher that
/// carries other processes' writes (`settings_sync`) doesn't announce it twice.
pub(super) fn update_settings(
    mutate: impl FnOnce(&mut Settings) -> Result<(), String>,
) -> Result<Settings, String> {
    let update = SETTINGS_REPOSITORY.update(mutate)?;
    if let Some(written) = update.written {
        settings_sync::ANNOUNCED.record(written);
        broadcast_settings_changed();
    }

    Ok(update.settings)
}

#[tauri::command]
pub(crate) fn settings_get() -> Settings {
    load()
}
