//! Small cross-cutting helpers shared by multiple modules (DRY).

use std::path::PathBuf;
use std::process::Command;

/// Is `command` resolvable on PATH? Uses the platform's own resolver (`where`
/// on Windows, `which` elsewhere) so shims (.cmd/.ps1) resolve as a shell would.
pub fn is_on_path(command: &str) -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    Command::new(finder)
        .arg(command)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The user's home directory, cross-platform, without pulling in a dependency
/// (`USERPROFILE` on Windows, `HOME` elsewhere).
pub fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from)
}

/// Encode an absolute path to Claude Code's project-dir name: drive colon and
/// both separators (`:` `\` `/`) collapse to `-`. Claude stores each project's
/// transcript under `~/.claude/projects/<encoded-path>/`.
pub fn encode_project(path: &str) -> String {
    path.chars()
        .map(|c| {
            if matches!(c, ':' | '\\' | '/') {
                '-'
            } else {
                c
            }
        })
        .collect()
}
