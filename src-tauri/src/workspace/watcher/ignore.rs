//! Which paths the Change Feed ignores: static directories, gitignore and git check-ignore.

use super::registry::ProjectRootPayload;
use super::registry::ProjectWatch;
use crate::vcs::git_in;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri::Emitter;

/// The git metadata entry a workspace gains when `git init` runs - the one
/// authoritative spelling of the `.git` name.
pub(super) const GIT_DIRECTORY_NAME: &str = ".git";

pub(super) const ALWAYS_IGNORED_DIRECTORIES: &[&str] = &[
    GIT_DIRECTORY_NAME,
    "node_modules",
    "target",
    "dist",
    "build",
    ".svelte-kit",
    ".ade",
    ".vite",
];

pub(super) fn is_always_ignored(path: &Path) -> bool {
    path_component_matches(path, |name| ALWAYS_IGNORED_DIRECTORIES.contains(&name))
}

/// Whether any component of `path` names an ignored directory, decided by the
/// `is_ignored_name` predicate. One authoritative component scan shared by the
/// always-on baseline [`ignored`] pre-filter (against the [`ALWAYS_IGNORED_DIRECTORIES`] slice) and
/// static-mode exclusion (against a tech-inferred [`HashSet`]).
pub(super) fn path_component_matches(path: &Path, is_ignored_name: impl Fn(&str) -> bool) -> bool {
    path.components()
        .filter_map(|component| component.as_os_str().to_str())
        .any(is_ignored_name)
}

/// Static-mode exclusion: whether `path` lies under any directory named in `directories`.
pub(super) fn is_under_static_ignore_directory(path: &Path, directories: &HashSet<String>) -> bool {
    path_component_matches(path, |name| directories.contains(name))
}

/// How a watch decides a path is "ignored", fixed once per watch root (when
/// `watch_projects` arms the Change Feed on it) and held in its [`IgnoreRules`].
///
/// A git work tree defers to git itself via `git check-ignore`, which honors
/// nested `.gitignore` files, `.git/info/exclude`, the global `core.excludesFile`,
/// and negation rules - exactly what the user's git already ignores, rather than a
/// brittle hand-rolled parse. A folder that is not a git repo has none of those
/// tools, so it falls back to a set of directory names inferred from the manifest
/// files at the root (see [`manifest_ignore_directories`]) unioned with the [`ALWAYS_IGNORED_DIRECTORIES`]
/// baseline - plus the root `.gitignore`'s own rules when the folder has one (a
/// scaffolded project knows what it will generate before its `git init` runs).
///
/// The policy is NOT fixed for the watch's lifetime: any `.gitignore`
/// change/appearance/deletion, and a mid-session `git init`, recompute it (see
/// [`refresh_ignore_policy`]) and announce [`FEED_IGNORE_EVENT`] so the frontend
/// re-filters what it already shows.
pub(super) enum IgnorePolicy {
    /// The root is a git work tree; ask `git check-ignore` per path (memoized).
    Git { root: PathBuf },
    /// The root is not a git repo; exclude any path whose component names one of
    /// these tech-inferred directories, or that the root `.gitignore` ignores.
    Static {
        root: PathBuf,
        directories: HashSet<String>,
        rules: crate::workspace::gitignore::Rules,
    },
}

/// One watch root's [`IgnorePolicy`] plus git's memoized answers under it - the
/// one ignore filter shared by the Change Feed and the language watch, so both
/// skip exactly what the project's own git skips.
#[derive(Default)]
pub(super) struct IgnoreRules {
    /// `None` until a policy is installed. Git mode defers to git's own ignore
    /// rules; static mode carries a tech-inferred set of directory names.
    policy: Mutex<Option<IgnorePolicy>>,
    /// Memoized `git check-ignore` results per absolute path (git mode only), so
    /// a path shells git at most once. Cleared with every new policy, since
    /// editing the rules can flip any path's state.
    git_ignore_cache: Mutex<HashMap<PathBuf, bool>>,
}

impl IgnoreRules {
    /// Adopt a freshly computed policy and forget every memoized decision.
    pub(super) fn install(&self, policy: IgnorePolicy) {
        if let Ok(mut guard) = self.policy.lock() {
            *guard = Some(policy);
        }
        if let Ok(mut cache) = self.git_ignore_cache.lock() {
            cache.clear();
        }
    }

    pub(super) fn is_installed(&self) -> bool {
        self.policy.lock().is_ok_and(|guard| guard.is_some())
    }

    /// Whether the policy excludes `path`. Git mode asks git (memoized per
    /// path); static mode matches the path's component names against the
    /// tech-inferred ignore set. A poisoned lock, an unset policy, or a git
    /// failure all resolve to "not excluded" so a fault never silently hides
    /// real changes.
    pub(super) fn excludes(&self, path: &Path) -> bool {
        let git_root = {
            let Ok(guard) = self.policy.lock() else {
                return false;
            };
            match guard.as_ref() {
                None => return false,
                Some(IgnorePolicy::Static {
                    root,
                    directories,
                    rules,
                }) => {
                    return is_under_static_ignore_directory(path, directories)
                        || gitignore_excludes(root, rules, path);
                }
                Some(IgnorePolicy::Git { root }) => root.clone(),
            }
        };
        self.git_excludes(&git_root, path)
    }

    /// Whether git considers `path` ignored under the work tree at `root`,
    /// memoized so each path shells git at most once.
    fn git_excludes(&self, root: &Path, path: &Path) -> bool {
        let memoized = self
            .git_ignore_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(path).copied());
        if let Some(is_ignored) = memoized {
            return is_ignored;
        }
        let is_ignored = git_check_ignore(root, path);
        if let Ok(mut cache) = self.git_ignore_cache.lock() {
            cache.insert(path.to_path_buf(), is_ignored);
        }
        is_ignored
    }
}

/// The ignore-rules file whose edits invalidate the git-check-ignore cache: when
/// its rules change, any path's ignored state can flip.
pub(super) const GITIGNORE_FILE_NAME: &str = ".gitignore";

/// Announces that the ignore rules themselves changed (a `.gitignore` touched,
/// a `git init` flipping the policy). Carries only the affected project's root
/// ([`ProjectRootPayload`]): the frontend re-asks `feed_ignored` about the
/// events it holds, so the answer is never stale.
pub(super) const FEED_IGNORE_EVENT: &str = "feed://ignore-changed";

/// The build- and dependency-output directories a given manifest file implies
/// should be excluded when there is no git to consult. The single authoritative
/// home for the manifest→ignore-directories mapping; each name is a real
/// per-ecosystem generated directory. Matched by exact file name, or by extension
/// for the .NET project/solution manifests. Returns an empty slice for a file that
/// implies nothing.
pub(super) fn manifest_ignore_directories(file_name: &str) -> &'static [&'static str] {
    const NODE: &[&str] = &[
        "node_modules",
        "dist",
        "build",
        "out",
        "coverage",
        ".next",
        ".nuxt",
        ".svelte-kit",
        ".turbo",
        ".cache",
        ".parcel-cache",
    ];
    const RUST: &[&str] = &["target"];
    const PYTHON: &[&str] = &[
        "__pycache__",
        ".venv",
        "venv",
        ".mypy_cache",
        ".pytest_cache",
        ".ruff_cache",
        ".tox",
        "build",
        "dist",
        ".eggs",
    ];
    const GO_DIRECTORIES: &[&str] = &["vendor", "bin"];
    const JVM: &[&str] = &["target", "build", ".gradle"];
    const RUBY: &[&str] = &["vendor", ".bundle"];
    const PHP: &[&str] = &["vendor"];
    const DOTNET: &[&str] = &["bin", "obj"];
    const NONE: &[&str] = &[];

    let extension = Path::new(file_name)
        .extension()
        .and_then(|name| name.to_str());
    if matches!(extension, Some("csproj" | "sln" | "fsproj")) {
        return DOTNET;
    }

    match file_name {
        "package.json" | "pnpm-lock.yaml" | "yarn.lock" | "package-lock.json" => NODE,
        "Cargo.toml" => RUST,
        "pyproject.toml" | "requirements.txt" | "setup.py" | "Pipfile" => PYTHON,
        "go.mod" => GO_DIRECTORIES,
        "pom.xml" | "build.gradle" | "build.gradle.kts" => JVM,
        "Gemfile" => RUBY,
        "composer.json" => PHP,
        _ => NONE,
    }
}

/// The set of directory names to exclude in static mode: the [`ALWAYS_IGNORED_DIRECTORIES`] baseline
/// (so static mode is never weaker than the always-on pre-filter) unioned with the
/// ignore directories implied by every manifest file sitting directly in `root`.
/// One cheap one-level directory scan, run once per armed root.
pub(super) fn static_ignore_directories(root: &Path) -> HashSet<String> {
    let mut directories: HashSet<String> = ALWAYS_IGNORED_DIRECTORIES
        .iter()
        .map(|&name| name.to_string())
        .collect();
    let Ok(entries) = std::fs::read_dir(root) else {
        return directories;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        for &directory in manifest_ignore_directories(name) {
            directories.insert(directory.to_string());
        }
    }
    directories
}

pub(super) fn is_gitignore_file(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(GITIGNORE_FILE_NAME)
}

/// Determine how to exclude ignored files for the watch `root`: git mode when
/// the root is a git work tree (defer to git's own ignore rules), else static
/// mode with a tech-inferred ignore-directory set plus the root `.gitignore`'s
/// rules when one exists. Runs git once (and, in static mode, one directory
/// scan + one file read); re-run whenever the rules could have changed.
pub(super) fn compute_ignore_policy(root: &Path) -> IgnorePolicy {
    if is_git_work_tree(root) {
        return IgnorePolicy::Git {
            root: root.to_path_buf(),
        };
    }
    let rules = std::fs::read_to_string(root.join(GITIGNORE_FILE_NAME))
        .map(|content| crate::workspace::gitignore::Rules::parse(&content))
        .unwrap_or_default();
    IgnorePolicy::Static {
        root: root.to_path_buf(),
        directories: static_ignore_directories(root),
        rules,
    }
}

/// Recompute the ignore policy for the project's watch root and forget every
/// memoized git decision - run when the rules themselves may have changed (a
/// `.gitignore` edited/created/deleted anywhere under the root, or a
/// mid-session `git init` flipping the root from static to git mode). The
/// frontend hears [`FEED_IGNORE_EVENT`] and re-filters the events it already
/// shows for that project against the fresh policy (`feed_ignored`).
pub(super) fn refresh_ignore_policy(app: &AppHandle, watch: &ProjectWatch) {
    watch
        .ignore_rules
        .install(compute_ignore_policy(&watch.root));
    let _ = app.emit_to(
        &watch.label,
        FEED_IGNORE_EVENT,
        ProjectRootPayload {
            root: watch.requested_root.clone(),
        },
    );
}

/// Whether `root` sits inside a git work tree, via `git -C <root> rev-parse
/// --is-inside-work-tree` (which prints `true` in a work tree). A missing git
/// binary, a non-repo folder, or a bare repo all resolve to `false`, routing
/// the watch to the static tech-inference fallback instead. Runs git against
/// `root` (not PADE's process cwd) so it reflects the watched project.
pub(super) fn is_git_work_tree(root: &Path) -> bool {
    let Ok(output) = git_in(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
    else {
        return false;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
}

/// Whether the static policy's `.gitignore` rules ignore `path` - matched on
/// its root-relative, `/`-joined spelling. A path outside the root (or one that
/// isn't valid UTF-8) can't match any rule.
pub(super) fn gitignore_excludes(
    root: &Path,
    rules: &crate::workspace::gitignore::Rules,
    path: &Path,
) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let Some(relative) = relative.to_str() else {
        return false;
    };
    rules.is_ignored(&relative.replace('\\', "/"))
}

/// Shell `git -C <root> check-ignore -q -- <path>` and report whether the path is
/// ignored: exit status 0 means git would ignore it. Because git evaluates the path
/// against every applicable ignore source itself, this correctly honors nested
/// `.gitignore` files, `.git/info/exclude`, `core.excludesFile`, and negations. Any
/// spawn failure resolves to `false` so a fault never hides a real change. Scoped to
/// `root` via `-C`, never PADE's process cwd.
pub(super) fn git_check_ignore(root: &Path, path: &Path) -> bool {
    let Ok(output) = git_in(root)
        .args(["check-ignore", "-q", "--"])
        .arg(path)
        .output()
    else {
        return false;
    };
    output.status.success()
}
