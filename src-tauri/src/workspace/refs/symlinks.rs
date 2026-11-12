//! Symlinks and junctions inside a moved folder that still point at its old location, recreated at the new one.

use super::paths::moved_path;
use std::path::Path;
use std::path::PathBuf;

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
pub(super) fn fix_project_symlinks(new_directory: &Path, old: &str, new: &str) {
    let mut stale = Vec::new();
    collect_stale_links(new_directory, old, new, &mut stale);
    recreate_links(&stale);
}

/// A link whose absolute target lay inside the moved folder, and where it
/// points now.
pub(super) struct StaleLink {
    pub(super) link: PathBuf,
    pub(super) target: String,
    /// A link to a directory is recreated as a directory link (a junction on
    /// Windows), a link to a file as a file symlink.
    pub(super) is_target_directory: bool,
}

/// Recurse `directory`, gathering the stale absolute links and descending only
/// into real sub-directories (never into a link - that bounds the walk and
/// dodges cycles). Skips `.git` (huge, holds no project-path links worth
/// rewriting).
pub(super) fn collect_stale_links(
    directory: &Path,
    old: &str,
    new: &str,
    stale: &mut Vec<StaleLink>,
) {
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
pub(super) fn stale_link(link: PathBuf, old: &str, new: &str) -> Option<StaleLink> {
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
pub(super) fn moved_link_target(target: &str, old: &str, new: &str) -> Option<String> {
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
pub(super) fn recreate_links(stale: &[StaleLink]) {
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
pub(super) fn recreate_links(stale: &[StaleLink]) {
    use std::os::unix::fs::symlink;

    for link in stale {
        let is_removed = remove_stale_link(link, std::fs::remove_file(&link.link));
        if is_removed && symlink(&link.target, &link.link).is_err() {
            report_unlinked(link);
        }
    }
}

/// Whether the stale link is gone, logging why when it isn't.
pub(super) fn remove_stale_link(link: &StaleLink, removed: std::io::Result<()>) -> bool {
    let Err(error) = removed else {
        return true;
    };
    eprintln!(
        "refs: failed to remove stale link {}: {error}",
        link.link.display()
    );
    false
}

pub(super) fn report_unlinked(link: &StaleLink) {
    eprintln!(
        "refs: failed to recreate link {} → {}",
        link.link.display(),
        link.target
    );
}

/// The code page that makes the shell read the script as UTF-8, so a path in
/// any script survives.
#[cfg(windows)]
pub(super) const UTF8_CODE_PAGE_LINE: &str = "@chcp 65001 >nul\r\n";

/// Make every junction from one shell. `mklink /J` is a shell builtin - the
/// only no-admin way to make a junction, as std has no junction constructor -
/// so one shell per link cost a process start each: minutes for a pnpm tree of
/// thousands. A script of them runs at file-system speed.
#[cfg(windows)]
pub(super) fn create_junctions(junctions: &[&StaleLink]) {
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
pub(super) fn junction_script(junctions: &[&StaleLink]) -> String {
    let lines = junctions.iter().map(|junction| {
        let link = junction.link.to_string_lossy().replace('%', "%%");
        let target = junction.target.replace('%', "%%");
        format!("@mklink /J \"{link}\" \"{target}\" >nul\r\n")
    });
    std::iter::once(UTF8_CODE_PAGE_LINE.to_string())
        .chain(lines)
        .collect()
}
