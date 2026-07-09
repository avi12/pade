//! Small cross-cutting helpers shared by multiple modules (DRY).

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
