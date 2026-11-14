//! Paths: PADE's data folder, the home directory, the spelling-independent path comparison and the directories no project walk enters.

use std::path::{Path, PathBuf};

/// Directories no project walk is ever interested in: dependency trees and
/// build output. One home for the list, because two walks read it - the
/// autonamer's file census (`naming.rs`) and the task-manifest scan
/// (`tasks.rs`) - and a directory worth skipping in one is worth skipping in
/// the other. Hidden directories are skipped separately by each walk, so `.git`
/// and friends need no entry here.
const SKIPPED_DIRECTORIES: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "bin",
    "obj",
    "out",
    "vendor",
    ".git",
    ".svelte-kit",
    ".vite",
];

/// Whether a directory name is one a project walk skips: build output, a
/// dependency tree, or any hidden directory.
pub(crate) fn is_noise_directory(name: &str) -> bool {
    name.starts_with('.') || SKIPPED_DIRECTORIES.contains(&name)
}

/// A directory's identity for de-duping (see [`comparable_path`]).
pub(super) fn directory_key(directory: &Path) -> String {
    comparable_path(&directory.to_string_lossy())
}

/// A path's spelling-independent form for comparison - `/`-separated, no
/// trailing slash, lowercased on Windows, where `C:\Foo` and `c:/foo/` name the
/// same folder. The one rule every "is this the same path" check reads.
pub(crate) fn comparable_path(path: &str) -> String {
    let forward_slashed = path.replace('\\', "/");
    let trimmed = forward_slashed.trim_end_matches('/');
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    }
}

/// Whether two paths name the same folder, however each was spelled.
pub(crate) fn same_path(left: &str, right: &str) -> bool {
    comparable_path(left) == comparable_path(right)
}

/// PADE's own per-user data folder: `%LOCALAPPDATA%\pade` on Windows,
/// `~/.pade` elsewhere. The one place every PADE-owned folder hangs off.
pub(crate) fn pade_data_directory() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("pade"));
    }
    Some(home_directory()?.join(".pade"))
}

/// The user's home directory, cross-platform, without pulling in a dependency
/// (`USERPROFILE` on Windows, `HOME` elsewhere).
pub(crate) fn home_directory() -> Option<PathBuf> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(variable).map(PathBuf::from)
}
