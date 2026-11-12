//! Package-manager manifests: `package.json`, `Cargo.toml`, `pyproject.toml` and `go.mod`.

use std::path::Path;

use super::{task, Task};

/// `package.json` - each `scripts` key is a task; the package manager is picked
/// from the lockfile in the same directory (pnpm/yarn/npm).
pub(super) fn npm_tasks(path: &Path) -> Vec<Task> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = json.get("scripts").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let directory = path.parent().unwrap_or(path);
    let package_manager = PackageManager::detect(directory);
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
pub(super) enum PackageManager {
    Pnpm,
    Yarn,
    Npm,
}

impl PackageManager {
    /// Detect from a lockfile in `directory`: pnpm, then yarn, else npm.
    pub(super) fn detect(directory: &Path) -> Self {
        if directory.join("pnpm-lock.yaml").exists() {
            PackageManager::Pnpm
        } else if directory.join("yarn.lock").exists() {
            PackageManager::Yarn
        } else {
            PackageManager::Npm
        }
    }

    /// The launcher binary name (the only place these literals live).
    pub(super) fn executable(self) -> &'static str {
        match self {
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Npm => "npm",
        }
    }

    /// The command to run an npm script: `npm run <s>`, but `pnpm <s>` / `yarn <s>`
    /// for those managers (both accept the bare-script shorthand).
    pub(super) fn run_command(self, script: &str) -> String {
        let executable = self.executable();
        match self {
            PackageManager::Pnpm | PackageManager::Yarn => format!("{executable} {script}"),
            PackageManager::Npm => format!("{executable} run {script}"),
        }
    }
}

/// `Cargo.toml` - the standard cargo verbs, always available for a crate.
pub(super) fn cargo_tasks(_path: &Path) -> Vec<Task> {
    ["build", "test", "run", "check", "clippy"]
        .into_iter()
        .map(|verb| Task {
            name: verb.to_string(),
            command: format!("cargo {verb}"),
        })
        .collect()
}

/// `pyproject.toml` - console-script keys under `[project.scripts]` or
/// `[tool.poetry.scripts]`; the command is the script name itself. Light line
/// scan (no toml crate). Empty if neither table has entries.
pub(super) fn python_tasks(path: &Path) -> Vec<Task> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    python_tasks_from_text(&text)
}

/// The pure scan behind [`python_tasks`]: script keys from pyproject text.
pub(super) fn python_tasks_from_text(text: &str) -> Vec<Task> {
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

/// `go.mod` - the module-wide verbs. `./...` is every package under the module,
/// which is what a module-level build or test means.
pub(super) fn go_tasks(_path: &Path) -> Vec<Task> {
    [
        ("build", "go build ./..."),
        ("run", "go run ."),
        ("test", "go test ./..."),
        ("vet", "go vet ./..."),
    ]
    .into_iter()
    .map(|(name, command)| task(name, command.to_string()))
    .collect()
}
