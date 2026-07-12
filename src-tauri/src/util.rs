//! Small cross-cutting helpers shared by multiple modules (DRY).

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

/// A `Command` that never flashes a console window on Windows. Every background
/// or captured-output spawn — PATH lookups, git, curl, the agent namer, task
/// runners, registry edits — goes through this so a GUI app stays windowless
/// instead of popping a `conhost` window per spawn (e.g. on the 5s agent
/// re-detect). Interactive terminals the user explicitly opens (`os.rs`) are
/// spawned directly so they *do* get a window.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW — the child gets no console window.
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// Is `command` resolvable on PATH? Uses the platform's own resolver (`where`
/// on Windows, `which` elsewhere) so shims (.cmd/.ps1) resolve as a shell would.
pub fn is_on_path(command: &str) -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    crate::util::command(finder)
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

/// Percent-encode `value` per RFC 3986: the unreserved set (`A–Z a–z 0–9 - _ . ~`)
/// and any bytes in `extra` stay literal, everything else becomes `%XX`. Callers
/// pass `extra` for characters a target must keep verbatim (e.g. `/` and `:` in a
/// URL path). One authoritative encoder (DRY) for every `%XX` need.
pub fn percent_encode(value: &str, extra: &[u8]) -> String {
    let mut out = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        let literal = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || extra.contains(&byte);
        if literal {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(hex_nibble(byte >> 4)));
            out.push(char::from(hex_nibble(byte & 0x0f)));
        }
    }
    out
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
        .map(|c| {
            if matches!(c, ':' | '\\' | '/') {
                '-'
            } else {
                c
            }
        })
        .collect()
}
