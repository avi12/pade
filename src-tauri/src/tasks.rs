//! Task runner: discover runnable tasks from a project's manifests.
//!
//! Scans the open project (bounded depth, skipping build/dep noise — mirrors
//! `naming.rs`'s walk) for the manifests it understands, extracts each one's
//! runnable tasks, and hands the frontend a run command per task. Monorepo-aware
//! (one group per manifest found) and multi-language (npm / cargo / make /
//! python). Read-only; nothing is executed here — the UI opens a terminal.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Directories never worth scanning for manifests (build output, VCS, deps).
/// Mirrors `naming.rs`'s `SKIP_DIRS`; dotdirs are skipped separately.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".git",
    ".svelte-kit",
    ".vite",
];

/// One runnable task: a display `name` and the shell `command` that runs it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    name: String,
    command: String,
}

/// The tasks extracted from one manifest.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGroup {
    /// Manifest path relative to the project root (`/`-joined).
    manifest: String,
    /// Absolute directory the tasks run in.
    dir: String,
    /// Manifest family: "npm" | "cargo" | "make" | "python".
    kind: String,
    tasks: Vec<Task>,
}

/// List every runnable task in the open project, grouped by manifest.
#[tauri::command]
pub fn tasks_list() -> Result<Vec<TaskGroup>, String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let mut groups = Vec::new();
    for dir in manifest_dirs(&root) {
        collect_group(&root, &dir, &mut groups);
    }
    Ok(groups)
}

/// Walk the project (bounded depth, skipping noise) and yield every directory
/// worth inspecting for manifests. Mirrors `naming.rs`'s iterative walk.
fn manifest_dirs(root: &Path) -> Vec<PathBuf> {
    const MAX_DEPTH: u8 = 3;
    let mut dirs = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0u8)];
    while let Some((dir, depth)) = stack.pop() {
        dirs.push(dir.clone());
        if depth >= MAX_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let is_noise_dir = name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref());
            if is_noise_dir {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, depth + 1));
            }
        }
    }
    dirs
}

/// One manifest ADE understands: its filename, its family, and how to parse its
/// tasks. The registry (`MANIFESTS`) is the single source of truth.
struct ManifestDef {
    file: &'static str,
    kind: &'static str,
    extract: fn(&Path) -> Vec<Task>,
}

const MANIFESTS: &[ManifestDef] = &[
    ManifestDef {
        file: "package.json",
        kind: "npm",
        extract: npm_tasks,
    },
    ManifestDef {
        file: "Cargo.toml",
        kind: "cargo",
        extract: cargo_tasks,
    },
    ManifestDef {
        file: "Makefile",
        kind: "make",
        extract: make_tasks,
    },
    ManifestDef {
        file: "pyproject.toml",
        kind: "python",
        extract: python_tasks,
    },
];

/// Extract this directory's manifests into `groups` (a monorepo dir can hold
/// several — e.g. a `package.json` and a `Cargo.toml` side by side).
fn collect_group(root: &Path, dir: &Path, groups: &mut Vec<TaskGroup>) {
    for def in MANIFESTS {
        let path = dir.join(def.file);
        if !path.is_file() {
            continue;
        }
        let tasks = (def.extract)(&path);
        if tasks.is_empty() {
            continue;
        }
        groups.push(TaskGroup {
            manifest: rel_display(root, &path),
            dir: dir.to_string_lossy().into_owned(),
            kind: def.kind.to_string(),
            tasks,
        });
    }
}

/// A manifest's path relative to the project root, `/`-joined for display.
fn rel_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

// ── Manifest parsers ─────────────────────────────────────────────────────────

/// `package.json` — each `scripts` key is a task; the package manager is picked
/// from the lockfile in the same directory (pnpm/yarn/npm).
fn npm_tasks(path: &Path) -> Vec<Task> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = json.get("scripts").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let dir = path.parent().unwrap_or(path);
    let pm = PackageManager::detect(dir);
    scripts
        .keys()
        .map(|name| Task {
            command: pm.run_command(name),
            name: name.clone(),
        })
        .collect()
}

/// A JS package manager, the closed set ADE knows how to drive.
#[derive(Clone, Copy)]
enum PackageManager {
    Pnpm,
    Yarn,
    Npm,
}

impl PackageManager {
    /// Detect from a lockfile in `dir`: pnpm, then yarn, else npm.
    fn detect(dir: &Path) -> Self {
        if dir.join("pnpm-lock.yaml").exists() {
            PackageManager::Pnpm
        } else if dir.join("yarn.lock").exists() {
            PackageManager::Yarn
        } else {
            PackageManager::Npm
        }
    }

    /// The launcher binary name (the only place these literals live).
    fn bin(self) -> &'static str {
        match self {
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Npm => "npm",
        }
    }

    /// The command to run an npm script: `npm run <s>`, but `pnpm <s>` / `yarn <s>`
    /// for those managers (both accept the bare-script shorthand).
    fn run_command(self, script: &str) -> String {
        let bin = self.bin();
        match self {
            PackageManager::Pnpm | PackageManager::Yarn => format!("{bin} {script}"),
            PackageManager::Npm => format!("{bin} run {script}"),
        }
    }
}

/// `Cargo.toml` — the standard cargo verbs, always available for a crate.
fn cargo_tasks(_path: &Path) -> Vec<Task> {
    ["build", "test", "run", "check", "clippy"]
        .into_iter()
        .map(|verb| Task {
            name: verb.to_string(),
            command: format!("cargo {verb}"),
        })
        .collect()
}

/// `Makefile` — each target line (`^name:`), skipping directives (`.PHONY` etc.)
/// and duplicates. Light scan, no makefile grammar.
fn make_tasks(path: &Path) -> Vec<Task> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut seen = Vec::new();
    let mut tasks = Vec::new();
    for line in text.lines() {
        let Some(target) = make_target(line) else {
            continue;
        };
        if target.starts_with('.') || seen.contains(&target) {
            continue;
        }
        seen.push(target.clone());
        tasks.push(Task {
            command: format!("make {target}"),
            name: target,
        });
    }
    tasks
}

/// Pull a target name from a Makefile line: the token before a `:` that is made
/// only of `[A-Za-z0-9_.-]` (a rule head, not a variable or recipe body).
fn make_target(line: &str) -> Option<String> {
    if line.starts_with([' ', '\t']) {
        return None; // recipe body, not a rule head
    }
    let head = line.split(':').next()?.trim();
    let is_rule_head = !head.is_empty()
        && head
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    if !is_rule_head {
        return None;
    }
    Some(head.to_string())
}

/// `pyproject.toml` — console-script keys under `[project.scripts]` or
/// `[tool.poetry.scripts]`; the command is the script name itself. Light line
/// scan (no toml crate). Empty if neither table has entries.
fn python_tasks(path: &Path) -> Vec<Task> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut in_scripts = false;
    let mut tasks = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_scripts = matches!(trimmed, "[project.scripts]" | "[tool.poetry.scripts]");
            continue;
        }
        if !in_scripts || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(name) = trimmed
            .split('=')
            .next()
            .map(|k| k.trim().trim_matches('"'))
        else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        tasks.push(Task {
            name: name.to_string(),
            command: name.to_string(),
        });
    }
    tasks
}
