//! Task runner: discover runnable tasks from a project's manifests.
//!
//! Scans the open project (bounded depth, skipping build/dep noise - the same
//! list `naming.rs` walks by) for the manifests it understands, extracts each
//! one's runnable tasks, and hands the frontend a run command per task.
//! Monorepo-aware (one group per manifest found) and multi-language: npm,
//! cargo, make, python, cmake (lists and presets), meson, ninja, bazel, xmake,
//! premake, scons, qmake, dotnet, msbuild, go, gradle and maven.
//! Read-only; nothing is executed here - the UI opens a terminal.

mod dotnet;
mod jvm;
mod manifests;
mod native_build;
mod package_managers;

use std::path::{Path, PathBuf};

use serde::Serialize;

use self::manifests::MANIFESTS;
use crate::util::is_noise_directory;

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
    #[serde(rename = "dir")]
    directory: String,
    kind: TaskKind,
    tasks: Vec<Task>,
}

/// A manifest family - the closed set the frontend groups and labels by,
/// serialized as its lowercase name (`"npm"`, `"msbuild"`, ...).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum TaskKind {
    Npm,
    Cargo,
    Make,
    Python,
    Cmake,
    Meson,
    Ninja,
    Bazel,
    Xmake,
    Premake,
    Scons,
    Qmake,
    Dotnet,
    Msbuild,
    Go,
    Gradle,
    Maven,
}

/// One token the frontend watches for (so a manifest landing on disk refreshes
/// the panel) and names in the empty state. A manifest is recognised either by
/// an exact file name or by an extension, and the frontend has to know which so
/// it can match a changed path the same way this module does. The registry
/// below remains the sole authority for both.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "match", rename_all = "camelCase")]
pub enum TaskManifestDescriptor {
    /// An exact file name: `package.json`.
    Name { value: &'static str },
    /// A file extension, dot included: `.csproj`.
    Extension { value: &'static str },
}

/// List every runnable task in `cwd`, grouped by manifest. The caller supplies
/// the window's workspace because multiple PADE windows share one process.
/// The tree walk and manifest reads run on the blocking pool, off the async
/// workers.
#[tauri::command]
pub async fn tasks_list(cwd: String) -> Result<Vec<TaskGroup>, String> {
    tauri::async_runtime::spawn_blocking(move || list_tasks(&PathBuf::from(cwd)))
        .await
        .map_err(|error| error.to_string())?
}

fn list_tasks(root: &Path) -> Result<Vec<TaskGroup>, String> {
    if !root.is_dir() {
        return Err(format!(
            "task workspace is not a directory: {}",
            root.display()
        ));
    }
    let mut groups = Vec::new();
    for directory in manifest_directories(root) {
        collect_group(root, &directory, &mut groups);
    }
    Ok(groups)
}

/// Describe every manifest understood by [`tasks_list`], in discovery order.
/// Deduplicated: two families can recognise the same token - a solution is read
/// by both the .NET and the MSVC extractor - and the frontend wants the tokens,
/// not the registry rows.
#[tauri::command]
pub fn tasks_descriptors() -> Vec<TaskManifestDescriptor> {
    let mut descriptors: Vec<TaskManifestDescriptor> = Vec::new();
    for definition in MANIFESTS {
        for descriptor in definition.matcher.descriptors() {
            if !descriptors.contains(&descriptor) {
                descriptors.push(descriptor);
            }
        }
    }
    descriptors
}

/// Walk the project (bounded depth, skipping noise) and yield every directory
/// worth inspecting for manifests. Mirrors `naming.rs`'s iterative walk.
fn manifest_directories(root: &Path) -> Vec<PathBuf> {
    const MAXIMUM_DEPTH: u8 = 3;
    let mut directories = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0u8)];
    while let Some((directory, depth)) = stack.pop() {
        directories.push(directory.clone());
        if depth >= MAXIMUM_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if is_noise_directory(&name.to_string_lossy()) {
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

/// Extract this directory's manifests into `groups` (a monorepo directory can hold
/// several - e.g. a `package.json` and a `Cargo.toml` side by side, or three
/// sibling `.csproj` files). One `read_dir` serves every definition, since an
/// extension match has to see the directory's real file names anyway.
fn collect_group(root: &Path, directory: &Path, groups: &mut Vec<TaskGroup>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut file_names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    file_names.sort();
    // Lowercased once per file, not once per file per registry row.
    let lowercase_names: Vec<String> = file_names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    for definition in MANIFESTS {
        let matching_names = file_names
            .iter()
            .zip(&lowercase_names)
            .filter(|(_, lowercase)| definition.matcher.matches(lowercase))
            .map(|(file_name, _)| file_name);
        for file_name in matching_names {
            let path = directory.join(file_name);
            let tasks = (definition.extract)(&path);
            if tasks.is_empty() {
                continue;
            }
            groups.push(TaskGroup {
                manifest: relative_display(root, &path),
                directory: directory.to_string_lossy().into_owned(),
                kind: definition.kind,
                tasks,
            });
        }
    }
}

/// A manifest's path relative to the project root, `/`-joined for display.
fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// One task, from the name shown in the panel and the command behind it.
fn task(name: &str, command: String) -> Task {
    Task {
        name: name.to_string(),
        command,
    }
}

/// The manifest's own file name, quoted for the command line: a directory can
/// hold several `.csproj` files, so every command names the one it builds, and
/// project names contain spaces often enough to need the quotes.
fn quoted_file_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    format!("\"{name}\"")
}

#[cfg(test)]
mod tests;
