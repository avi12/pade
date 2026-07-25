//! Workspace & projects.
//!
//! Two launch modes:
//!  - Launched *inside* a project directory → use it directly (the agent rules
//!    apply to that dir).
//!  - Launched with no project → the project onboarding lets the user pick root
//!    directories, browse the projects inside them, open one, or create a new
//!    one.
//!
//! Settings (roots, default/per-project agent) persist to the OS config dir.
//! They are plain JSON so they can later live in a git-backed shelf for sync.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// How many recently-opened projects to remember.
const RECENT_PROJECT_LIMIT: usize = 20;
static SETTINGS_REPOSITORY: SettingsRepository = SettingsRepository {
    lock: Mutex::new(()),
    path: None,
};
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Files/dirs that mark a directory as a project worth listing.
const MARKERS: &[&str] = &[
    ".git",
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

/// An editor the user located by executable path — first-class alongside the
/// PATH-detected ones. `command` for launching is the absolute `path`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddedEditor {
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
/// verbatim through `passthrough` — so a new UI-only pref never means editing
/// this struct, and Rust never duplicates a type the frontend already owns.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    /// Editor-rules engine: project-kind → IDE id. When a project's primary kind
    /// matches a key here, that IDE is suggested first (if installed).
    #[serde(default)]
    pub ide_rules: BTreeMap<String, String>,
    /// IDE id used when no `ide_rules` entry matches the project kind.
    #[serde(default)]
    pub ide_fallback: Option<String>,
    /// Explicit per-project editor picks — canonical project path → IDE id. A
    /// pick from the workspace's editor menu lands here and outranks every
    /// suggestion rule for that project (`ide_suggest` puts it first).
    #[serde(default)]
    pub ide_project_choices: BTreeMap<String, String>,
    /// Editors the user located by executable path (not auto-detected on PATH).
    /// Merged into the detected editor list so they show up in every menu.
    #[serde(default)]
    pub added_editors: Vec<AddedEditor>,
    /// Frontend-owned preferences Rust never acts on, kept verbatim so they
    /// survive a load/save round-trip. `flatten` captures every key not named
    /// above; the TS zod schema is their single source of truth.
    #[serde(flatten)]
    pub passthrough: BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
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
    /// these may be renamed/moved/deleted — never a real project the user owns.
    #[serde(default)]
    pub owned_workspaces: Vec<String>,
    /// Friendly display names for workspaces, keyed by absolute path. Auto-derived
    /// for temp workspaces and shown instead of the `temp-<stamp>` folder name. A
    /// label never touches the directory on disk (the live agent locks its cwd).
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// Appearance & editor preferences.
    #[serde(default)]
    pub prefs: Prefs,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchContext {
    /// True when the launch directory already looks like a project.
    has_project: bool,
    cwd: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEntry {
    name: String,
    path: String,
    is_git: bool,
}

fn config_base() -> Result<PathBuf, String> {
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
pub(crate) fn ensure_config_dir() -> Result<PathBuf, String> {
    let dir = config_base()?.join("pade");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// One-time migration from the old `ade` config dir to `pade`: move the whole
/// directory (settings, workspaces, worktrees) and rewrite absolute paths stored
/// in the history so recents/temp workspaces keep working. Idempotent.
pub fn migrate_from_ade() {
    let Ok(base) = config_base() else { return };
    let old = base.join("ade");
    let new = base.join("pade");
    if new.exists() || !old.exists() {
        return; // already migrated, or nothing to migrate
    }
    if std::fs::rename(&old, &new).is_err() {
        return;
    }

    // Rewrite paths that pointed inside the old dir (temp workspaces, worktrees).
    let (old_s, new_s) = (
        old.to_string_lossy().to_string(),
        new.to_string_lossy().to_string(),
    );
    let fix = |p: &String| p.replace(&old_s, &new_s);
    let _ = update_settings(|settings| {
        settings.recent_projects = settings.recent_projects.iter().map(fix).collect();
        settings.owned_workspaces = settings.owned_workspaces.iter().map(fix).collect();
        settings.roots = settings.roots.iter().map(fix).collect();
        Ok(())
    });
}

fn settings_path() -> Result<PathBuf, String> {
    Ok(ensure_config_dir()?.join("settings.json"))
}

fn is_project(dir: &Path) -> bool {
    MARKERS.iter().any(|marker| dir.join(marker).exists())
}

struct SettingsRepository {
    lock: Mutex<()>,
    path: Option<PathBuf>,
}

impl SettingsRepository {
    fn path(&self) -> Result<PathBuf, String> {
        self.path.clone().map_or_else(settings_path, Ok)
    }

    fn load_unlocked(&self) -> Settings {
        let mut settings: Settings = self
            .path()
            .and_then(|path| std::fs::read_to_string(path).map_err(|e| e.to_string()))
            .and_then(|contents| serde_json::from_str(&contents).map_err(|e| e.to_string()))
            .unwrap_or_default();
        settings.recent_projects = canonical_dedup(&settings.recent_projects);
        settings.pinned_projects = canonical_dedup(&settings.pinned_projects);
        settings
    }

    fn load(&self) -> Settings {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.load_unlocked()
    }

    fn update(
        &self,
        mutate: impl FnOnce(&mut Settings) -> Result<(), String>,
    ) -> Result<Settings, String> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut settings = self.load_unlocked();
        mutate(&mut settings)?;
        self.save_unlocked(&settings)?;
        Ok(settings)
    }

    fn save_unlocked(&self, settings: &Settings) -> Result<(), String> {
        let path = self.path()?;
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = path.with_extension(format!("json.tmp-{}-{sequence}", std::process::id()));
        let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
        file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;

        #[cfg(windows)]
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())
    }
}

pub(crate) fn load() -> Settings {
    SETTINGS_REPOSITORY.load()
}

fn update_settings(
    mutate: impl FnOnce(&mut Settings) -> Result<(), String>,
) -> Result<Settings, String> {
    SETTINGS_REPOSITORY.update(mutate)
}

/// The directory PADE launched into, and whether it came from an explicit request.
pub(crate) struct LaunchDirectory {
    pub path: PathBuf,
    /// `true` when a `pade <dir>` argument named it (an explicit open), so the
    /// caller treats it as a project without probing for markers.
    pub explicit: bool,
}

/// Resolve the directory this instance launched into — the single source of truth
/// for both `launch_context` (what the frontend boots into) and the per-instance
/// `WebView2` folder keying (which project's process tree this instance owns). A
/// directory passed as an argument — `pade <dir>` from a terminal or the folder's
/// context menu — is an explicit request to open that project; otherwise it is the
/// process working directory.
pub(crate) fn launch_directory() -> LaunchDirectory {
    if let Some(dir) = std::env::args().skip(1).find(|arg| Path::new(arg).is_dir()) {
        return LaunchDirectory {
            path: PathBuf::from(dir),
            explicit: true,
        };
    }
    LaunchDirectory {
        path: std::env::current_dir().unwrap_or_default(),
        explicit: false,
    }
}

/// This instance's `WebView2` user-data folder, keyed by the launch project so two
/// projects open in parallel each run in their own browser + GPU process tree
/// instead of sharing one — the shared default lets one instance's GPU load (and
/// the ~16 `WebGL`-context cap) compound into the other and trip a DWM/GPU reset.
/// Reopening the same project reuses its folder (a stable digest of the canonical
/// path), so nothing accumulates per launch. `None` when `LOCALAPPDATA` can't be
/// resolved, leaving `WebView2` on its shared default. Windows-only — the folder is
/// a `WebView2` concept.
#[cfg(windows)]
pub(crate) fn webview_data_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    let launch = launch_directory().path;
    let canonical = std::fs::canonicalize(&launch).unwrap_or(launch);
    let key = canonical.to_string_lossy().to_lowercase();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&key, &mut hasher);
    let digest = std::hash::Hasher::finish(&hasher);

    let name = canonical.file_name().map_or_else(
        || "project".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    Some(
        base.join("pade")
            .join("webview2")
            .join(format!("{name}-{digest:016x}")),
    )
}

#[tauri::command]
pub fn launch_context() -> LaunchContext {
    let launch = launch_directory();
    LaunchContext {
        has_project: launch.explicit || is_project(&launch.path),
        cwd: launch.path.to_string_lossy().into_owned(),
    }
}

#[tauri::command]
pub fn settings_get() -> Settings {
    load()
}

/// The result of trying to add a root folder. A directory that already exists (or
/// one just created on request) is added and carries the refreshed `Settings`; the
/// two "didn't add" outcomes tell the picker to prompt or show an error instead of
/// persisting a broken root.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum AddRootOutcome {
    /// The path is (now) a directory and was added. Boxed so this data-carrying
    /// variant doesn't bloat the empty ones (serializes identically to `Settings`).
    Added { settings: Box<Settings> },
    /// The path doesn't exist and creation wasn't requested.
    Missing,
    /// The path exists but names a file, not a directory.
    NotADirectory,
}

/// Rebuild a path from its components — collapses doubled or trailing separators
/// and forward slashes so one folder is spelled exactly one way and dedups (e.g.
/// `C:\\a\\b`, `C:/a/b` and `C:\a\b\` all fold to `C:\a\b`), while keeping a bare
/// drive root like `C:\` intact.
pub(crate) fn canonical_path(path: &str) -> String {
    Path::new(path)
        .components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned()
}

/// Join a user-provided folder name beneath `root` without allowing the name to
/// become a path, drive, device, alternate data stream, or parent traversal.
pub(crate) fn validated_child_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    let has_separator = name.contains(['/', '\\']);
    let has_control = name.chars().any(char::is_control);
    if name.is_empty() || name == "." || name == ".." || has_separator || has_control {
        return Err("folder name must be one plain path segment".into());
    }

    #[cfg(windows)]
    {
        let stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let is_reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || stem
                .strip_prefix("COM")
                .or_else(|| stem.strip_prefix("LPT"))
                .is_some_and(|number| {
                    matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
                });
        if name.contains(':') || name.ends_with(['.', ' ']) || is_reserved {
            return Err("folder name is reserved by Windows".into());
        }
    }

    Ok(root.join(name))
}

/// Canonicalize a path list and drop duplicates, keeping first-seen order — so a
/// folder recorded twice under different spellings collapses to one entry.
fn canonical_dedup(paths: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    paths
        .iter()
        .map(|path| canonical_path(path))
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

/// Persist `path` as a root (deduped) and hand back the refreshed settings. The
/// path is canonicalized first (see [`canonical_path`]) so a root is stored one
/// way and dedups.
fn push_root(path: String) -> Result<Settings, String> {
    let path = canonical_path(&path);
    update_settings(|settings| {
        if !settings.roots.contains(&path) {
            settings.roots.push(path);
        }
        Ok(())
    })
}

/// Add a root folder. An existing directory is added as-is; a missing path is only
/// created (and then added) when `create` is set, otherwise it reports `Missing`;
/// a path that exists but is a file reports `NotADirectory`.
#[tauri::command]
pub async fn workspace_add_root(path: String, create: bool) -> Result<AddRootOutcome, String> {
    let target = Path::new(&path);
    if target.is_dir() {
        return Ok(AddRootOutcome::Added {
            settings: Box::new(push_root(path)?),
        });
    }
    if target.exists() {
        return Ok(AddRootOutcome::NotADirectory);
    }
    if !create {
        return Ok(AddRootOutcome::Missing);
    }
    std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
    Ok(AddRootOutcome::Added {
        settings: Box::new(push_root(path)?),
    })
}

#[tauri::command]
pub fn workspace_remove_root(path: String) -> Result<Settings, String> {
    update_settings(|settings| {
        settings.roots.retain(|root| root != &path);
        Ok(())
    })
}

/// Immediate sub-directories of `root` that look like projects.
#[tauri::command]
pub async fn workspace_scan(root: String) -> Result<Vec<ProjectEntry>, String> {
    let directory = std::fs::read_dir(&root).map_err(|e| e.to_string())?;
    let mut entries: Vec<ProjectEntry> = directory
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && is_project(path))
        .map(|path| ProjectEntry {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("?")
                .to_string(),
            is_git: path.join(".git").exists(),
            path: path.to_string_lossy().into_owned(),
        })
        .collect();
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(entries)
}

/// A live look at a path the user is typing into the add-root field: what the
/// path itself is on disk (so the field can say "exists" vs "will be created", or
/// reject a file), plus the child directories that would complete it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathProbe {
    /// The typed path is an existing directory.
    is_dir: bool,
    /// The typed path exists but names a file, not a directory.
    is_file: bool,
    /// The typed path's parent is an existing directory — so even though the path
    /// itself doesn't exist yet, it names a real place PADE can create it in. This
    /// is what tells a not-yet-created folder ("C:\repositories\new-app") apart
    /// from a stray, un-locatable string — an existence check in place of a regex.
    parent_exists: bool,
    /// Absolute paths of child directories that complete the text, name-sorted
    /// and capped. Empty when nothing matches or the parent can't be read.
    suggestions: Vec<String>,
}

/// How many directory completions to offer at once — enough to be useful, few
/// enough to stay a glance rather than a scroll.
const SUGGESTION_LIMIT: usize = 8;

/// Split a partially-typed path into the directory to list and the (possibly
/// empty) leaf typed so far. A trailing separator means "list everything inside
/// this directory"; otherwise the last segment is the prefix to match. The
/// separator is kept on a bare drive/root head so `C:\` stays absolute. `None`
/// when there's no separator yet (e.g. a lone `C:` — nothing to complete).
fn split_for_completion(input: &str) -> Option<(PathBuf, String)> {
    let cut = input.rfind(['\\', '/'])?;
    let (head, tail) = input.split_at(cut);
    let prefix = tail[1..].to_string();
    let parent = if head.is_empty() || head.ends_with(':') {
        format!("{head}{}", std::path::MAIN_SEPARATOR)
    } else {
        head.to_string()
    };
    Some((PathBuf::from(parent), prefix))
}

/// Child directories of `parent` whose name starts with `prefix` (case-insensitive),
/// as absolute paths, name-sorted and capped at [`SUGGESTION_LIMIT`]. Collecting into
/// a `BTreeMap` keyed by the lowercased leaf yields the sort for free — no mutable
/// scratch vector to sort in place.
fn directory_completions(parent: &Path, prefix: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    let needle = prefix.to_lowercase();
    let leaf_lower = |path: &Path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_lowercase()
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && leaf_lower(path).starts_with(&needle))
        .map(|path| (leaf_lower(&path), path))
        .collect::<BTreeMap<String, PathBuf>>()
        .into_values()
        .take(SUGGESTION_LIMIT)
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

/// Probe the path being typed into the add-root field: what it is on disk, plus
/// the child-directory completions that drive the field's autocomplete. Pure
/// query (no persistence); a bad or unreadable path just yields empty suggestions.
#[tauri::command]
pub async fn workspace_probe_path(path: String) -> PathProbe {
    let trimmed = path.trim();
    let target = Path::new(trimmed);
    let suggestions = split_for_completion(trimmed)
        .map(|(parent, prefix)| directory_completions(&parent, &prefix))
        .unwrap_or_default();
    PathProbe {
        is_dir: target.is_dir(),
        is_file: target.is_file(),
        parent_exists: target.parent().is_some_and(Path::is_dir),
        suggestions,
    }
}

/// A path directly under the config `.../workspaces/temp-*` is one ADE created,
/// even if predating the `owned_workspaces` list.
fn is_temp_workspace(path: &str) -> bool {
    let target = Path::new(path);
    let has_temp_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("temp-"));
    if !has_temp_name {
        return false;
    }

    let Ok(workspaces) = ensure_config_dir().map(|directory| directory.join("workspaces")) else {
        return false;
    };
    std::fs::canonicalize(target)
        .ok()
        .zip(std::fs::canonicalize(workspaces).ok())
        .is_some_and(|(target, parent)| target.parent() == Some(parent.as_path()))
}

/// May ADE rename/move/delete this path? Only its own workspaces — never a real
/// project the user owns.
fn is_ade_owned(settings: &Settings, path: &str) -> bool {
    settings
        .owned_workspaces
        .iter()
        .any(|workspace| workspace == path)
        || is_temp_workspace(path)
}

/// Public ownership check for the naming module: is this an ADE-owned workspace
/// (so it's safe to scan and label)?
pub fn is_owned(path: &str) -> bool {
    is_ade_owned(&load(), path)
}

/// Push a path to the front of the recent list (canonicalized, deduped, capped).
fn record_recent(settings: &mut Settings, path: &str) {
    let path = canonical_path(path);
    settings.recent_projects.retain(|project| project != &path);
    settings.recent_projects.insert(0, path);
    settings.recent_projects.truncate(RECENT_PROJECT_LIMIT);
}

/// Delete a consumed auto-handoff doc. The one file-deletion seam the frontend
/// has, so it can only ever remove what auto-handoff itself created: a bare
/// `continue-*.md` name (no path separators) directly inside `dir`. A doc that
/// is already gone is fine — the goal is its absence.
#[tauri::command]
pub async fn handoff_doc_delete(dir: String, name: String) -> Result<(), String> {
    let is_handoff_doc = name.starts_with("continue-")
        && Path::new(&name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        && !name.contains(['/', '\\']);
    if !is_handoff_doc {
        return Err("only a continue-*.md handoff doc can be deleted".into());
    }
    let path = Path::new(&dir).join(&name);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Open a project: point the process (and thus the watcher/VCS/agent) at it and
/// remember it in the recent history.
#[tauri::command]
pub fn workspace_open(path: String) -> Result<(), String> {
    std::env::set_current_dir(&path).map_err(|e| e.to_string())?;
    update_settings(|settings| {
        record_recent(settings, &path);
        Ok(())
    })?;
    Ok(())
}

/// Create a throwaway workspace so the user can start coding immediately without
/// choosing a project, then switch to a real one whenever they like.
#[tauri::command]
pub async fn workspace_temp() -> Result<String, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let dir = ensure_config_dir()?
        .join("workspaces")
        .join(format!("temp-{stamp}"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.to_string_lossy().into_owned();

    // Mark it ADE-owned so it can later be renamed/moved/deleted.
    update_settings(|settings| {
        if !settings.owned_workspaces.contains(&path) {
            settings.owned_workspaces.push(path.clone());
        }
        Ok(())
    })?;

    workspace_open(path.clone())?;
    Ok(path)
}

/// Replace a path across recent + owned lists (used when a workspace moves). The
/// display label, if any, follows the path to its new key.
fn retarget(settings: &mut Settings, from: &str, to: &str) {
    for list in [
        &mut settings.recent_projects,
        &mut settings.owned_workspaces,
    ] {
        for entry in list.iter_mut() {
            if entry == from {
                *entry = to.to_string();
            }
        }
    }
    if let Some(label) = settings.labels.remove(from) {
        settings.labels.insert(to.to_string(), label);
    }
}

/// Set a friendly display label for an ADE-owned workspace. Non-destructive: the
/// directory keeps its `temp-<stamp>` name on disk (the live agent holds it as
/// cwd, which the OS locks against rename); only the shown name changes.
#[tauri::command]
pub fn workspace_set_label(path: String, name: String) -> Result<Settings, String> {
    let label = crate::naming::sanitize(&name).ok_or("invalid name")?;
    update_settings(|settings| {
        if !is_ade_owned(settings, &path) {
            return Err("only ADE-created workspaces can be labeled".into());
        }
        settings.labels.insert(path, label);
        Ok(())
    })
}

/// Move `from` into `dest_dir` (keeping its folder name). The result is a normal
/// directory — no longer "temp" — but stays ADE-owned so it's still deletable.
#[tauri::command]
pub async fn workspace_move(from: String, dest_dir: String) -> Result<String, String> {
    let settings = load();
    if !is_ade_owned(&settings, &from) {
        return Err("only ADE-created workspaces can be moved".into());
    }
    let name = Path::new(&from)
        .file_name()
        .ok_or("bad source path")?
        .to_owned();
    let destination = Path::new(&dest_dir).join(name);
    std::fs::rename(&from, &destination).map_err(|e| e.to_string())?;
    let destination_string = destination.to_string_lossy().into_owned();
    // Re-point external tools (Claude transcripts, IDE recents) at the new path.
    // Best-effort and independent of the internal `retarget` below.
    crate::refs::update_references(&from, &destination_string);
    update_settings(|settings| {
        retarget(settings, &from, &destination_string);
        Ok(())
    })?;
    workspace_open(destination_string.clone())?;
    Ok(destination_string)
}

/// Rename a temp workspace, promoting it into a saved project root under the new
/// name — turning it into a real project. `root` picks which saved root receives
/// it (it must be one of `settings.roots`); omitted, the primary root
/// (`roots[0]`) is used.
#[tauri::command]
pub async fn workspace_rename(
    from: String,
    new_name: String,
    root: Option<String>,
) -> Result<String, String> {
    let settings = load();
    if !is_ade_owned(&settings, &from) {
        return Err("only ADE-created workspaces can be renamed".into());
    }
    let root = match root {
        Some(chosen) => {
            let chosen = canonical_path(&chosen);
            settings
                .roots
                .iter()
                .find(|saved| canonical_path(saved) == chosen)
                .ok_or("the chosen destination is not a saved root")?
                .clone()
        }
        None => settings
            .roots
            .first()
            .ok_or("add a root folder first — rename saves into the primary root")?
            .clone(),
    };
    let destination = validated_child_path(Path::new(&root), &new_name)?;
    std::fs::rename(&from, &destination).map_err(|e| e.to_string())?;
    let destination_string = destination.to_string_lossy().into_owned();
    // Re-point external tools (agent memory, IDE recents) at the new path —
    // best-effort, independent of the internal `retarget` below.
    crate::refs::update_references(&from, &destination_string);
    update_settings(|settings| {
        retarget(settings, &from, &destination_string);
        Ok(())
    })?;
    workspace_open(destination_string.clone())?;
    Ok(destination_string)
}

/// How many times to re-try a delete that lost a race with Windows, and how long
/// to wait between attempts.
const DELETE_ATTEMPTS: u32 = 10;
const DELETE_RETRY: Duration = Duration::from_millis(100);

/// Step the process out of a folder it is about to lose. Opening a project points
/// the whole process at it — `workspace_open` chdirs — and on Windows the current
/// directory is an open handle: the OS refuses to delete the folder a process is
/// standing in, however many agents have been killed. So walk out to its parent
/// first. (The project is on its way out; the frontend drops it at the same time.)
fn leave_if_inside(path: &Path) {
    let Ok(cwd) = std::env::current_dir() else {
        return;
    };
    let is_inside = std::fs::canonicalize(cwd)
        .ok()
        .zip(std::fs::canonicalize(path).ok())
        .is_some_and(|(here, doomed)| here.starts_with(&doomed));
    if !is_inside {
        return;
    }

    if let Some(parent) = path.parent() {
        let _ = std::env::set_current_dir(parent);
    }
}

/// Remove a directory tree, riding out a Windows sharing violation. The agents
/// holding the folder are killed before this runs, but the OS closes their
/// handles asynchronously: a process can be gone while its last handle is still
/// open, and a delete in that window fails with "used by another process".
/// Retrying briefly turns that race into a wait instead of an error.
fn remove_dir_all_patiently(path: &str) -> std::io::Result<()> {
    for attempt in 1..=DELETE_ATTEMPTS {
        match std::fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            // Already gone (deleted outside PADE): the folder is in the state the
            // caller asked for, so this is a success — and the entry still on the
            // Recent list gets forgotten instead of being stuck there forever.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) if attempt == DELETE_ATTEMPTS => return Err(error),
            Err(_) => std::thread::sleep(DELETE_RETRY),
        }
    }
    Ok(())
}

/// Has this folder been deleted out from under us? Only if its parent is still
/// there: a project on a drive that is merely unplugged has a missing folder AND a
/// missing parent, and it keeps its place in the list rather than being forgotten
/// because someone pulled a USB stick.
fn has_vanished(path: &str) -> bool {
    let path = Path::new(path);
    !path.exists() && path.parent().is_some_and(Path::exists)
}

/// Forget every remembered path whose folder is gone, and hand back the settings
/// the picker should now show. Called on every picker refresh — including the ones
/// its directory watcher triggers — so a workspace deleted in Explorer, by a
/// script, or from a terminal leaves the page like one deleted from the menu.
#[tauri::command]
pub async fn workspace_prune() -> Result<Settings, String> {
    update_settings(|settings| {
        settings.recent_projects.retain(|path| !has_vanished(path));
        settings.pinned_projects.retain(|path| !has_vanished(path));
        settings.owned_workspaces.retain(|path| !has_vanished(path));
        settings.labels.retain(|path, _| !has_vanished(path));
        Ok(())
    })
}

/// Remove `path` from disk (stepping the process out first, riding out the
/// Windows sharing race) and forget it from every list — recents, pins, owned,
/// and its label. Shared by the owned-only `workspace_delete` and the
/// confirmation-gated `workspace_delete_directory`.
fn delete_directory(path: &str) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("only a real project directory can be deleted".into());
    }
    let target = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    if target.parent().is_none() {
        return Err("a filesystem root cannot be deleted".into());
    }
    leave_if_inside(&target);
    remove_dir_all_patiently(&target.to_string_lossy()).map_err(|e| e.to_string())?;
    Ok(())
}

fn forget_directory(settings: &mut Settings, path: &str) {
    settings.recent_projects.retain(|entry| entry != path);
    settings.pinned_projects.retain(|entry| entry != path);
    settings.owned_workspaces.retain(|entry| entry != path);
    settings.labels.remove(path);
}

/// Delete an ADE-owned workspace directory and forget it.
#[tauri::command]
pub async fn workspace_delete(path: String) -> Result<Settings, String> {
    let settings = load();
    if !is_ade_owned(&settings, &path) {
        return Err("only ADE-created workspaces can be deleted".into());
    }
    delete_directory(&path)?;
    update_settings(|settings| {
        forget_directory(settings, &path);
        Ok(())
    })
}

/// Delete ANY project directory from disk and forget it — the switcher's "Delete
/// directory" action. Unlike `workspace_delete` this is not gated to ADE-owned
/// workspaces, so it can remove a real project the user points at; the UI raises
/// an explicit, path-naming confirmation before calling it, and the caller (the
/// relocator) kills the sessions holding the folder first.
#[tauri::command]
pub async fn workspace_delete_directory(path: String) -> Result<Settings, String> {
    let settings = load();
    let target = std::fs::canonicalize(&path).map_err(|e| e.to_string())?;
    let is_remembered = settings
        .recent_projects
        .iter()
        .chain(&settings.pinned_projects)
        .filter_map(|remembered| std::fs::canonicalize(remembered).ok())
        .any(|remembered| remembered == target);
    if !is_remembered {
        return Err("only a project shown in the switcher can be deleted".into());
    }
    delete_directory(&path)?;
    update_settings(|settings| {
        forget_directory(settings, &path);
        Ok(())
    })
}

/// Create a new project directory under `root` and open it.
#[tauri::command]
pub async fn workspace_create(root: String, name: String) -> Result<String, String> {
    let path = validated_child_path(Path::new(&root), &name)?;
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let path_string = path.to_string_lossy().into_owned();
    workspace_open(path_string.clone())?;
    Ok(path_string)
}

/// Master switch: set the default agent for every project and clear per-project
/// overrides so the whole workspace moves to it at once.
#[tauri::command]
pub fn set_default_agent(agent: String) -> Result<Settings, String> {
    update_settings(|settings| {
        settings.default_agent = Some(agent);
        settings.project_agents.clear();
        Ok(())
    })
}

/// Override the agent for a single project.
#[tauri::command]
pub fn set_project_agent(path: String, agent: String) -> Result<Settings, String> {
    update_settings(|settings| {
        settings.project_agents.insert(path, agent);
        Ok(())
    })
}

/// Clear the recent-projects history.
#[tauri::command]
pub fn workspace_clear_recent() -> Result<Settings, String> {
    update_settings(|settings| {
        settings.recent_projects.clear();
        Ok(())
    })
}

/// Pin or unpin a project in the switcher. Pinning moves it to the front of the
/// pinned list; unpinning drops it. Returns the refreshed settings.
#[tauri::command]
pub fn workspace_set_pinned(path: String, pinned: bool) -> Result<Settings, String> {
    update_settings(|settings| {
        settings
            .pinned_projects
            .retain(|pinned_path| pinned_path != &path);
        if pinned {
            settings.pinned_projects.insert(0, path);
        }
        Ok(())
    })
}

/// Forget a project from the switcher — drop it from the recent history and, if
/// pinned, from the pinned list too (a pin outlives recents, so removing only the
/// recent entry would leave the row still showing). The folder on disk is
/// untouched, and its display label is kept so a later re-open keeps the friendly
/// name. Returns the refreshed settings.
#[tauri::command]
pub fn workspace_remove_recent(path: String) -> Result<Settings, String> {
    update_settings(|settings| {
        settings.recent_projects.retain(|entry| entry != &path);
        settings.pinned_projects.retain(|entry| entry != &path);
        Ok(())
    })
}

/// Replace the pinned-project order with `paths` — a drag-reorder of the existing
/// pins. Reconciles rather than trusting the client: only already-pinned paths are
/// kept (this reorders, it never adds a pin — that stays `workspace_set_pinned`),
/// and any current pin the caller omitted is appended in its existing order, so a
/// list that raced with a toggle in another window can't silently drop a pin.
#[tauri::command]
pub fn workspace_set_pinned_order(paths: Vec<String>) -> Result<Settings, String> {
    update_settings(|settings| {
        let mut reordered: Vec<String> = paths
            .into_iter()
            .filter(|path| settings.pinned_projects.contains(path))
            .collect();
        for pinned in &settings.pinned_projects {
            if !reordered.contains(pinned) {
                reordered.push(pinned.clone());
            }
        }
        settings.pinned_projects = reordered;
        Ok(())
    })
}

/// Persist a user-added editor, de-duplicated by executable path (re-adding the
/// same path is a no-op move-to-end). Returns the refreshed settings.
pub fn add_editor(editor: AddedEditor) -> Result<Settings, String> {
    update_settings(|settings| {
        settings
            .prefs
            .added_editors
            .retain(|existing| existing.path != editor.path);
        settings.prefs.added_editors.push(editor);
        Ok(())
    })
}

/// Persist the user's explicit editor pick for one project — keyed by the
/// canonical path so spelling variants of the same folder resolve to one entry.
/// Returns the refreshed settings.
pub fn set_project_editor(path: &str, editor_id: &str) -> Result<Settings, String> {
    update_settings(|settings| {
        settings
            .prefs
            .ide_project_choices
            .insert(canonical_path(path), editor_id.to_string());
        Ok(())
    })
}

/// Drop a user-added editor by its id. Returns the refreshed settings; removing
/// an id that isn't present is a no-op.
pub fn remove_editor(id: &str) -> Result<Settings, String> {
    update_settings(|settings| {
        settings
            .prefs
            .added_editors
            .retain(|editor| editor.id != id);
        Ok(())
    })
}

fn apply_prefs_patch(
    current: &mut Prefs,
    patch: serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let serde_json::Value::Object(mut prefs) =
        serde_json::to_value(&*current).map_err(|error| error.to_string())?
    else {
        return Err("preferences did not serialize as an object".into());
    };
    for (key, value) in patch {
        if value.is_null() {
            prefs.remove(&key);
        } else {
            prefs.insert(key, value);
        }
    }
    *current = serde_json::from_value(serde_json::Value::Object(prefs))
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Apply only the preference fields the frontend changed. A null value removes
/// the optional key so its default takes effect again.
#[tauri::command]
pub fn set_prefs(patch: serde_json::Map<String, serde_json::Value>) -> Result<Settings, String> {
    update_settings(|settings| apply_prefs_patch(&mut settings.prefs, patch))
}

#[cfg(test)]
mod tests {
    use super::{
        apply_prefs_patch, canonical_dedup, canonical_path, validated_child_path, Prefs,
        SettingsRepository,
    };
    use std::path::Path;
    use std::sync::Arc;

    #[cfg(windows)]
    #[test]
    fn folds_doubled_and_forward_separators_to_one_spelling() {
        let canonical = canonical_path(r"C:\repositories\avi\sb-companion");
        assert_eq!(
            canonical_path(r"C:\\repositories\\avi\\sb-companion"),
            canonical
        );
        assert_eq!(
            canonical_path("C:/repositories/avi/sb-companion"),
            canonical
        );
        assert_eq!(
            canonical_path(r"C:\repositories\avi\sb-companion\"),
            canonical
        );
    }

    #[test]
    fn dedup_collapses_the_same_folder_spelled_two_ways() {
        let deduped = canonical_dedup(&[
            r"C:\repositories\avi\sb-companion".to_string(),
            r"C:\\repositories\\avi\\sb-companion".to_string(),
            r"C:\repositories\avi\pade".to_string(),
        ]);
        // The doubled-backslash duplicate folds away; first-seen order is kept.
        assert_eq!(
            deduped,
            vec![
                canonical_path(r"C:\repositories\avi\sb-companion"),
                canonical_path(r"C:\repositories\avi\pade"),
            ]
        );
    }

    #[test]
    fn child_paths_reject_traversal_and_absolute_names() {
        let root = Path::new("root");
        for name in [
            "../outside",
            "..\\outside",
            "/outside",
            "C:\\outside",
            ".",
            "",
        ] {
            assert!(validated_child_path(root, name).is_err(), "accepted {name}");
        }
        assert_eq!(
            validated_child_path(root, "project").expect("valid child"),
            root.join("project")
        );
    }

    #[test]
    fn preference_patch_removes_a_null_optional_key() {
        let mut prefs = Prefs {
            passthrough: std::collections::BTreeMap::from([(
                "themeMode".to_string(),
                serde_json::Value::String("dark".to_string()),
            )]),
            ..Prefs::default()
        };
        let patch =
            serde_json::Map::from_iter([("themeMode".to_string(), serde_json::Value::Null)]);

        apply_prefs_patch(&mut prefs, patch).expect("apply patch");

        assert!(!prefs.passthrough.contains_key("themeMode"));
    }

    #[test]
    fn repository_serializes_concurrent_updates_without_losing_fields() {
        let path = std::env::temp_dir().join(format!(
            "pade-settings-repository-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let repository = Arc::new(SettingsRepository {
            lock: std::sync::Mutex::new(()),
            path: Some(path.clone()),
        });
        let roots_repository = Arc::clone(&repository);
        let roots = std::thread::spawn(move || {
            roots_repository
                .update(|settings| {
                    settings.roots.push("root".to_string());
                    Ok(())
                })
                .expect("update roots");
        });
        let pins_repository = Arc::clone(&repository);
        let pins = std::thread::spawn(move || {
            pins_repository
                .update(|settings| {
                    settings.pinned_projects.push("project".to_string());
                    Ok(())
                })
                .expect("update pins");
        });

        roots.join().expect("roots thread");
        pins.join().expect("pins thread");
        let settings = repository.load();
        assert_eq!(settings.roots, ["root"]);
        assert_eq!(settings.pinned_projects, ["project"]);
        std::fs::remove_file(path).expect("clean settings fixture");
    }
}
