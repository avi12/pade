//! Re-running the package manager after a move, for the dependency folders whose links held the old path.

use crate::util::is_on_path;
use std::path::Path;

// ---------------------------------------------------------------------------
// Adapter - package-manager reconcile (cross-platform).
// ---------------------------------------------------------------------------

/// A package manager we can re-materialize after a move: if `lockfile` is present
/// in the moved project and the manager's `tool` is on PATH, run `command` there
/// to regenerate its (absolute-link-laden) install tree from scratch. This is the
/// robust "easy way" that complements `fix_project_symlinks` - the fixer re-points
/// links in place; this rebuilds them wholesale where a manager exists.
pub(super) struct PackageManager {
    /// Lockfile whose presence marks this manager as the project's (e.g.
    /// `pnpm-lock.yaml`).
    lockfile: &'static str,
    /// Executable that must be on PATH for the reconcile to run (e.g. `pnpm`).
    tool: &'static str,
    /// Command line run (through the shell) in the new project dir.
    command: &'static str,
}

/// Managers we know how to reconcile, in priority order - the first whose
/// lockfile is present and whose tool is on PATH wins (a project uses one). The
/// surest fix after a move is to let the manager rebuild its links/store.
pub(super) const PACKAGE_MANAGERS: &[PackageManager] = &[
    PackageManager {
        lockfile: "pnpm-lock.yaml",
        tool: "pnpm",
        command: "pnpm install",
    },
    PackageManager {
        lockfile: "package-lock.json",
        tool: "npm",
        command: "npm install",
    },
    PackageManager {
        lockfile: "yarn.lock",
        tool: "yarn",
        command: "yarn install",
    },
    // Bun's lockfile is `bun.lockb` (binary) or `bun.lock` (text, newer) - match either.
    PackageManager {
        lockfile: "bun.lockb",
        tool: "bun",
        command: "bun install",
    },
    PackageManager {
        lockfile: "bun.lock",
        tool: "bun",
        command: "bun install",
    },
];

/// If the moved project uses a known package manager (lockfile present + tool on
/// PATH), kick off its install in the new dir to regenerate the dependency tree.
///
/// Fire-and-forget by design: we **spawn detached and never wait** so the move
/// stays fast - the install rebuilds in the background. The child handle is
/// dropped (never joined, never killed). Best-effort: a missing tool or spawn
/// failure is logged and ignored, never failing the move.
pub(super) fn reconcile_package_manager(new_directory: &Path) {
    // One package manager per project - run only the first match.
    let project_manager = PACKAGE_MANAGERS
        .iter()
        .find(|manager| new_directory.join(manager.lockfile).is_file() && is_on_path(manager.tool));
    if let Some(manager) = project_manager {
        spawn_install(new_directory, manager.command);
    }
}

/// Spawn `command` through the platform shell in `directory` (see
/// [`crate::util::shell_command`]), detached. Stdio is nulled so the background
/// install doesn't tie its pipes to us; the handle is dropped so we neither wait
/// nor kill it.
pub(super) fn spawn_install(directory: &Path, command: &str) {
    use std::process::Stdio;

    let mut process = crate::util::shell_command(command);
    process
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Spawn and drop the handle - detached, no wait, no kill.
    if let Err(error) = process.spawn() {
        eprintln!(
            "refs: failed to start `{command}` in {}: {error}",
            directory.display()
        );
    }
}
