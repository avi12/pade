//! Small cross-cutting helpers shared by multiple modules (DRY).

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Whether a caller may use an id in a process-global resource registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OwnerAccess {
    Vacant,
    Owned,
    Foreign,
}

/// Decide access without exposing one window's resource to another. Window
/// labels survive a `WebView` reload, so `Owned` also permits reload re-adoption.
pub(crate) fn owner_access(owner: Option<&str>, requester: &str) -> OwnerAccess {
    match owner {
        None => OwnerAccess::Vacant,
        Some(owner) if owner == requester => OwnerAccess::Owned,
        Some(_) => OwnerAccess::Foreign,
    }
}

/// A `Command` that never flashes a console window on Windows. Every background
/// or captured-output spawn — PATH lookups, git, curl, the agent namer, task
/// runners, registry edits — goes through this so a GUI app stays windowless
/// instead of popping a `conhost` window per spawn (e.g. on the 5s agent
/// re-detect). Interactive terminals the user explicitly opens (`os.rs`) are
/// spawned directly so they *do* get a window.
///
/// stdin is closed by default: none of these children are interactive, and a
/// CLI that decides to wait on stdin (the opencode-db hang class) must see EOF
/// immediately rather than block forever. The rare caller that feeds a child on
/// stdin (the usage curl `--config`) overrides this with `Stdio::piped()`.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW — the child gets no console window.
        command.creation_flags(0x0800_0000);
    }
    command
}

/// How often [`succeeds_within`] re-checks a child it is waiting on. Matches the
/// runner's exit-poll cadence: prompt enough for an interactive check, cheap
/// enough to leave the child undisturbed.
const CHILD_WAIT_POLL: Duration = Duration::from_millis(50);

/// Run `command` to completion under a hard deadline, reporting only whether it
/// exited successfully. All three standard streams are closed (the caller wants
/// a verdict, not output — and an open stdin is exactly what lets a child wait
/// forever). On deadline the child is killed and the answer is `false`: a probe
/// that can hang is never worth a hang.
pub fn succeeds_within(mut command: Command, timeout: Duration) -> bool {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(CHILD_WAIT_POLL),
            // Deadline reached or the wait itself failed: reap and report no.
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// Spawn `program args` as a process that fully **outlives** ADE — used only for
/// launching a GUI IDE (see `ide::spawn_launcher`), which the user expects to
/// keep running after they close ADE.
///
/// Why the plain [`command`] spawn is not enough on Windows: a child inherits
/// its parent's console *and* its parent's Job object, and job membership is
/// inherited transitively. A GUI app like ADE is commonly placed in a Job whose
/// limit flags include `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so when ADE exits
/// and its job handle closes, every process still in that job — including an IDE
/// we merely started — is terminated. That is exactly requirement #1's failure.
/// Three creation flags break the child out of both:
///
/// - `DETACHED_PROCESS` (`0x0000_0008`): the child gets no console of its own. A
///   GUI IDE opens its own window and needs none, so this is the right console
///   flag here — and it *replaces* [`command`]'s `CREATE_NO_WINDOW`, with which
///   it is mutually exclusive (both are console-disposition flags). No console
///   means no `conhost` flash, preserving [`command`]'s windowless property.
/// - `CREATE_NEW_PROCESS_GROUP` (`0x0000_0200`): the child leads its own process
///   group, so a Ctrl-C / console signal aimed at ADE never reaches it.
/// - `CREATE_BREAKAWAY_FROM_JOB` (`0x0100_0000`): the child is created *outside*
///   ADE's job, so closing ADE's job on exit cannot kill it.
///
/// Robustness: `CREATE_BREAKAWAY_FROM_JOB` makes `CreateProcess` fail with
/// `ERROR_ACCESS_DENIED` (os error 5) when ADE's job forbids breakaway (its
/// limits lack `JOB_OBJECT_LIMIT_BREAKAWAY_OK`) or ADE is in no job at all. On
/// that one error we retry without breakaway — still detached and in its own
/// group, which is the best we can do inside a no-breakaway job. Any other error
/// is returned unchanged, never swallowed.
///
/// On non-Windows there is no job/console teardown to escape: a spawned child is
/// already independent and is re-parented to init when ADE exits, so a plain
/// spawn suffices.
#[cfg(windows)]
pub fn spawn_detached(program: impl AsRef<OsStr>, args: &[String]) -> std::io::Result<Child> {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    const ERROR_ACCESS_DENIED: i32 = 5;

    let detached = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    let spawn = |flags: u32| {
        Command::new(&program)
            .args(args)
            .creation_flags(flags)
            .spawn()
    };
    match spawn(detached | CREATE_BREAKAWAY_FROM_JOB) {
        Ok(child) => Ok(child),
        Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED) => spawn(detached),
        Err(error) => Err(error),
    }
}

#[cfg(not(windows))]
pub fn spawn_detached(program: impl AsRef<OsStr>, args: &[String]) -> std::io::Result<Child> {
    Command::new(program).args(args).spawn()
}

/// Every directory an executable could live in, most authoritative first: this
/// process's `PATH`, then the *live* `PATH` the OS would hand a brand-new
/// process, then the per-user bin directories package managers install into.
///
/// Why more than `PATH`: a process inherits its environment once, at launch — and
/// a GUI app inherits it from the Explorer session that started it, which was
/// itself born at login. An installer's `PATH` edit lands in the registry, not in
/// any running process, so a long-lived ADE would keep missing a CLI the user
/// just installed no matter how often they hit Reload. Reading `PATH` back from
/// its source is what makes Reload actually reload. The extra bin directories
/// then cover the installers that never touch `PATH` at all.
///
/// Built once per detect (it spawns `reg` on Windows) and passed to [`find_in`],
/// rather than rebuilt per lookup.
pub fn search_dirs() -> Vec<PathBuf> {
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

/// A directory's identity for de-duping — case-insensitive on Windows, where
/// `C:\Foo` and `c:\foo` are the same directory.
fn directory_key(directory: &Path) -> String {
    let key = directory
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_string();
    if cfg!(windows) {
        key.to_lowercase()
    } else {
        key
    }
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
        .flat_map(|value| std::env::split_paths(&expand_env(&value)).collect::<Vec<PathBuf>>())
        .collect()
}

#[cfg(not(windows))]
fn live_path_directories() -> Vec<PathBuf> {
    Vec::new()
}

/// The single home for reading the registry via `reg query` — no registry crate,
/// per the minimize-dependencies rule. `arguments` are the tokens after `query`
/// (a key, optionally `/v <name>`); returns the captured stdout on success, or
/// `None` when the command can't spawn or `reg` reports a missing key. Every
/// registry read (`registry_string`, the context-menu probes) routes here so the
/// spawn and success-check live in exactly one place (DRY).
#[cfg(windows)]
pub(crate) fn registry_query(arguments: &[&str]) -> Option<String> {
    let output = command("reg").arg("query").args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// One string value out of the registry. The value comes back unexpanded, because
/// `Path` is stored as a `REG_EXPAND_SZ` full of `%VAR%` references.
#[cfg(windows)]
fn registry_string(key: &str, name: &str) -> Option<String> {
    let text = registry_query(&[key, "/v", name])?;
    // A hit prints as `    Path    REG_EXPAND_SZ    C:\one;C:\two`.
    text.lines()
        .filter(|line| line.trim_start().starts_with(name))
        .find_map(|line| {
            line.split_once("REG_EXPAND_SZ")
                .or_else(|| line.split_once("REG_SZ"))
        })
        .map(|(_, value)| value.trim().to_string())
}

/// Expand `%VAR%` references against the process environment. An unset variable
/// is left verbatim (the resulting path simply won't exist, and is skipped).
#[cfg(windows)]
fn expand_env(value: &str) -> String {
    let mut expanded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(open) = rest.find('%') {
        expanded.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('%') else {
            expanded.push('%');
            rest = after;
            continue;
        };
        let name = &after[..close];
        if let Ok(value) = std::env::var(name) {
            expanded.push_str(&value);
        } else {
            expanded.push('%');
            expanded.push_str(name);
            expanded.push('%');
        }
        rest = &after[close + 1..];
    }
    expanded.push_str(rest);
    expanded
}

/// The bin directories package managers drop CLIs into. Several of them never add
/// themselves to `PATH` (or add it only for future login sessions), so a detect
/// that trusted `PATH` alone would miss a perfectly working install.
fn package_manager_directories() -> Vec<PathBuf> {
    let Some(home) = home_dir() else {
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
/// winget's shim folder — plus every winget *portable package* folder, because
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

/// The first of `names` that exists as an executable in `dirs`. Name-major on
/// purpose: the caller passes its preferred name first and every directory is
/// tried for it before falling back to an alternate spelling.
pub fn find_in(dirs: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    let extensions = executable_extensions();
    names.iter().find_map(|name| {
        dirs.iter().find_map(|dir| {
            extensions
                .iter()
                .map(|extension| dir.join(format!("{name}{extension}")))
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
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        extensions.extend(
            pathext
                .split(';')
                .map(str::trim)
                .filter(|extension| extension.starts_with('.'))
                .map(str::to_lowercase),
        );
    }
    extensions
}

/// Is this path a file we could actually exec? (On Unix that means the execute
/// bit is set — a readable-but-not-executable file is not a command.)
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
/// up across [`search_dirs`]. For a one-off lookup — a caller resolving several
/// commands should build [`search_dirs`] once and call [`find_in`].
pub fn resolve(command: &str) -> Option<PathBuf> {
    let given = Path::new(command);
    if given.components().count() > 1 {
        return is_executable(given).then(|| given.to_path_buf());
    }
    find_in(&search_dirs(), &[command])
}

/// Is `command` runnable on this machine? Sees installs made since ADE launched —
/// see [`search_dirs`].
pub fn is_on_path(command: &str) -> bool {
    resolve(command).is_some()
}

/// Parse a JSON file into a `serde_json::Value`, or `None` when it can't be read
/// or isn't valid JSON. The single home for the read-then-parse pair every local
/// credential / auth probe repeats (DRY) — an expected miss (no file, bad JSON)
/// is a plain `None`, never an error to handle.
pub fn read_json(path: &Path) -> Option<serde_json::Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Read a JSONC file — the dialect Windows Terminal and VS Code write, where
/// `//` and `/* … */` comments and a trailing comma before a closing brace or
/// bracket are all legal. `read_json` is the right reader for strict JSON; this
/// one is for a file a *human* edits, where the comments the vendor's own
/// template ships with would otherwise make the file unreadable.
pub fn read_jsonc(path: &Path) -> Option<serde_json::Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&strip_jsonc_sugar(&raw)).ok()
}

/// Rewrite JSONC as plain JSON: comments dropped, trailing commas dropped, and
/// everything inside a string left exactly as written (a `//` in a URL is not a
/// comment, and neither is a `,` in a label).
fn strip_jsonc_sugar(raw: &str) -> String {
    let mut json = String::with_capacity(raw.len());
    let mut characters = raw.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(character) = characters.next() {
        if in_string {
            json.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }

        match character {
            '"' => {
                in_string = true;
                json.push(character);
            }
            '/' if characters.peek() == Some(&'/') => {
                // A line comment ends at the newline, which is kept so the rest
                // of the file keeps the line structure the user wrote.
                for skipped in characters.by_ref() {
                    if skipped == '\n' {
                        json.push('\n');
                        break;
                    }
                }
            }
            '/' if characters.peek() == Some(&'*') => {
                characters.next();
                let mut star_seen = false;
                for skipped in characters.by_ref() {
                    if star_seen && skipped == '/' {
                        break;
                    }
                    star_seen = skipped == '*';
                }
            }
            '}' | ']' => {
                while json.ends_with(char::is_whitespace) {
                    json.pop();
                }
                if json.ends_with(',') {
                    json.pop();
                }
                json.push(character);
            }
            _ => json.push(character),
        }
    }

    json
}

/// The user's home directory, cross-platform, without pulling in a dependency
/// (`USERPROFILE` on Windows, `HOME` elsewhere).
pub fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from)
}

/// Percent-encode `value` per RFC 3986: the unreserved set (`A–Z a–z 0–9 - _ . ~`)
/// and any bytes in `extra` stay literal, everything else becomes `%XX`. Callers
/// pass `extra` for characters a target must keep verbatim (e.g. `/` and `:` in a
/// URL path). One authoritative encoder (DRY) for every `%XX` need.
pub fn percent_encode(value: &str, extra: &[u8]) -> String {
    let mut output = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        let literal = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || extra.contains(&byte);
        if literal {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(hex_nibble(byte >> 4)));
            output.push(char::from(hex_nibble(byte & 0x0f)));
        }
    }
    output
}

/// The uppercase hex character for a nibble (0..=15).
fn hex_nibble(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        _ => b'A' + (nibble - 10),
    }
}

/// Encode an absolute path to Claude Code's project-dir name: drive colon and
/// both separators (`:` `\` `/`) collapse to `-`. Claude stores each project's
/// transcript under `~/.claude/projects/<encoded-path>/`.
pub fn encode_project(path: &str) -> String {
    path.chars()
        .map(|character| {
            if matches!(character, ':' | '\\' | '/') {
                '-'
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::expand_env;
    use super::{find_in, owner_access, strip_jsonc_sugar, OwnerAccess};
    use std::fs;
    use std::path::PathBuf;
    use std::slice;

    /// A scratch directory populated with empty, executable `files`.
    fn scratch(name: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pade-search-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        for file in files {
            let path = dir.join(file);
            fs::write(&path, b"").expect("create scratch file");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                    .expect("mark scratch file executable");
            }
        }
        dir
    }

    const CODEX_NAMES: &[&str] = &["codex", "codex-x86_64-pc-windows-msvc"];

    #[test]
    fn owner_access_distinguishes_vacant_owned_and_foreign_resources() {
        assert_eq!(owner_access(None, "main"), OwnerAccess::Vacant);
        assert_eq!(owner_access(Some("main"), "main"), OwnerAccess::Owned);
        assert_eq!(owner_access(Some("other"), "main"), OwnerAccess::Foreign);
    }

    #[test]
    fn find_in_falls_back_to_an_alias() {
        // What winget's Codex package actually looks like: no `codex` anywhere,
        // only the release binary under its target-triple name.
        let dir = scratch("alias", &["codex-x86_64-pc-windows-msvc"]);
        assert_eq!(
            find_in(slice::from_ref(&dir), CODEX_NAMES),
            Some(dir.join("codex-x86_64-pc-windows-msvc"))
        );
    }

    #[test]
    fn find_in_prefers_the_canonical_name_over_an_alias() {
        let dir = scratch("canonical", &["codex", "codex-x86_64-pc-windows-msvc"]);
        assert_eq!(
            find_in(slice::from_ref(&dir), CODEX_NAMES),
            Some(dir.join("codex"))
        );
    }

    #[test]
    fn find_in_reports_a_command_that_is_not_installed() {
        let dir = scratch("empty", &[]);
        assert_eq!(find_in(slice::from_ref(&dir), CODEX_NAMES), None);
    }

    /// The whole point of the JSONC reader: a settings file a human edits keeps
    /// its comments, and the `//` inside a URL is not one of them.
    #[test]
    fn strip_jsonc_sugar_drops_comments_but_never_touches_a_string() {
        let raw = concat!(
            "{\n",
            "  // the schema link is a comment magnet\n",
            "  \"$schema\": \"https://aka.ms/terminal-profiles-schema\",\n",
            "  /* block comments too */\n",
            "  \"quoted\": \"a // b /* c */ d\",\n",
            "  \"escaped\": \"a \\\" // still a string\",\n",
            "  \"trailing\": [1, 2,],\n",
            "}\n"
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&strip_jsonc_sugar(raw)).expect("JSONC parses once relaxed");
        assert_eq!(parsed["$schema"], "https://aka.ms/terminal-profiles-schema");
        assert_eq!(parsed["quoted"], "a // b /* c */ d");
        assert_eq!(parsed["escaped"], "a \" // still a string");
        assert_eq!(parsed["trailing"], serde_json::json!([1, 2]));
    }

    /// The registry hands back `PATH` as a `REG_EXPAND_SZ`, so a directory only
    /// becomes searchable once its `%VAR%`s are resolved.
    #[cfg(windows)]
    #[test]
    fn expand_env_resolves_known_variables_and_leaves_unknown_ones() {
        let home = std::env::var("USERPROFILE").expect("USERPROFILE is always set on Windows");
        assert_eq!(
            expand_env(r"%USERPROFILE%\bin;%PADE_UNSET_VAR%\bin"),
            format!(r"{home}\bin;%PADE_UNSET_VAR%\bin")
        );
    }
}
