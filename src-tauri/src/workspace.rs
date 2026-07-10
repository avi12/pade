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
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Appearance & editor preferences. All optional so the frontend can fall back
/// to its own defaults; `None` means "unset, use the default".
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    /// UI font family; falls back to the M3 stack.
    pub ui_font: Option<String>,
    /// Terminal/code font family; falls back to `JetBrains` Mono.
    pub mono_font: Option<String>,
    /// "system" (follow OS) | "light" | "dark".
    pub theme_mode: Option<String>,
    /// Diff layout: "unified" | "split".
    pub diff_style: Option<String>,
    /// What to do when launched with no project: "temp" (default) | "picker".
    pub start_mode: Option<String>,
    /// Editor-rules engine: project-kind → IDE id. When a project's primary kind
    /// matches a key here, that IDE is suggested first (if installed).
    #[serde(default)]
    pub ide_rules: BTreeMap<String, String>,
    /// IDE id used when no `ide_rules` entry matches the project kind.
    #[serde(default)]
    pub ide_fallback: Option<String>,
    /// Auto-hand-off to a fresh agent near the context limit. Opt-out:
    /// `None`/`Some(true)` = on, `Some(false)` = disabled.
    #[serde(default)]
    pub auto_handoff: Option<bool>,
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

pub(crate) fn config_dir() -> Result<PathBuf, String> {
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
    Ok(config_dir()?.join("settings.json"))
}

fn is_project(dir: &Path) -> bool {
    MARKERS.iter().any(|m| dir.join(m).exists())
}

pub(crate) fn load() -> Settings {
    settings_path()
        .and_then(|p| std::fs::read_to_string(p).map_err(|e| e.to_string()))
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        .unwrap_or_default()
}

fn save(settings: &Settings) -> Result<Settings, String> {
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(settings_path()?, json).map_err(|e| e.to_string())?;
    // Return the persisted value so the frontend stays in sync in one round-trip.
    Ok(load())
}

#[tauri::command]
pub fn launch_context() -> Result<LaunchContext, String> {
    // A directory passed as an argument — `pade <dir>` from a terminal or the
    // folder's context menu — is an explicit request to open that project.
    if let Some(dir) = std::env::args().skip(1).find(|arg| Path::new(arg).is_dir()) {
        return Ok(LaunchContext {
            has_project: true,
            cwd: dir,
        });
    }
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    Ok(LaunchContext {
        has_project: is_project(&cwd),
        cwd: cwd.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn settings_get() -> Settings {
    load()
}

#[tauri::command]
pub fn workspace_add_root(path: String) -> Result<Settings, String> {
    let mut s = load();
    if !s.roots.contains(&path) {
        s.roots.push(path);
    }
    save(&s)
}

#[tauri::command]
pub fn workspace_remove_root(path: String) -> Result<Settings, String> {
    let mut s = load();
    s.roots.retain(|r| r != &path);
    save(&s)
}

/// Immediate sub-directories of `root` that look like projects.
#[tauri::command]
pub fn workspace_scan(root: String) -> Result<Vec<ProjectEntry>, String> {
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

/// Push a path to the front of the recent list (deduped, capped).
fn record_recent(settings: &mut Settings, path: &str) {
    settings.recent_projects.retain(|p| p != path);
    settings.recent_projects.insert(0, path.to_string());
    settings.recent_projects.truncate(RECENT_CAP);
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
pub fn workspace_temp() -> Result<String, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dir = config_dir()?
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
pub fn workspace_move(from: String, dest_dir: String) -> Result<String, String> {
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
    retarget(&mut settings, &from, &dest_str);
    save(&settings)?;
    workspace_open(dest_str.clone())?;
    Ok(dest_str)
}

/// Rename a temp workspace, promoting it into the primary project root
/// (`roots[0]`) under the new name — turning it into a real project.
#[tauri::command]
pub fn workspace_rename(from: String, new_name: String) -> Result<String, String> {
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
    retarget(&mut settings, &from, &dest_str);
    save(&settings)?;
    workspace_open(dest_str.clone())?;
    Ok(dest_str)
}

/// Delete an ADE-owned workspace directory and forget it.
#[tauri::command]
pub fn workspace_delete(path: String) -> Result<Settings, String> {
    let mut settings = load();
    if !is_ade_owned(&settings, &path) {
        return Err("only ADE-created workspaces can be deleted".into());
    }
    std::fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
    settings.recent_projects.retain(|p| p != &path);
    settings.owned_workspaces.retain(|p| p != &path);
    settings.labels.remove(&path);
    save(&settings)
}

/// Create a new project directory under `root` and open it.
#[tauri::command]
pub fn workspace_create(root: String, name: String) -> Result<String, String> {
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

/// Replace appearance/editor preferences (frontend sends the full set).
#[tauri::command]
pub fn set_prefs(prefs: Prefs) -> Result<Settings, String> {
    let mut s = load();
    s.prefs = prefs;
    save(&s)
}

/// Derive a directory name from a repo URL: the last path segment sans `.git`.
fn repo_dir_name(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("repo")
        .trim_end_matches(".git")
        .to_string()
}

/// Clone a version-control repo into `root` and open it. Git for the MVP; the
/// same seam extends to other VCSes later.
#[tauri::command]
pub fn workspace_clone(root: String, url: String) -> Result<String, String> {
    let name = repo_dir_name(&url);
    let dest = Path::new(&root).join(&name);
    let out = std::process::Command::new("git")
        .args(["clone", &url, &dest.to_string_lossy()])
        .current_dir(&root)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let path = dest.to_string_lossy().into_owned();
    workspace_open(path.clone())?;
    Ok(path)
}
