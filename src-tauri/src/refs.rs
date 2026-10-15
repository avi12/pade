//! Move-directory reference updater.
//!
//! When PADE moves a workspace folder (`old` path → `new` path), external tools
//! that remember the old absolute path - each agent's memory of the project
//! (see `agents`), the `JetBrains` / VS Code / Cursor "recent projects" lists -
//! would silently lose the project. This module re-points those references so
//! the linked tools keep working.
//!
//! It is strictly **best-effort**: every adapter swallows its own errors (a
//! missing, absent, or locked file must never fail the move). We only ever touch
//! data the tool already persisted locally - no network, no CLI.
//!
//! Gating: the agent adapters touch only entries for the moved folder or
//! something inside it. Each IDE adapter runs only when the moved directory
//! actually carries that IDE's marker dir (e.g. `<new>/.idea`, `<new>/.vscode`),
//! so we never rewrite recents for an IDE the project was never opened in. The
//! IDE adapters are additionally Windows-only (that's where PADE ships);
//! off-Windows they are no-ops so the crate still compiles everywhere.

mod agents;

use std::path::Path;

use crate::util::is_on_path;

/// Best-effort: re-point every external reference from `old` to `new` (absolute
/// dir paths). Per-adapter errors are logged and ignored - one failing tool must
/// never abort the others or the move itself.
pub fn update_references(old: &str, new: &str) {
    let new_directory = Path::new(new);
    agents::relocate_memories(old, new);
    rewrite_ide_recents(new_directory, old, new);
    fix_project_symlinks(new_directory, old, new);
    reconcile_package_manager(new_directory);
}

// ---------------------------------------------------------------------------
// Shared string helpers.
// ---------------------------------------------------------------------------

/// VS Code (and forks) store recent folders as file URIs, e.g.
/// `C:\repositories\avi\foo` → `file:///c%3A/repositories/avi/foo`:
/// lowercase drive letter, `:` percent-encoded to `%3A`, `\` → `/`.
fn vscode_uri(path: &str) -> String {
    let mut output = String::from("file:///");
    for character in path.chars() {
        match character {
            ':' => output.push_str("%3A"),
            '\\' => output.push('/'),
            // Lowercase the drive letter (URIs use `file:///c%3A/...`). Only the
            // ASCII head is a drive letter; lowercasing later chars is harmless
            // for the path segments VS Code emits here.
            _ => output.extend(character.to_lowercase()),
        }
    }
    output
}

/// The characters that may end a stored path to the moved folder: a separator
/// (something inside it) or the quote, line end or text end that closes it. Any
/// other character means a longer name - `pade-effort` is not inside `pade`.
const PATH_END_CHARACTERS: [char; 6] = ['\\', '/', '"', '\'', '\n', '\r'];

/// Re-point every stored form of the folder `old` (and of anything inside it) to
/// `new`: the raw backslash path, its JSON-escaped form, the forward-slash path
/// and the VS Code file-URI form. Matched case-insensitively, as Windows paths
/// are; a match stored in lower case (Codex keys its projects that way) is
/// written back in lower case. Shared by every adapter, so each tool's own
/// storage format needs no substitution logic of its own.
fn rebase_paths_in(haystack: &str, old: &str, new: &str) -> String {
    let old = old.trim_end_matches(['\\', '/']);
    let new = new.trim_end_matches(['\\', '/']);
    let forms = [
        (old.replace('\\', r"\\"), new.replace('\\', r"\\")),
        (old.to_string(), new.to_string()),
        (old.replace('\\', "/"), new.replace('\\', "/")),
        (vscode_uri(old), vscode_uri(new)),
    ];
    let mut rebased = haystack.to_string();
    for (old_form, new_form) in forms {
        rebased = replace_folder(&rebased, &old_form, &new_form);
    }
    rebased
}

/// Where `path` lands when the folder `old` becomes `new`: `None` unless it is
/// that folder or inside it.
pub fn moved_path(path: &str, old: &str, new: &str) -> Option<String> {
    let moved = rebase_paths_in(path, old, new);
    (moved != path).then_some(moved)
}

/// `haystack` with each folder-bounded, case-insensitive `old` swapped for `new`.
fn replace_folder(haystack: &str, old: &str, new: &str) -> String {
    let lower_haystack = haystack.to_ascii_lowercase();
    let lower_old = old.to_ascii_lowercase();
    let mut output = String::with_capacity(haystack.len());
    let mut copied_up_to = 0;
    for (start, _) in lower_haystack.match_indices(&lower_old) {
        if start < copied_up_to {
            continue;
        }
        let end = start + old.len();
        let ends_the_folder = haystack[end..]
            .chars()
            .next()
            .is_none_or(|next| PATH_END_CHARACTERS.contains(&next));
        if !ends_the_folder {
            continue;
        }
        let matched = &haystack[start..end];
        let is_stored_lower_case = matched == lower_old && matched != old;
        output.push_str(&haystack[copied_up_to..start]);
        if is_stored_lower_case {
            output.push_str(&new.to_ascii_lowercase());
        } else {
            output.push_str(new);
        }
        copied_up_to = end;
    }
    output.push_str(&haystack[copied_up_to..]);
    output
}

/// Read `path`, apply `rebase_paths_in`, and write back only if the content
/// actually changed. Best-effort: returns quietly on any read/write error.
/// Shared by the plain-text adapters (`.idea`, IDE recents, agent configs).
fn rewrite_file_forms(path: &Path, old: &str, new: &str) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let updated = rebase_paths_in(&content, old, new);
    if updated == content {
        return;
    }
    if let Err(error) = std::fs::write(path, updated) {
        eprintln!("refs: failed to write {}: {error}", path.display());
    }
}

// ---------------------------------------------------------------------------
// IDE adapter table - marker-gated, easy to extend.
// ---------------------------------------------------------------------------

/// How to re-point one IDE's stored references after a move.
///
/// `marker` is the project-local dir that proves the IDE was used here (checked
/// under the NEW path); the adapter only runs when it exists. `apply` performs
/// the IDE-specific rewrites. Adding an IDE is one more entry in `IDE_ADAPTERS`.
struct IdeAdapter {
    /// Project-local marker dir gating this adapter (e.g. `.idea`, `.vscode`).
    marker: &'static str,
    /// Re-point this IDE's references. `new_directory` is the moved project (for
    /// project-local files like `.idea`); `old`/`new` are the absolute paths.
    apply: fn(new_directory: &Path, old: &str, new: &str),
}

/// The IDEs we know how to re-point. Extend this table to support more.
const IDE_ADAPTERS: &[IdeAdapter] = &[
    // JetBrains: project-local `.idea/*.xml|*.iml` (safety net) + global recents.
    IdeAdapter {
        marker: ".idea",
        apply: apply_jetbrains,
    },
    // VS Code: recents under ~/AppData/Roaming/Code/User/globalStorage.
    IdeAdapter {
        marker: ".vscode",
        apply: apply_vscode,
    },
    // Cursor (a VS Code fork): same on-disk recents format, under
    // ~/AppData/Roaming/Cursor/... . Opening a folder in Cursor writes `.vscode`;
    // some setups also carry `.cursor`. Gate on either via two entries.
    IdeAdapter {
        marker: ".vscode",
        apply: apply_cursor,
    },
    IdeAdapter {
        marker: ".cursor",
        apply: apply_cursor,
    },
    // TODO: Zed - recents in ~/AppData/Roaming/Zed (SQLite `db.sqlite`); marker
    // `.zed`. Left as a stub until its schema is pinned down.
    // TODO: Sublime Text - session in
    // ~/AppData/Roaming/Sublime Text/Local/Session.sublime_session; marker
    // `.sublime-project`. Left as a stub.
];

/// Run every IDE adapter whose marker dir is present in the moved project.
fn rewrite_ide_recents(new_directory: &Path, old: &str, new: &str) {
    for adapter in IDE_ADAPTERS {
        if new_directory.join(adapter.marker).is_dir() {
            (adapter.apply)(new_directory, old, new);
        }
    }
}

/// `JetBrains` adapter: project-local `.idea` files, then the global recents.
fn apply_jetbrains(new_directory: &Path, old: &str, new: &str) {
    rewrite_idea(new_directory, old, new);
    rewrite_jetbrains_recents(old, new);
}

/// VS Code adapter: rewrite recents under the VS Code `globalStorage` base.
fn apply_vscode(_new_dir: &Path, old: &str, new: &str) {
    rewrite_vscode_family(VsCodeApp::Code, old, new);
}

/// Cursor adapter: identical stores to VS Code, under the Cursor base.
fn apply_cursor(_new_dir: &Path, old: &str, new: &str) {
    rewrite_vscode_family(VsCodeApp::Cursor, old, new);
}

/// A VS Code-family app, identifying its `~/AppData/Roaming/<dir>` base. The
/// on-disk `globalStorage` layout (`storage.json` + `state.vscdb`) is identical
/// across the family, so one code path handles them all (DRY).
#[derive(Clone, Copy)]
enum VsCodeApp {
    Code,
    Cursor,
}

impl VsCodeApp {
    /// The app's directory name under `~/AppData/Roaming`.
    fn roaming_directory(self) -> &'static str {
        match self {
            VsCodeApp::Code => "Code",
            VsCodeApp::Cursor => "Cursor",
        }
    }
}

// ---------------------------------------------------------------------------
// JetBrains `.idea` project files (cross-platform).
// ---------------------------------------------------------------------------

/// Rewrite absolute occurrences of `old` → `new` in every `*.xml` / `*.iml`
/// under `<new_directory>/.idea/`, in both backslash and forward-slash forms.
/// `.idea` is mostly `$PROJECT_DIR$`-relative, so this is a light safety net for
/// the occasional stored absolute path. Best-effort per file.
fn rewrite_idea(new_directory: &Path, old: &str, new: &str) {
    let idea = new_directory.join(".idea");
    let Ok(entries) = std::fs::read_dir(&idea) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_idea_file = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("xml" | "iml")
        );
        if is_idea_file {
            rewrite_file_forms(&path, old, new);
        }
    }
}

// ---------------------------------------------------------------------------
// Windows-only IDE recents (real bodies below, no-ops off-Windows).
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod windows {
    use std::path::PathBuf;

    use crate::util::home_dir;

    use super::{rewrite_file_forms, VsCodeApp};

    /// Rewrite `old` → `new` in every `JetBrains` IDE's `recentProjects.xml`
    /// (`~/AppData/Roaming/JetBrains/*/options/recentProjects.xml`) - one file per
    /// installed IDE version. Both backslash and forward-slash forms. Best-effort.
    pub(super) fn rewrite_jetbrains_recents(old: &str, new: &str) {
        let Some(home) = home_dir() else { return };
        let jetbrains = home.join("AppData").join("Roaming").join("JetBrains");
        let Ok(entries) = std::fs::read_dir(&jetbrains) else {
            return;
        };
        for entry in entries.flatten() {
            let recents = entry.path().join("options").join("recentProjects.xml");
            if recents.is_file() {
                rewrite_file_forms(&recents, old, new);
            }
        }
    }

    /// The VS Code-family app's `User/globalStorage` dir, if home is known.
    fn global_storage(app: VsCodeApp) -> Option<PathBuf> {
        Some(
            home_dir()?
                .join("AppData")
                .join("Roaming")
                .join(app.roaming_directory())
                .join("User")
                .join("globalStorage"),
        )
    }

    /// Rewrite `old` → `new` across one VS Code-family app's recents: the
    /// plain-text `storage.json` and the `SQLite` `state.vscdb`. Best-effort.
    pub(super) fn rewrite_vscode_family(app: VsCodeApp, old: &str, new: &str) {
        let Some(base) = global_storage(app) else {
            return;
        };
        rewrite_vscode_storage(&base, old, new);
        rewrite_vscode_database(&base, old, new);
    }

    /// Rewrite `old` → `new` in `<globalStorage>/storage.json` (which caches,
    /// among other things, recently-opened paths) in backslash, forward-slash,
    /// and file-URI forms. Best-effort.
    fn rewrite_vscode_storage(base: &std::path::Path, old: &str, new: &str) {
        let storage = base.join("storage.json");
        if storage.is_file() {
            rewrite_file_forms(&storage, old, new);
        }
    }

    /// Rewrite `old` → `new` in `<globalStorage>/state.vscdb`'s recents list.
    ///
    /// `state.vscdb` is a `SQLite` database; the recents live in table `ItemTable`,
    /// key `history.recentlyOpenedPathsList`, whose value is a JSON string of file
    /// URIs. We open the DB read-write, replace all path forms in that JSON, and
    /// write the row back. Best-effort: if the DB is missing or locked (the app is
    /// open), any step errors and we simply return.
    fn rewrite_vscode_database(base: &std::path::Path, old: &str, new: &str) {
        use rusqlite::Connection;

        const RECENTS_KEY: &str = "history.recentlyOpenedPathsList";

        let database = base.join("state.vscdb");
        if !database.is_file() {
            return;
        }

        let Ok(connection) = Connection::open(&database).inspect_err(|error| {
            eprintln!("refs: failed to open {}: {error}", database.display());
        }) else {
            return;
        };

        // No such row / locked / other - nothing safe to do.
        let Ok(value) = connection
            .query_row(
                "SELECT value FROM ItemTable WHERE key = ?1",
                [RECENTS_KEY],
                |row| row.get::<_, String>(0),
            )
            .inspect_err(|error| {
                eprintln!(
                    "refs: could not read recents from {}: {error}",
                    database.display()
                );
            })
        else {
            return;
        };

        let Some(updated) = super::moved_path(&value, old, new) else {
            return;
        };

        if let Err(error) = connection.execute(
            "UPDATE ItemTable SET value = ?1 WHERE key = ?2",
            rusqlite::params![updated, RECENTS_KEY],
        ) {
            eprintln!(
                "refs: failed to update recents in {}: {error}",
                database.display()
            );
        }
    }
}

#[cfg(windows)]
use windows::{rewrite_jetbrains_recents, rewrite_vscode_family};

// Off-Windows: no JetBrains/VS Code layout to touch. No-op stubs keep the IDE
// adapters cross-platform and the crate compiling everywhere.
#[cfg(not(windows))]
fn rewrite_jetbrains_recents(_old: &str, _new: &str) {}
#[cfg(not(windows))]
fn rewrite_vscode_family(_app: VsCodeApp, _old: &str, _new: &str) {}

// ---------------------------------------------------------------------------
// Adapter - project-tree symlinks/junctions (cross-platform).
// ---------------------------------------------------------------------------

/// Re-point every link in the moved project whose ABSOLUTE target lived under
/// the old tree. Many toolchains plant absolute symlinks/junctions inside a
/// project - pnpm's `node_modules`, Python's `.venv`, and others - and a link
/// with an absolute target under `old` dangles after the move, so we swing it to
/// the matching path under `new`.
///
/// Relative links are left untouched - they survive a move within one drive
/// (the common case), so usually nothing here changes. This handles the
/// absolute-target edge cases: workspace/monorepo links, cross-tree links, venv
/// interpreter links, and globally-linked packages, uniformly across techs.
///
/// Best-effort throughout: a giant tree must never fail or hang the move, so
/// every fallible step is swallowed and we bound the walk by never following the
/// links we inspect (which also avoids link cycles).
fn fix_project_symlinks(new_directory: &Path, old: &str, new: &str) {
    relink_directory(new_directory, old, new);
}

/// Recurse `directory`, re-pointing stale absolute links and descending only
/// into real sub-directories (never into a link - that bounds the walk and
/// dodges cycles). Skips `.git` (huge, holds no project-path links worth
/// rewriting).
fn relink_directory(directory: &Path, old: &str, new: &str) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `symlink_metadata` describes the entry itself, not its target - so a
        // link reports as a link rather than as whatever it points at.
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            remap_link(&path, old, new);
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        let is_git_directory = path.file_name().and_then(|name| name.to_str()) == Some(".git");
        if is_git_directory {
            continue;
        }
        relink_directory(&path, old, new);
    }
}

/// If `link`'s target is an absolute path under `old`, recreate it pointing at
/// the same suffix under `new`. No-op for relative targets or targets outside
/// `old`. `is_target_dir` decides junction-vs-file-symlink on recreation.
fn remap_link(link: &Path, old: &str, new: &str) {
    let Ok(target) = std::fs::read_link(link) else {
        return;
    };
    let Some(target_text) = target.to_str() else {
        return;
    };
    let Some(remapped) = remap_under(target_text, old, new) else {
        return; // relative, or not under `old` - leave it alone
    };
    // A link to a directory must be recreated as a dir link (junction on
    // Windows); resolve through the link to learn what it points at.
    let is_target_directory = std::fs::metadata(link).map_or(true, |metadata| metadata.is_dir());
    recreate_link(link, &remapped, is_target_directory);
}

/// Remap an absolute `target` that sits under `old` to the same suffix under
/// `new`; `None` if `target` is relative or not under `old`. Case-insensitive
/// prefix match on Windows (its filesystem is), case-sensitive elsewhere.
fn remap_under(target: &str, old: &str, new: &str) -> Option<String> {
    if !Path::new(target).is_absolute() {
        return None;
    }
    let starts_under_old = if cfg!(windows) {
        target.to_lowercase().starts_with(&old.to_lowercase())
    } else {
        target.starts_with(old)
    };
    if !starts_under_old {
        return None;
    }
    // Preserve the original suffix bytes (only the `old` prefix is swapped).
    let suffix = &target[old.len()..];
    Some(format!("{new}{suffix}"))
}

/// Remove the stale link at `link` and recreate it pointing at `target`. On
/// Windows a directory link is a junction (`mklink /J`, no admin needed, matching
/// pnpm); a file link uses `symlink_file`. On other platforms a single `symlink`
/// covers both. Best-effort: any failure is logged and swallowed.
#[cfg(windows)]
fn recreate_link(link: &Path, target: &str, is_target_directory: bool) {
    use std::os::windows::fs::symlink_file;

    // Removing a junction is a directory op (`remove_dir`); a file symlink is a
    // file op. Pick by what we're about to recreate.
    let removed = if is_target_directory {
        std::fs::remove_dir(link)
    } else {
        std::fs::remove_file(link)
    };
    if let Err(error) = removed {
        eprintln!(
            "refs: failed to remove stale link {}: {error}",
            link.display()
        );
        return;
    }

    let result = if is_target_directory {
        // pnpm uses junctions for dir links (no admin). `mklink /J` is the only
        // no-admin way to make one; the std lib has no junction constructor.
        crate::util::command("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .is_ok_and(|output| output.status.success())
    } else {
        symlink_file(target, link).is_ok()
    };
    if !result {
        eprintln!(
            "refs: failed to recreate link {} → {target}",
            link.display()
        );
    }
}

#[cfg(not(windows))]
fn recreate_link(link: &Path, target: &str, _is_target_directory: bool) {
    use std::os::unix::fs::symlink;

    if let Err(error) = std::fs::remove_file(link) {
        eprintln!(
            "refs: failed to remove stale link {}: {error}",
            link.display()
        );
        return;
    }
    if let Err(error) = symlink(target, link) {
        eprintln!(
            "refs: failed to recreate link {} → {target}: {error}",
            link.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Adapter - package-manager reconcile (cross-platform).
// ---------------------------------------------------------------------------

/// A package manager we can re-materialize after a move: if `lockfile` is present
/// in the moved project and the manager's `tool` is on PATH, run `command` there
/// to regenerate its (absolute-link-laden) install tree from scratch. This is the
/// robust "easy way" that complements `fix_project_symlinks` - the fixer re-points
/// links in place; this rebuilds them wholesale where a manager exists.
struct PackageManager {
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
const PACKAGE_MANAGERS: &[PackageManager] = &[
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
fn reconcile_package_manager(new_directory: &Path) {
    // One package manager per project - run only the first match.
    let project_manager = PACKAGE_MANAGERS
        .iter()
        .find(|manager| new_directory.join(manager.lockfile).is_file() && is_on_path(manager.tool));
    if let Some(manager) = project_manager {
        spawn_install(new_directory, manager.command);
    }
}

/// Spawn `command` through the platform shell in `directory`, detached: on Windows
/// `cmd /C <command>` so the `.cmd` shim resolves (mirroring `runner`/`ide`),
/// else the command directly. Stdio is nulled so the background install doesn't
/// tie its pipes to us; the handle is dropped so we neither wait nor kill it.
fn spawn_install(directory: &Path, command: &str) {
    use std::process::Stdio;

    let mut process = if cfg!(windows) {
        let mut command_process = crate::util::command("cmd");
        command_process.args(["/C", command]);
        command_process
    } else {
        let mut command_process = crate::util::command("sh");
        command_process.args(["-c", command]);
        command_process
    };
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

#[cfg(test)]
mod tests {
    use super::{rebase_paths_in, remap_under, vscode_uri};

    #[test]
    fn vscode_uri_encodes_a_windows_path() {
        assert_eq!(
            vscode_uri(r"C:\repositories\avi\foo"),
            "file:///c%3A/repositories/avi/foo"
        );
    }

    #[test]
    fn rebasing_covers_raw_escaped_forward_slash_and_uri_forms() {
        let haystack = "raw C:\\old\\proj\n json \"C:\\\\old\\\\proj\\\\src\"\n fwd 'C:/old/proj'\n uri file:///c%3A/old/proj";
        assert_eq!(
            rebase_paths_in(haystack, r"C:\old\proj", r"C:\new\proj"),
            "raw C:\\new\\proj\n json \"C:\\\\new\\\\proj\\\\src\"\n fwd 'C:/new/proj'\n uri file:///c%3A/new/proj"
        );
    }

    #[test]
    fn rebasing_leaves_a_sibling_whose_name_merely_starts_the_same() {
        let haystack = r#"["C:/old/proj-effort", "C:/old/proj"]"#;
        assert_eq!(
            rebase_paths_in(haystack, r"C:\old\proj", r"C:\new\Proj"),
            r#"["C:/old/proj-effort", "C:/new/Proj"]"#
        );
    }

    #[test]
    fn a_path_stored_in_lower_case_stays_lower_case() {
        let haystack = r"[projects.'c:\repositories\old']";
        assert_eq!(
            rebase_paths_in(haystack, r"C:\repositories\old", r"C:\repositories\New"),
            r"[projects.'c:\repositories\new']"
        );
    }

    #[test]
    fn remap_under_ignores_relative_targets() {
        assert_eq!(remap_under("relative/path", "/old", "/new"), None);
    }

    #[cfg(windows)]
    #[test]
    fn remap_under_swings_a_target_under_the_old_tree() {
        assert_eq!(
            remap_under(
                r"C:\old\proj\node_modules\pkg",
                r"C:\old\proj",
                r"C:\new\proj"
            )
            .as_deref(),
            Some(r"C:\new\proj\node_modules\pkg")
        );
    }

    #[cfg(windows)]
    #[test]
    fn remap_under_matches_the_old_prefix_case_insensitively() {
        assert_eq!(
            remap_under(r"c:\OLD\proj\x", r"C:\old\proj", r"C:\new\proj").as_deref(),
            Some(r"C:\new\proj\x")
        );
    }

    #[cfg(windows)]
    #[test]
    fn remap_under_leaves_targets_outside_the_old_tree() {
        assert_eq!(
            remap_under(r"C:\elsewhere\x", r"C:\old\proj", r"C:\new\proj"),
            None
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn remap_under_swings_a_target_under_the_old_tree() {
        assert_eq!(
            remap_under("/old/proj/node_modules/pkg", "/old/proj", "/new/proj").as_deref(),
            Some("/new/proj/node_modules/pkg")
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn remap_under_leaves_targets_outside_the_old_tree() {
        assert_eq!(remap_under("/elsewhere/x", "/old/proj", "/new/proj"), None);
    }
}
