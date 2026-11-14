//! Workspace member discovery - the parts a workspace is actually made of.
//!
//! A member is one of two things, and one walk finds both:
//!
//! * a **package**, IFF the directory holds its own manifest - never because of
//!   its name. The root's workspace-defining files (`pnpm-workspace.yaml`,
//!   `package.json` `"workspaces"`, `Cargo.toml` `[workspace]`, `go.work`,
//!   `pyproject.toml` `[tool.uv.workspace]`) contribute include/exclude member
//!   patterns, and a census directory becomes a package member only when an
//!   include pattern matches it and no exclude does.
//! * a **repository**, IFF the directory holds its own `.git` - a checkout
//!   nested inside the workspace (a multi-repo folder, a submodule, a
//!   worktree). No pattern has to claim it: nobody declares these anywhere, and
//!   a checkout with its own branch and history is part of the workspace
//!   whether or not a manifest agrees.
//!
//! The root itself is always the first member, so every changed file has an
//! enclosing bucket. Read-only; the frontend does the longest-prefix
//! file→member assignment (`change-groups.ts`) and reads the same list for the
//! member switcher, its branch chips and the feed's per-group branch.

mod patterns;
#[cfg(test)]
mod tests;
mod toml_text;

use patterns::{collect_patterns, glob_match, MemberPatterns};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use toml_text::toml_string_value;

/// The package ecosystems whose manifests confirm a directory as a package.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Ecosystem {
    JavaScript,
    Rust,
    #[serde(rename = "go")]
    Golang,
    Python,
}

/// Manifest filename → the ecosystem it confirms. Order is priority when one
/// directory holds several manifests (a JS app with a `Cargo.toml` sidecar
/// reads its name from `package.json`).
const MANIFESTS: &[(&str, Ecosystem)] = &[
    ("package.json", Ecosystem::JavaScript),
    ("Cargo.toml", Ecosystem::Rust),
    ("go.mod", Ecosystem::Golang),
    ("pyproject.toml", Ecosystem::Python),
];

/// Directories never worth walking (build output, deps, vendored code);
/// dot-directories (`.git`, `.venv`, …) are pruned separately.
const PRUNED_DIRECTORIES: &[&str] = &["node_modules", "target", "dist", "build", "vendor"];

/// What marks a directory as its own checkout - a directory in a normal clone,
/// a FILE in a worktree or submodule (it points at the real git dir). The walk
/// tests the name, never the kind, so all three count.
const GIT_MARKER: &str = ".git";

/// One confirmed workspace member, ready for the frontend's longest-prefix
/// bucket assignment.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Member {
    /// Repo-relative `/`-joined directory (`""` = the repo root).
    pub path: String,
    /// The package name its manifest declares, when it declares one.
    pub name: Option<String>,
    /// The manifest family that confirmed it; `None` for a checkout with no
    /// manifest, and for a root that has neither (kept so every file still has
    /// a bucket).
    pub ecosystem: Option<Ecosystem>,
    /// It holds its own `.git`, so it has a branch, a status and a history of
    /// its own. The frontend shows git chrome only where this is true.
    pub repository: bool,
}

/// Manifest-confirmed members of the workspace at `root` (the open project),
/// for the Change Feed's manifest-driven grouping. The root is always first.
#[tauri::command]
pub(crate) async fn members_list(root: String) -> Result<Vec<Member>, String> {
    let root = Path::new(&root);
    if !root.is_dir() {
        return Err(format!("not a directory: {}", root.display()));
    }
    Ok(discover_members(root))
}

/// Every confirmed member: the root, plus each census directory that is either
/// its own checkout or a package the declared patterns include (all includes
/// first, then excludes subtract - negation is order-sensitive in every
/// ecosystem).
fn discover_members(root: &Path) -> Vec<Member> {
    let census = census(root);
    let patterns = collect_patterns(root);

    let found_root = census.get("");
    let root_member = Member {
        path: String::new(),
        name: found_root.and_then(Found::manifest_name),
        ecosystem: found_root.and_then(Found::ecosystem),
        repository: found_root.is_some_and(|found| found.is_repository),
    };
    let nested_members = census
        .iter()
        .filter(|(path, found)| {
            !path.is_empty() && (found.is_repository || is_declared_package(path, &patterns, found))
        })
        .map(|(path, found)| Member {
            path: path.clone(),
            name: found.manifest_name(),
            ecosystem: found.ecosystem(),
            repository: found.is_repository,
        });
    std::iter::once(root_member).chain(nested_members).collect()
}

/// Whether the root's workspace files claim this directory as a member package.
/// A directory with no manifest is not a package, so nothing can claim it.
fn is_declared_package(path: &str, patterns: &MemberPatterns, found: &Found) -> bool {
    if found.manifest.is_none() {
        return false;
    }

    let included = patterns
        .include
        .iter()
        .any(|pattern| glob_match(pattern, path));
    if !included {
        return false;
    }

    !patterns
        .exclude
        .iter()
        .any(|pattern| glob_match(pattern, path))
}

/// A directory the census confirmed as a package.
struct FoundManifest {
    ecosystem: Ecosystem,
    name: Option<String>,
}

/// What a walked directory turned out to be: a package, a checkout, or both.
/// A directory that is neither is never recorded.
struct Found {
    manifest: Option<FoundManifest>,
    is_repository: bool,
}

impl Found {
    fn manifest_name(&self) -> Option<String> {
        self.manifest.as_ref().and_then(|found| found.name.clone())
    }

    fn ecosystem(&self) -> Option<Ecosystem> {
        self.manifest.as_ref().map(|found| found.ecosystem)
    }
}

/// Walk once from the root recording every directory that holds a real package
/// manifest or its own `.git`, pruning noise directories and hidden directories
/// during the walk (a greedy member glob must never sweep in `node_modules`).
/// A nested checkout is still walked into - a repo can hold packages of its own.
fn census(root: &Path) -> BTreeMap<String, Found> {
    let mut found = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        // Index into MANIFESTS of the highest-priority manifest seen so far.
        let mut best_manifest: Option<usize> = None;
        let mut is_repository = false;
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            is_repository = is_repository || name == GIT_MARKER;
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                let is_noise = name.starts_with('.') || PRUNED_DIRECTORIES.contains(&name.as_ref());
                if !is_noise {
                    stack.push(entry.path());
                }
                continue;
            }
            if let Some(index) = MANIFESTS.iter().position(|(manifest, _)| *manifest == name) {
                best_manifest = Some(best_manifest.map_or(index, |best| best.min(index)));
            }
        }
        let manifest = best_manifest.and_then(|index| MANIFESTS.get(index)).map(
            |(manifest_name, ecosystem)| FoundManifest {
                ecosystem: *ecosystem,
                name: package_name(&directory.join(manifest_name), *ecosystem),
            },
        );
        if manifest.is_none() && !is_repository {
            continue;
        }
        found.insert(
            relative_display(root, &directory),
            Found {
                manifest,
                is_repository,
            },
        );
    }
    found
}

/// A directory's path relative to the root, `/`-joined (`""` for the root).
fn relative_display(root: &Path, directory: &Path) -> String {
    directory
        .strip_prefix(root)
        .unwrap_or(directory)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The declared package name inside one manifest file.
fn package_name(manifest: &Path, ecosystem: Ecosystem) -> Option<String> {
    let text = fs::read_to_string(manifest).ok()?;
    package_name_from_text(&text, ecosystem)
}

/// The pure read behind [`package_name`], per manifest family.
fn package_name_from_text(text: &str, ecosystem: Ecosystem) -> Option<String> {
    match ecosystem {
        Ecosystem::JavaScript => {
            let json: serde_json::Value = serde_json::from_str(text).ok()?;
            json.get("name")?.as_str().map(str::to_string)
        }
        Ecosystem::Rust => toml_string_value(text, "package", "name"),
        Ecosystem::Golang => go_module_name(text),
        Ecosystem::Python => toml_string_value(text, "project", "name")
            .or_else(|| toml_string_value(text, "tool.poetry", "name")),
    }
}

/// `go.mod`'s `module example.com/team/api` → the short display name `api`.
fn go_module_name(text: &str) -> Option<String> {
    let module_path = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("module "))?;
    let short = module_path.trim().trim_matches('"').rsplit('/').next()?;
    if short.is_empty() {
        return None;
    }
    Some(short.to_string())
}
