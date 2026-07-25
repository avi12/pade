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
/// Mirrors `naming.rs`'s `SKIPPED_DIRECTORIES`; hidden directories are skipped separately.
const SKIPPED_DIRECTORIES: &[&str] = &[
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

/// One supported task manifest exposed to the frontend for refresh filtering
/// and empty-state copy. The registry below remains the sole authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskManifestDescriptor {
    file: &'static str,
    label: &'static str,
}

/// List every runnable task in `cwd`, grouped by manifest. The caller supplies
/// the window's workspace because multiple PADE windows share one process.
#[tauri::command]
pub async fn tasks_list(cwd: String) -> Result<Vec<TaskGroup>, String> {
    let root = PathBuf::from(cwd);
    if !root.is_dir() {
        return Err(format!(
            "task workspace is not a directory: {}",
            root.display()
        ));
    }
    let mut groups = Vec::new();
    for dir in manifest_directories(&root) {
        collect_group(&root, &dir, &mut groups);
    }
    Ok(groups)
}

/// Describe every manifest understood by [`tasks_list`], in discovery order.
#[tauri::command]
pub fn tasks_descriptors() -> Vec<TaskManifestDescriptor> {
    MANIFESTS
        .iter()
        .map(ManifestDefinition::descriptor)
        .collect()
}

/// Walk the project (bounded depth, skipping noise) and yield every directory
/// worth inspecting for manifests. Mirrors `naming.rs`'s iterative walk.
fn manifest_directories(root: &Path) -> Vec<PathBuf> {
    const MAXIMUM_DEPTH: u8 = 3;
    let mut directories = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0u8)];
    while let Some((dir, depth)) = stack.pop() {
        directories.push(dir.clone());
        if depth >= MAXIMUM_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let is_noise_directory =
                name.starts_with('.') || SKIPPED_DIRECTORIES.contains(&name.as_ref());
            if is_noise_directory {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, depth + 1));
            }
        }
    }
    directories
}

/// One manifest ADE understands: its filename, its family, and how to parse its
/// tasks. The registry (`MANIFESTS`) is the single source of truth.
struct ManifestDefinition {
    file: &'static str,
    label: &'static str,
    kind: &'static str,
    extract: fn(&Path) -> Vec<Task>,
}

impl ManifestDefinition {
    fn descriptor(&self) -> TaskManifestDescriptor {
        TaskManifestDescriptor {
            file: self.file,
            label: self.label,
        }
    }
}

const MANIFESTS: &[ManifestDefinition] = &[
    ManifestDefinition {
        file: "package.json",
        label: "package.json",
        kind: "npm",
        extract: npm_tasks,
    },
    ManifestDefinition {
        file: "Cargo.toml",
        label: "Cargo.toml",
        kind: "cargo",
        extract: cargo_tasks,
    },
    ManifestDefinition {
        file: "Makefile",
        label: "a Makefile",
        kind: "make",
        extract: make_tasks,
    },
    ManifestDefinition {
        file: "pyproject.toml",
        label: "pyproject.toml",
        kind: "python",
        extract: python_tasks,
    },
];

/// Extract this directory's manifests into `groups` (a monorepo dir can hold
/// several — e.g. a `package.json` and a `Cargo.toml` side by side).
fn collect_group(root: &Path, dir: &Path, groups: &mut Vec<TaskGroup>) {
    for definition in MANIFESTS {
        let path = dir.join(definition.file);
        if !path.is_file() {
            continue;
        }
        let tasks = (definition.extract)(&path);
        if tasks.is_empty() {
            continue;
        }
        groups.push(TaskGroup {
            manifest: relative_display(root, &path),
            dir: dir.to_string_lossy().into_owned(),
            kind: definition.kind.to_string(),
            tasks,
        });
    }
}

/// A manifest's path relative to the project root, `/`-joined for display.
fn relative_display(root: &Path, path: &Path) -> String {
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
    let package_manager = PackageManager::detect(dir);
    scripts
        .keys()
        .map(|name| Task {
            command: package_manager.run_command(name),
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
    fn executable(self) -> &'static str {
        match self {
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Npm => "npm",
        }
    }

    /// The command to run an npm script: `npm run <s>`, but `pnpm <s>` / `yarn <s>`
    /// for those managers (both accept the bare-script shorthand).
    fn run_command(self, script: &str) -> String {
        let executable = self.executable();
        match self {
            PackageManager::Pnpm | PackageManager::Yarn => format!("{executable} {script}"),
            PackageManager::Npm => format!("{executable} run {script}"),
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
    make_tasks_from_text(&text)
}

/// The pure scan behind [`make_tasks`]: targets from Makefile text.
fn make_tasks_from_text(text: &str) -> Vec<Task> {
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
        && head.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
        });
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
    python_tasks_from_text(&text)
}

/// The pure scan behind [`python_tasks`]: script keys from pyproject text.
fn python_tasks_from_text(text: &str) -> Vec<Task> {
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
            .map(|key| key.trim().trim_matches('"'))
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        cargo_tasks, make_target, make_tasks_from_text, python_tasks_from_text, relative_display,
        tasks_descriptors, PackageManager, Task, TaskManifestDescriptor,
    };

    fn names(tasks: &[Task]) -> Vec<&str> {
        tasks.iter().map(|task| task.name.as_str()).collect()
    }

    #[test]
    fn descriptors_preserve_registry_order_and_display_labels() {
        assert_eq!(
            tasks_descriptors(),
            [
                TaskManifestDescriptor {
                    file: "package.json",
                    label: "package.json",
                },
                TaskManifestDescriptor {
                    file: "Cargo.toml",
                    label: "Cargo.toml",
                },
                TaskManifestDescriptor {
                    file: "Makefile",
                    label: "a Makefile",
                },
                TaskManifestDescriptor {
                    file: "pyproject.toml",
                    label: "pyproject.toml",
                },
            ]
        );
    }

    #[test]
    fn make_target_reads_a_rule_head() {
        assert_eq!(make_target("build: src/main.rs"), Some("build".to_string()));
        assert_eq!(make_target("lint-all_v2:"), Some("lint-all_v2".to_string()));
    }

    #[test]
    fn make_target_ignores_indented_recipe_bodies() {
        assert_eq!(make_target("\tcargo build"), None);
        assert_eq!(make_target("    echo done"), None);
    }

    #[test]
    fn make_target_ignores_blank_and_comment_lines() {
        assert_eq!(make_target(""), None);
        assert_eq!(make_target("# a comment"), None);
    }

    #[test]
    fn make_target_ignores_a_plain_variable_assignment() {
        assert_eq!(make_target("CC = gcc"), None);
    }

    #[test]
    fn make_target_treats_a_colon_assignment_like_a_rule_head() {
        assert_eq!(make_target("VAR := value"), Some("VAR".to_string()));
    }

    #[test]
    fn make_target_still_yields_dot_directives_for_the_caller_to_filter() {
        assert_eq!(make_target(".PHONY: build"), Some(".PHONY".to_string()));
    }

    #[test]
    fn make_tasks_skip_dot_directives_and_duplicates() {
        let text = ".PHONY: build test\nbuild:\n\tcargo build\nbuild:\ntest:\n\tcargo test\n";
        let tasks = make_tasks_from_text(text);
        assert_eq!(names(&tasks), ["build", "test"]);
        assert!(tasks
            .iter()
            .all(|task| task.command == format!("make {}", task.name)));
    }

    #[test]
    fn python_tasks_read_project_scripts_entries() {
        let text = "[project]\nname = \"demo\"\n\n[project.scripts]\nserve = \"demo.cli:serve\"\nmigrate = \"demo.cli:migrate\"\n";
        let tasks = python_tasks_from_text(text);
        assert_eq!(names(&tasks), ["serve", "migrate"]);
        assert!(tasks.iter().all(|task| task.command == task.name));
    }

    #[test]
    fn python_tasks_read_poetry_scripts_entries() {
        let text = "[tool.poetry.scripts]\ncli = \"pkg:main\"\n";
        assert_eq!(names(&python_tasks_from_text(text)), ["cli"]);
    }

    #[test]
    fn python_tasks_trim_quoted_script_keys() {
        let text = "[project.scripts]\n\"lint-all\" = \"demo.cli:lint\"\n";
        assert_eq!(names(&python_tasks_from_text(text)), ["lint-all"]);
    }

    #[test]
    fn python_tasks_skip_comments_and_blank_lines_inside_the_table() {
        let text = "[project.scripts]\n# a comment\n\nserve = \"demo.cli:serve\"\n";
        assert_eq!(names(&python_tasks_from_text(text)), ["serve"]);
    }

    #[test]
    fn python_tasks_ignore_non_script_tables() {
        let text = "[tool.ruff]\nline-length = 100\n\n[project.scripts]\nserve = \"x\"\n\n[tool.pytest.ini_options]\naddopts = \"-q\"\n";
        assert_eq!(names(&python_tasks_from_text(text)), ["serve"]);
    }

    #[test]
    fn python_tasks_are_empty_without_a_scripts_table() {
        let text = "[project]\nname = \"demo\"\n";
        assert!(python_tasks_from_text(text).is_empty());
    }

    #[test]
    fn npm_runs_scripts_through_npm_run_but_pnpm_and_yarn_take_the_bare_script() {
        assert_eq!(PackageManager::Npm.run_command("dev"), "npm run dev");
        assert_eq!(PackageManager::Pnpm.run_command("dev"), "pnpm dev");
        assert_eq!(PackageManager::Yarn.run_command("dev"), "yarn dev");
    }

    #[test]
    fn cargo_manifests_always_offer_the_standard_verbs() {
        let tasks = cargo_tasks(Path::new("Cargo.toml"));
        assert_eq!(names(&tasks), ["build", "test", "run", "check", "clippy"]);
        assert!(tasks
            .iter()
            .all(|task| task.command == format!("cargo {}", task.name)));
    }

    #[test]
    fn relative_display_strips_the_root_and_joins_with_forward_slashes() {
        let root = Path::new("repo");
        let path = root.join("crates").join("core").join("Cargo.toml");
        assert_eq!(relative_display(root, &path), "crates/core/Cargo.toml");
    }

    #[test]
    fn relative_display_falls_back_to_the_full_path_outside_the_root() {
        let root = Path::new("alpha");
        let path = Path::new("beta").join("Makefile");
        assert_eq!(relative_display(root, &path), "beta/Makefile");
    }
}
