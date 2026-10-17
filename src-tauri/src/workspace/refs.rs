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

use std::path::{Path, PathBuf};

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
fn apply_vscode(_new_directory: &Path, old: &str, new: &str) {
    rewrite_vscode_family(VsCodeApp::Code, old, new);
}

/// Cursor adapter: identical stores to VS Code, under the Cursor base.
fn apply_cursor(_new_directory: &Path, old: &str, new: &str) {
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
            path.extension().and_then(|extension| extension.to_str()),
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

    use crate::util::home_directory;

    use super::{rewrite_file_forms, VsCodeApp};

    /// Rewrite `old` → `new` in every `JetBrains` IDE's `recentProjects.xml`
    /// (`~/AppData/Roaming/JetBrains/*/options/recentProjects.xml`) - one file per
    /// installed IDE version. Both backslash and forward-slash forms. Best-effort.
    pub(super) fn rewrite_jetbrains_recents(old: &str, new: &str) {
        let Some(home) = home_directory() else { return };
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
            home_directory()?
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
    let mut stale = Vec::new();
    collect_stale_links(new_directory, old, new, &mut stale);
    recreate_links(&stale);
}

/// A link whose absolute target lay inside the moved folder, and where it
/// points now.
struct StaleLink {
    link: PathBuf,
    target: String,
    /// A link to a directory is recreated as a directory link (a junction on
    /// Windows), a link to a file as a file symlink.
    is_target_directory: bool,
}

/// Recurse `directory`, gathering the stale absolute links and descending only
/// into real sub-directories (never into a link - that bounds the walk and
/// dodges cycles). Skips `.git` (huge, holds no project-path links worth
/// rewriting).
fn collect_stale_links(directory: &Path, old: &str, new: &str, stale: &mut Vec<StaleLink>) {
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
            stale.extend(stale_link(path, old, new));
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        let is_git_directory = path.file_name().and_then(|name| name.to_str()) == Some(".git");
        if is_git_directory {
            continue;
        }
        collect_stale_links(&path, old, new, stale);
    }
}

/// `link` re-pointed, when its target is an absolute path under `old`. `None`
/// for a relative target or one outside `old`.
fn stale_link(link: PathBuf, old: &str, new: &str) -> Option<StaleLink> {
    let target = std::fs::read_link(&link).ok()?;
    let target = moved_link_target(target.to_str()?, old, new)?;
    // Resolve through the link to learn what it points at.
    let is_target_directory = std::fs::metadata(&link).map_or(true, |metadata| metadata.is_dir());
    Some(StaleLink {
        link,
        target,
        is_target_directory,
    })
}

/// Where an absolute link `target` points once the folder `old` becomes `new`:
/// `None` for a relative target (it survives the move as-is) or one outside
/// `old`. Reads [`moved_path`], so a sibling that merely shares the folder's
/// name prefix (`proj-effort` beside `proj`) is never re-pointed.
fn moved_link_target(target: &str, old: &str, new: &str) -> Option<String> {
    if !Path::new(target).is_absolute() {
        return None;
    }
    moved_path(target, old, new)
}

/// Remove each stale link and recreate it pointing at its new target. On
/// Windows a directory link is a junction (`mklink /J`, no admin needed,
/// matching pnpm); a file link uses `symlink_file`. Best-effort: any failure is
/// logged and swallowed.
#[cfg(windows)]
fn recreate_links(stale: &[StaleLink]) {
    use std::os::windows::fs::symlink_file;

    let mut junctions = Vec::new();
    for link in stale {
        // Removing a junction is a directory op (`remove_dir`); a file symlink
        // is a file op.
        if link.is_target_directory {
            if remove_stale_link(link, std::fs::remove_dir(&link.link)) {
                junctions.push(link);
            }
            continue;
        }
        let is_removed = remove_stale_link(link, std::fs::remove_file(&link.link));
        if is_removed && symlink_file(&link.target, &link.link).is_err() {
            report_unlinked(link);
        }
    }
    create_junctions(&junctions);
}

#[cfg(not(windows))]
fn recreate_links(stale: &[StaleLink]) {
    use std::os::unix::fs::symlink;

    for link in stale {
        let is_removed = remove_stale_link(link, std::fs::remove_file(&link.link));
        if is_removed && symlink(&link.target, &link.link).is_err() {
            report_unlinked(link);
        }
    }
}

/// Whether the stale link is gone, logging why when it isn't.
fn remove_stale_link(link: &StaleLink, removed: std::io::Result<()>) -> bool {
    let Err(error) = removed else {
        return true;
    };
    eprintln!(
        "refs: failed to remove stale link {}: {error}",
        link.link.display()
    );
    false
}

fn report_unlinked(link: &StaleLink) {
    eprintln!(
        "refs: failed to recreate link {} → {}",
        link.link.display(),
        link.target
    );
}

/// The code page that makes the shell read the script as UTF-8, so a path in
/// any script survives.
#[cfg(windows)]
const UTF8_CODE_PAGE_LINE: &str = "@chcp 65001 >nul\r\n";

/// Make every junction from one shell. `mklink /J` is a shell builtin - the
/// only no-admin way to make a junction, as std has no junction constructor -
/// so one shell per link cost a process start each: minutes for a pnpm tree of
/// thousands. A script of them runs at file-system speed.
#[cfg(windows)]
fn create_junctions(junctions: &[&StaleLink]) {
    if junctions.is_empty() {
        return;
    }
    let script_path = std::env::temp_dir().join(format!("pade-relink-{}.cmd", std::process::id()));
    if let Err(error) = std::fs::write(&script_path, junction_script(junctions)) {
        eprintln!("refs: failed to write {}: {error}", script_path.display());
        return;
    }
    let is_created = crate::util::command(crate::util::COMMAND_SHELL)
        .arg(crate::util::COMMAND_SHELL_RUN_FLAG)
        .arg(&script_path)
        .output()
        .is_ok_and(|output| output.status.success());
    if !is_created {
        eprintln!(
            "refs: some of {} junctions were not recreated",
            junctions.len()
        );
    }
    if let Err(error) = std::fs::remove_file(&script_path) {
        eprintln!("refs: failed to remove {}: {error}", script_path.display());
    }
}

/// One `mklink /J` line per junction. A batch script expands `%`, so each one
/// in a path is doubled; a Windows path can't hold a quote, so quoting each
/// path covers the rest (`&`, `^`, spaces).
#[cfg(windows)]
fn junction_script(junctions: &[&StaleLink]) -> String {
    let lines = junctions.iter().map(|junction| {
        let link = junction.link.to_string_lossy().replace('%', "%%");
        let target = junction.target.replace('%', "%%");
        format!("@mklink /J \"{link}\" \"{target}\" >nul\r\n")
    });
    std::iter::once(UTF8_CODE_PAGE_LINE.to_string())
        .chain(lines)
        .collect()
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

/// Spawn `command` through the platform shell in `directory` (see
/// [`crate::util::shell_command`]), detached. Stdio is nulled so the background
/// install doesn't tie its pipes to us; the handle is dropped so we neither wait
/// nor kill it.
fn spawn_install(directory: &Path, command: &str) {
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

#[cfg(test)]
mod tests {
    use super::{moved_link_target, rebase_paths_in, vscode_uri};
    use std::path::Path;

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

    /// An absolute path on this platform: `C:\…` on Windows, `/…` elsewhere.
    fn absolute(relative: &str) -> String {
        if cfg!(windows) {
            format!(r"C:\{}", relative.replace('/', r"\"))
        } else {
            format!("/{relative}")
        }
    }

    fn moved(target: &str) -> Option<String> {
        moved_link_target(target, &absolute("old/proj"), &absolute("new/proj"))
    }

    #[test]
    fn a_relative_link_target_is_left_alone() {
        assert_eq!(moved("relative/path"), None);
    }

    #[test]
    fn a_link_target_under_the_old_tree_swings_to_the_new_one() {
        assert_eq!(
            moved(&absolute("old/proj/node_modules/pkg")),
            Some(absolute("new/proj/node_modules/pkg"))
        );
    }

    #[test]
    fn a_link_target_outside_the_old_tree_is_left_alone() {
        assert_eq!(moved(&absolute("elsewhere/x")), None);
    }

    #[test]
    fn a_link_into_a_sibling_sharing_the_name_prefix_is_left_alone() {
        assert_eq!(moved(&absolute("old/proj-effort/x")), None);
    }

    #[test]
    fn a_link_target_matches_the_old_folder_ascii_case_insensitively() {
        assert_eq!(moved(&absolute("OLD/Proj/x")), Some(absolute("new/proj/x")));
    }

    /// The Kelvin sign lowercases to an ASCII `k` but is three bytes long, so a
    /// Unicode-lowercased prefix match used to slice the target mid-character.
    #[test]
    fn a_non_ascii_lookalike_never_matches_or_panics() {
        assert_eq!(
            moved_link_target(&absolute("\u{212A}/x"), &absolute("k"), &absolute("new")),
            None
        );
    }

    /// A pnpm tree's junctions all point at absolute paths inside the project;
    /// after the move each one is re-pointed under the new folder.
    #[cfg(windows)]
    #[test]
    fn a_junction_into_the_moved_folder_is_re_pointed() {
        let scratch = std::env::temp_dir().join(format!("pade-relink-{}", std::process::id()));
        let old = scratch.join("old");
        let new = scratch.join("new");
        std::fs::create_dir_all(new.join("store").join("package")).expect("store folder");
        let link = new.join("package");
        let is_created = crate::util::command(crate::util::COMMAND_SHELL)
            .args([crate::util::COMMAND_SHELL_RUN_FLAG, "mklink", "/J"])
            .arg(&link)
            .arg(old.join("store").join("package"))
            .output()
            .is_ok_and(|output| output.status.success());
        if is_created {
            super::fix_project_symlinks(&new, &old.to_string_lossy(), &new.to_string_lossy());
            let target = std::fs::read_link(&link).expect("re-pointed junction");
            assert!(
                target.ends_with(Path::new("new").join("store").join("package")),
                "{}",
                target.display()
            );
        }
        std::fs::remove_dir(&link).ok();
        std::fs::remove_dir_all(&scratch).ok();
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_script_doubles_percent_signs_and_quotes_each_path() {
        let junction = super::StaleLink {
            link: std::path::PathBuf::from(r"C:\a & b\100%"),
            target: r"C:\c\100%".to_string(),
            is_target_directory: true,
        };
        let script = super::junction_script(&[&junction]);
        assert!(
            script.ends_with("@mklink /J \"C:\\a & b\\100%%\" \"C:\\c\\100%%\" >nul\r\n"),
            "{script}"
        );
    }
}
