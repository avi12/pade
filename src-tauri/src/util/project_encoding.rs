//! Claude Code's project-folder name for a path (its dash encoding and its length-cut hash).

/// Claude Code cuts a project-dir name this long and appends a hash of the path.
const MAXIMUM_PROJECT_NAME: usize = 200;

/// Encode an absolute path to Claude Code's project-dir name, as Claude does:
/// every character but an ASCII letter or digit becomes `-` (one per UTF-16
/// unit, since Claude replaces JavaScript string units), and a name past 200
/// characters is cut there and suffixed with a hash of the path. Claude stores
/// each project's transcripts under `~/.claude/projects/<encoded-path>/`.
pub fn encode_project(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for character in path.chars() {
        if character.is_ascii_alphanumeric() {
            encoded.push(character);
            continue;
        }
        for _ in 0..character.len_utf16() {
            encoded.push('-');
        }
    }
    if encoded.len() <= MAXIMUM_PROJECT_NAME {
        return encoded;
    }
    encoded.truncate(MAXIMUM_PROJECT_NAME);
    format!(
        "{encoded}-{}",
        base36(java_string_hash(path).unsigned_abs())
    )
}

/// The 32-bit string hash Claude Code uses (Java's `hashCode`) over the path's
/// UTF-16 units.
fn java_string_hash(text: &str) -> i32 {
    let mut hash: i32 = 0;
    for unit in text.encode_utf16() {
        hash = hash
            .wrapping_shl(5)
            .wrapping_sub(hash)
            .wrapping_add(i32::from(unit));
    }
    hash
}

/// `value` in lower-case base 36, as JavaScript's `toString(36)` writes it.
fn base36(mut value: u32) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut digits = Vec::new();
    loop {
        digits.push(DIGITS[usize::try_from(value % 36).unwrap_or_default()]);
        value /= 36;
        if value == 0 {
            break;
        }
    }
    digits.reverse();
    String::from_utf8(digits).unwrap_or_default()
}
