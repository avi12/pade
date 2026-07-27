//! Language-agnostic re-provisioning for a saved workspace.
//!
//! When a temp workspace is saved into a real project, its dependency
//! directories (`node_modules`, a Python `.venv`, …) are NOT copied — they are
//! large, machine-specific, and trivially re-downloadable. This module is the one
//! authoritative table pairing each ecosystem's dependency directories (what the
//! copy skips) with the install command that restores them (what the app runs in
//! a task pane afterwards), so the two can never drift: skip `node_modules`,
//! reinstall with the project's package manager; skip `.venv`, reinstall with its
//! Python tool; and so on.

use std::path::Path;

/// One project ecosystem: how to recognise it, which dependency directories it
/// keeps (skipped by the copy), and the command that restores them.
struct Ecosystem {
    /// Marker files identifying the ecosystem, most specific first — a lockfile
    /// pins the exact tool, a bare manifest is the fallback. The FIRST marker that
    /// exists on disk selects this ecosystem's `install`.
    markers: &'static [&'static str],
    /// Dependency directory names re-created by `install`, so the copy can skip
    /// them. Empty when the ecosystem caches dependencies outside the project
    /// (Rust in `~/.cargo`, Go's module cache) — nothing in-tree to skip.
    dependency_directories: &'static [&'static str],
    /// The command that restores the dependencies. Also the pane's label.
    install: &'static str,
}

/// The ecosystems PADE knows how to re-provision, checked in order — a JS lockfile
/// wins over a bare `package.json`, `uv.lock` over `requirements.txt`. One home
/// for both halves of the skip/reinstall contract.
const ECOSYSTEMS: &[Ecosystem] = &[
    // JavaScript / TypeScript — the lockfile picks the package manager.
    Ecosystem {
        markers: &["pnpm-lock.yaml"],
        dependency_directories: &["node_modules"],
        install: "pnpm install",
    },
    Ecosystem {
        markers: &["bun.lockb", "bun.lock"],
        dependency_directories: &["node_modules"],
        install: "bun install",
    },
    Ecosystem {
        markers: &["yarn.lock"],
        dependency_directories: &["node_modules"],
        install: "yarn",
    },
    Ecosystem {
        markers: &["package-lock.json", "package.json"],
        dependency_directories: &["node_modules"],
        install: "npm install",
    },
    // Python — the lockfile / manifest picks the tool.
    Ecosystem {
        markers: &["uv.lock"],
        dependency_directories: &[".venv", "venv"],
        install: "uv sync",
    },
    Ecosystem {
        markers: &["poetry.lock"],
        dependency_directories: &[".venv", "venv"],
        install: "poetry install",
    },
    Ecosystem {
        markers: &["Pipfile.lock", "Pipfile"],
        dependency_directories: &[".venv", "venv"],
        install: "pipenv install",
    },
    Ecosystem {
        markers: &["requirements.txt"],
        dependency_directories: &[".venv", "venv"],
        install: "pip install -r requirements.txt",
    },
    // PHP.
    Ecosystem {
        markers: &["composer.json"],
        dependency_directories: &["vendor"],
        install: "composer install",
    },
    // Ruby — gems install outside the tree.
    Ecosystem {
        markers: &["Gemfile.lock", "Gemfile"],
        dependency_directories: &[],
        install: "bundle install",
    },
    // Go — modules live in the global module cache.
    Ecosystem {
        markers: &["go.mod"],
        dependency_directories: &[],
        install: "go mod download",
    },
];

/// Is `name` a dependency directory some ecosystem re-creates on install? Those
/// are the directories the save-copy skips — large, machine-specific, and
/// restored by [`detect_install`]. Matched case-insensitively so a `Node_Modules`
/// on a case-preserving Windows volume is still recognised.
pub fn is_dependency_directory(name: &str) -> bool {
    ECOSYSTEMS.iter().any(|ecosystem| {
        ecosystem
            .dependency_directories
            .iter()
            .any(|directory| directory.eq_ignore_ascii_case(name))
    })
}

/// The install command that restores `directory`'s dependencies, or `None` when
/// no ecosystem is recognised (nothing to reinstall — the copy carried
/// everything). The first ecosystem with a marker file present wins, so the exact
/// package manager is chosen from its lockfile.
pub fn detect_install(directory: &Path) -> Option<String> {
    for ecosystem in ECOSYSTEMS {
        let recognised = ecosystem
            .markers
            .iter()
            .any(|marker| directory.join(marker).is_file());
        if recognised {
            return Some(ecosystem.install.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{detect_install, is_dependency_directory};
    use std::path::PathBuf;

    /// A unique, pre-created scratch directory under the OS temp dir — the same
    /// convention `members.rs` / `mcp.rs` tests use, so no extra dependency.
    fn scratch(test: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("pade-provision-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create scratch");
        directory
    }

    #[test]
    fn recognises_dependency_directories_case_insensitively() {
        assert!(is_dependency_directory("node_modules"));
        assert!(is_dependency_directory("Node_Modules"));
        assert!(is_dependency_directory(".venv"));
        assert!(is_dependency_directory("vendor"));
    }

    #[test]
    fn keeps_build_and_source_directories() {
        assert!(!is_dependency_directory("dist"));
        assert!(!is_dependency_directory("build"));
        assert!(!is_dependency_directory("target"));
        assert!(!is_dependency_directory("src"));
        assert!(!is_dependency_directory(".git"));
    }

    #[test]
    fn a_lockfile_pins_the_package_manager_over_the_manifest() {
        let directory = scratch("lockfile");
        std::fs::write(directory.join("package.json"), "{}").expect("write manifest");
        assert_eq!(detect_install(&directory).as_deref(), Some("npm install"));

        std::fs::write(directory.join("pnpm-lock.yaml"), "").expect("write lockfile");
        assert_eq!(detect_install(&directory).as_deref(), Some("pnpm install"));
    }

    #[test]
    fn detects_python_and_returns_none_when_unrecognised() {
        let directory = scratch("python");
        assert_eq!(detect_install(&directory), None);

        std::fs::write(directory.join("requirements.txt"), "").expect("write requirements");
        assert_eq!(
            detect_install(&directory).as_deref(),
            Some("pip install -r requirements.txt")
        );
    }
}
