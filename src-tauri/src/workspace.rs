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
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// How many recently-opened projects to remember.
const RECENT_CAP: usize = 20;

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
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
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
    let mut settings = load();
    settings.recent_projects = settings.recent_projects.iter().map(fix).collect();
    settings.owned_workspaces = settings.owned_workspaces.iter().map(fix).collect();
    settings.roots = settings.roots.iter().map(fix).collect();
    let _ = save(&settings);
}

fn settings_path() -> Result<PathBuf, String> {
    Ok(ensure_config_dir()?.join("settings.json"))
}

fn is_project(dir: &Path) -> bool {
    MARKERS.iter().any(|m| dir.join(m).exists())
}

pub(crate) fn load() -> Settings {
    let mut settings: Settings = settings_path()
        .and_then(|p| std::fs::read_to_string(p).map_err(|e| e.to_string()))
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        .unwrap_or_default();
    // Self-heal any project recorded twice under different path spellings (a
    // doubled-backslash entry vs a normal one): recents + pins are always
    // canonical and unique on read, so the switcher never shows a folder twice.
    settings.recent_projects = canonical_dedup(&settings.recent_projects);
    settings.pinned_projects = canonical_dedup(&settings.pinned_projects);
    settings
}

fn save(settings: &Settings) -> Result<Settings, String> {
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(settings_path()?, json).map_err(|e| e.to_string())?;
    // Return the persisted value so the frontend stays in sync in one round-trip.
    Ok(load())
}

/// The directory PADE launched into, and whether it came from an explicit request.
pub(crate) struct LaunchDir {
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
pub(crate) fn launch_directory() -> LaunchDir {
    if let Some(dir) = std::env::args().skip(1).find(|arg| Path::new(arg).is_dir()) {
        return LaunchDir {
            path: PathBuf::from(dir),
            explicit: true,
        };
    }
    LaunchDir {
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
        |n| n.to_string_lossy().into_owned(),
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
    let mut s = load();
    if !s.roots.contains(&path) {
        s.roots.push(path);
    }
    save(&s)
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
    let mut s = load();
    s.roots.retain(|r| r != &path);
    save(&s)
}

/// Immediate sub-directories of `root` that look like projects.
#[tauri::command]
pub async fn workspace_scan(root: String) -> Result<Vec<ProjectEntry>, String> {
    let dir = std::fs::read_dir(&root).map_err(|e| e.to_string())?;
    let mut entries: Vec<ProjectEntry> = dir
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir() && is_project(p))
        .map(|p| ProjectEntry {
            name: p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string(),
            is_git: p.join(".git").exists(),
            path: p.to_string_lossy().into_owned(),
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
const SUGGESTION_CAP: usize = 8;

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
/// as absolute paths, name-sorted and capped at [`SUGGESTION_CAP`]. Collecting into
/// a `BTreeMap` keyed by the lowercased leaf yields the sort for free — no mutable
/// scratch vector to sort in place.
fn dir_completions(parent: &Path, prefix: &str) -> Vec<String> {
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
        .take(SUGGESTION_CAP)
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
        .map(|(parent, prefix)| dir_completions(&parent, &prefix))
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
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("temp-"))
        && path.contains("workspaces")
}

/// May ADE rename/move/delete this path? Only its own workspaces — never a real
/// project the user owns.
fn is_ade_owned(settings: &Settings, path: &str) -> bool {
    settings.owned_workspaces.iter().any(|p| p == path) || is_temp_workspace(path)
}

/// Public ownership check for the naming module: is this an ADE-owned workspace
/// (so it's safe to scan and label)?
pub fn is_owned(path: &str) -> bool {
    is_ade_owned(&load(), path)
}

/// Push a path to the front of the recent list (canonicalized, deduped, capped).
fn record_recent(settings: &mut Settings, path: &str) {
    let path = canonical_path(path);
    settings.recent_projects.retain(|p| p != &path);
    settings.recent_projects.insert(0, path);
    settings.recent_projects.truncate(RECENT_CAP);
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
    let mut settings = load();
    record_recent(&mut settings, &path);
    save(&settings)?;
    Ok(())
}

/// Create a throwaway workspace so the user can start coding immediately without
/// choosing a project, then switch to a real one whenever they like.
#[tauri::command]
pub async fn workspace_temp() -> Result<String, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dir = ensure_config_dir()?
        .join("workspaces")
        .join(format!("temp-{stamp}"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.to_string_lossy().into_owned();

    // Mark it ADE-owned so it can later be renamed/moved/deleted.
    let mut settings = load();
    if !settings.owned_workspaces.contains(&path) {
        settings.owned_workspaces.push(path.clone());
    }
    save(&settings)?;

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
    let mut settings = load();
    if !is_ade_owned(&settings, &path) {
        return Err("only ADE-created workspaces can be labeled".into());
    }
    let label = crate::naming::sanitize(&name).ok_or("invalid name")?;
    settings.labels.insert(path, label);
    save(&settings)
}

/// Move `from` into `dest_dir` (keeping its folder name). The result is a normal
/// directory — no longer "temp" — but stays ADE-owned so it's still deletable.
#[tauri::command]
pub async fn workspace_move(from: String, dest_dir: String) -> Result<String, String> {
    let mut settings = load();
    if !is_ade_owned(&settings, &from) {
        return Err("only ADE-created workspaces can be moved".into());
    }
    let name = Path::new(&from)
        .file_name()
        .ok_or("bad source path")?
        .to_owned();
    let dest = Path::new(&dest_dir).join(name);
    std::fs::rename(&from, &dest).map_err(|e| e.to_string())?;
    let dest_str = dest.to_string_lossy().into_owned();
    // Re-point external tools (Claude transcripts, IDE recents) at the new path.
    // Best-effort and independent of the internal `retarget` below.
    crate::refs::update_references(&from, &dest_str);
    retarget(&mut settings, &from, &dest_str);
    save(&settings)?;
    workspace_open(dest_str.clone())?;
    Ok(dest_str)
}

/// Rename a temp workspace, promoting it into the primary project root
/// (`roots[0]`) under the new name — turning it into a real project.
#[tauri::command]
pub async fn workspace_rename(from: String, new_name: String) -> Result<String, String> {
    let mut settings = load();
    if !is_ade_owned(&settings, &from) {
        return Err("only ADE-created workspaces can be renamed".into());
    }
    let root = settings
        .roots
        .first()
        .ok_or("add a root folder first — rename saves into the primary root")?
        .clone();
    let dest = Path::new(&root).join(new_name.trim());
    std::fs::rename(&from, &dest).map_err(|e| e.to_string())?;
    let dest_str = dest.to_string_lossy().into_owned();
    // Re-point external tools (agent memory, IDE recents) at the new path —
    // best-effort, independent of the internal `retarget` below.
    crate::refs::update_references(&from, &dest_str);
    retarget(&mut settings, &from, &dest_str);
    save(&settings)?;
    workspace_open(dest_str.clone())?;
    Ok(dest_str)
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
    let mut settings = load();
    let before = (
        settings.recent_projects.len(),
        settings.pinned_projects.len(),
        settings.owned_workspaces.len(),
        settings.labels.len(),
    );

    settings.recent_projects.retain(|path| !has_vanished(path));
    settings.pinned_projects.retain(|path| !has_vanished(path));
    settings.owned_workspaces.retain(|path| !has_vanished(path));
    settings.labels.retain(|path, _| !has_vanished(path));

    let after = (
        settings.recent_projects.len(),
        settings.pinned_projects.len(),
        settings.owned_workspaces.len(),
        settings.labels.len(),
    );
    if before == after {
        return Ok(settings);
    }

    save(&settings)
}

/// Remove `path` from disk (stepping the process out first, riding out the
/// Windows sharing race) and forget it from every list — recents, pins, owned,
/// and its label. Shared by the owned-only `workspace_delete` and the
/// confirmation-gated `workspace_delete_directory`.
fn delete_directory(settings: &mut Settings, path: &str) -> Result<(), String> {
    leave_if_inside(Path::new(path));
    remove_dir_all_patiently(path).map_err(|e| e.to_string())?;
    settings.recent_projects.retain(|entry| entry != path);
    settings.pinned_projects.retain(|entry| entry != path);
    settings.owned_workspaces.retain(|entry| entry != path);
    settings.labels.remove(path);
    Ok(())
}

/// Delete an ADE-owned workspace directory and forget it.
#[tauri::command]
pub async fn workspace_delete(path: String) -> Result<Settings, String> {
    let mut settings = load();
    if !is_ade_owned(&settings, &path) {
        return Err("only ADE-created workspaces can be deleted".into());
    }
    delete_directory(&mut settings, &path)?;
    save(&settings)
}

/// Delete ANY project directory from disk and forget it — the switcher's "Delete
/// directory" action. Unlike `workspace_delete` this is not gated to ADE-owned
/// workspaces, so it can remove a real project the user points at; the UI raises
/// an explicit, path-naming confirmation before calling it, and the caller (the
/// relocator) kills the sessions holding the folder first.
#[tauri::command]
pub async fn workspace_delete_directory(path: String) -> Result<Settings, String> {
    let mut settings = load();
    delete_directory(&mut settings, &path)?;
    save(&settings)
}

/// Create a new project directory under `root` and open it.
#[tauri::command]
pub async fn workspace_create(root: String, name: String) -> Result<String, String> {
    let path = Path::new(&root).join(&name);
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let path_str = path.to_string_lossy().into_owned();
    workspace_open(path_str.clone())?;
    Ok(path_str)
}

/// Master switch: set the default agent for every project and clear per-project
/// overrides so the whole workspace moves to it at once.
#[tauri::command]
pub fn set_default_agent(agent: String) -> Result<Settings, String> {
    let mut s = load();
    s.default_agent = Some(agent);
    s.project_agents.clear();
    save(&s)
}

/// Override the agent for a single project.
#[tauri::command]
pub fn set_project_agent(path: String, agent: String) -> Result<Settings, String> {
    let mut s = load();
    s.project_agents.insert(path, agent);
    save(&s)
}

/// Clear the recent-projects history.
#[tauri::command]
pub fn workspace_clear_recent() -> Result<Settings, String> {
    let mut settings = load();
    settings.recent_projects.clear();
    save(&settings)
}

/// Pin or unpin a project in the switcher. Pinning moves it to the front of the
/// pinned list; unpinning drops it. Returns the refreshed settings.
#[tauri::command]
pub fn workspace_set_pinned(path: String, pinned: bool) -> Result<Settings, String> {
    let mut settings = load();
    settings
        .pinned_projects
        .retain(|pinned_path| pinned_path != &path);
    if pinned {
        settings.pinned_projects.insert(0, path);
    }
    save(&settings)
}

/// Forget a project from the switcher — drop it from the recent history and, if
/// pinned, from the pinned list too (a pin outlives recents, so removing only the
/// recent entry would leave the row still showing). The folder on disk is
/// untouched, and its display label is kept so a later re-open keeps the friendly
/// name. Returns the refreshed settings.
#[tauri::command]
pub fn workspace_remove_recent(path: String) -> Result<Settings, String> {
    let mut settings = load();
    settings.recent_projects.retain(|entry| entry != &path);
    settings.pinned_projects.retain(|entry| entry != &path);
    save(&settings)
}

/// Replace the pinned-project order with `paths` — a drag-reorder of the existing
/// pins. Reconciles rather than trusting the client: only already-pinned paths are
/// kept (this reorders, it never adds a pin — that stays `workspace_set_pinned`),
/// and any current pin the caller omitted is appended in its existing order, so a
/// list that raced with a toggle in another window can't silently drop a pin.
#[tauri::command]
pub fn workspace_set_pinned_order(paths: Vec<String>) -> Result<Settings, String> {
    let mut settings = load();
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
    save(&settings)
}

/// Persist a user-added editor, de-duplicated by executable path (re-adding the
/// same path is a no-op move-to-end). Returns the refreshed settings.
pub fn add_editor(editor: AddedEditor) -> Result<Settings, String> {
    let mut s = load();
    s.prefs.added_editors.retain(|e| e.path != editor.path);
    s.prefs.added_editors.push(editor);
    save(&s)
}

/// Persist the user's explicit editor pick for one project — keyed by the
/// canonical path so spelling variants of the same folder resolve to one entry.
/// Returns the refreshed settings.
pub fn set_project_editor(path: &str, editor_id: &str) -> Result<Settings, String> {
    let mut s = load();
    s.prefs
        .ide_project_choices
        .insert(canonical_path(path), editor_id.to_string());
    save(&s)
}

/// Drop a user-added editor by its id. Returns the refreshed settings; removing
/// an id that isn't present is a no-op.
pub fn remove_editor(id: &str) -> Result<Settings, String> {
    let mut s = load();
    s.prefs.added_editors.retain(|e| e.id != id);
    save(&s)
}

/// Replace appearance/editor preferences (frontend sends the full set).
#[tauri::command]
pub fn set_prefs(prefs: Prefs) -> Result<Settings, String> {
    let mut s = load();
    s.prefs = prefs;
    save(&s)
}

#[cfg(test)]
mod tests {
    use super::{canonical_dedup, canonical_path};

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
}
