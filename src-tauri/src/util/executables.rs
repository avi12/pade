//! Executable resolution: the search path PADE builds, and finding a command on it.

use super::paths::{directory_key, home_directory};
use super::registry::{expand_environment_variables, registry_string};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Every directory an executable could live in, most authoritative first: this
/// process's `PATH`, then the *live* `PATH` the OS would hand a brand-new
/// process, then the per-user bin directories package managers install into.
///
/// Why more than `PATH`: a process inherits its environment once, at launch - and
/// a GUI app inherits it from the Explorer session that started it, which was
/// itself born at login. An installer's `PATH` edit lands in the registry, not in
/// any running process, so a long-lived ADE would keep missing a CLI the user
/// just installed no matter how often they hit Reload. Reading `PATH` back from
/// its source is what makes Reload actually reload. The extra bin directories
/// then cover the installers that never touch `PATH` at all.
///
/// Built once per detect (it spawns `reg` on Windows) and passed to [`find_in`],
/// rather than rebuilt per lookup.
pub fn search_directories() -> Vec<PathBuf> {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let mut directories: Vec<PathBuf> = std::env::split_paths(&inherited).collect();
    directories.extend(live_path_directories());
    directories.extend(package_manager_directories());

    let mut seen = HashSet::new();
    directories.retain(|directory| {
        !directory.as_os_str().is_empty() && seen.insert(directory_key(directory))
    });
    directories
}

/// The `PATH` a newly-launched process would get, read from where Windows keeps
/// it (the user's and the machine's environment keys) rather than from our own
/// stale copy. Empty elsewhere: on Unix a shell re-reads its profile per session,
/// so there is no equivalent registry of truth to consult.
#[cfg(windows)]
fn live_path_directories() -> Vec<PathBuf> {
    const KEYS: &[&str] = &[
        r"HKCU\Environment",
        r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
    ];
    KEYS.iter()
        .filter_map(|key| registry_string(key, "Path"))
        .flat_map(|value| {
            std::env::split_paths(&expand_environment_variables(&value)).collect::<Vec<PathBuf>>()
        })
        .collect()
}

#[cfg(not(windows))]
fn live_path_directories() -> Vec<PathBuf> {
    Vec::new()
}

/// The bin directories package managers drop CLIs into. Several of them never add
/// themselves to `PATH` (or add it only for future login sessions), so a detect
/// that trusted `PATH` alone would miss a perfectly working install.
fn package_manager_directories() -> Vec<PathBuf> {
    let Some(home) = home_directory() else {
        return Vec::new();
    };
    // cargo, bun, deno, volta, pipx/uv, and hand-rolled `~/.local/bin` installs.
    let mut directories: Vec<PathBuf> = [
        ".cargo/bin",
        ".bun/bin",
        ".deno/bin",
        ".volta/bin",
        ".local/bin",
        ".npm-global/bin",
    ]
    .iter()
    .map(|relative| home.join(relative))
    .collect();

    if cfg!(windows) {
        directories.extend(windows_package_directories(&home));
    } else {
        // Homebrew (Apple silicon, Intel) and the classic system-wide prefix.
        directories.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]);
    }
    directories
}

/// Windows package-manager bin directories: npm's global prefix, pnpm's home,
/// winget's shim folder - plus every winget *portable package* folder, because
/// winget installs those as a directory of raw binaries and only sometimes adds
/// one to `PATH`.
#[cfg(windows)]
fn windows_package_directories(home: &Path) -> Vec<PathBuf> {
    let mut directories = vec![
        home.join("AppData/Roaming/npm"),
        home.join("AppData/Local/pnpm"),
        home.join("AppData/Local/pnpm/bin"),
        home.join("scoop/shims"),
    ];

    let winget = home.join("AppData/Local/Microsoft/WinGet");
    directories.push(winget.join("Links"));
    let packages = winget.join("Packages");
    if let Ok(entries) = std::fs::read_dir(&packages) {
        directories.extend(
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir()),
        );
    }
    directories
}

#[cfg(not(windows))]
fn windows_package_directories(_home: &Path) -> Vec<PathBuf> {
    Vec::new()
}

/// The first of `names` that exists as an executable in `directories`. Name-major on
/// purpose: the caller passes its preferred name first and every directory is
/// tried for it before falling back to an alternate spelling.
pub fn find_in(directories: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    let extensions = executable_extensions();
    names.iter().find_map(|name| {
        directories.iter().find_map(|directory| {
            extensions
                .iter()
                .map(|extension| directory.join(format!("{name}{extension}")))
                .find(|candidate| is_executable(candidate))
        })
    })
}

/// The suffixes to try after a bare command name. Windows resolves a name through
/// `PATHEXT` (so an `.exe`/`.cmd` shim is found the way a shell would find it);
/// everywhere else the name is the filename. The empty suffix comes first so an
/// exact filename always wins over an extension guess.
fn executable_extensions() -> Vec<String> {
    let mut extensions = vec![String::new()];
    if cfg!(windows) {
        let path_extensions =
            std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        extensions.extend(
            path_extensions
                .split(';')
                .map(str::trim)
                .filter(|extension| extension.starts_with('.'))
                .map(str::to_lowercase),
        );
    }
    extensions
}

/// Is this path a file we could actually exec? (On Unix that means the execute
/// bit is set - a readable-but-not-executable file is not a command.)
fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return std::fs::metadata(path).is_ok_and(|metadata| {
            metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
        });
    }
    #[cfg(not(unix))]
    path.is_file()
}

/// The executable `command` names, or `None` if it isn't installed. A command that
/// already carries a path (`C:\…\codex.exe`) is taken as-is; a bare name is looked
/// up across [`search_directories`]. For a one-off lookup - a caller resolving several
/// commands should build [`search_directories`] once and call [`find_in`].
pub fn resolve(command: &str) -> Option<PathBuf> {
    let given = Path::new(command);
    if given.components().count() > 1 {
        return is_executable(given).then(|| given.to_path_buf());
    }
    find_in(&search_directories(), &[command])
}

/// Is `command` runnable on this machine? Sees installs made since ADE launched -
/// see [`search_directories`].
pub fn is_on_path(command: &str) -> bool {
    resolve(command).is_some()
}
