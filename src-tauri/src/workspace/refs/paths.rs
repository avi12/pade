//! Re-pointing a path inside text: the one rewriter for a folder's raw, escaped, forward-slash and file-URI forms.

use std::path::Path;

// ---------------------------------------------------------------------------
// Shared string helpers.
// ---------------------------------------------------------------------------

/// VS Code (and forks) store recent folders as file URIs, e.g.
/// `C:\repositories\avi\foo` → `file:///c%3A/repositories/avi/foo`:
/// lowercase drive letter, `:` percent-encoded to `%3A`, `\` → `/`.
pub(super) fn vscode_uri(path: &str) -> String {
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
pub(super) const PATH_END_CHARACTERS: [char; 6] = ['\\', '/', '"', '\'', '\n', '\r'];

/// Re-point every stored form of the folder `old` (and of anything inside it) to
/// `new`: the raw backslash path, its JSON-escaped form, the forward-slash path
/// and the VS Code file-URI form. Matched case-insensitively, as Windows paths
/// are; a match stored in lower case (Codex keys its projects that way) is
/// written back in lower case. Shared by every adapter, so each tool's own
/// storage format needs no substitution logic of its own.
pub(super) fn rebase_paths_in(haystack: &str, old: &str, new: &str) -> String {
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
pub(crate) fn moved_path(path: &str, old: &str, new: &str) -> Option<String> {
    let moved = rebase_paths_in(path, old, new);
    (moved != path).then_some(moved)
}

/// `haystack` with each folder-bounded, case-insensitive `old` swapped for `new`.
pub(super) fn replace_folder(haystack: &str, old: &str, new: &str) -> String {
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
pub(super) fn rewrite_file_forms(path: &Path, old: &str, new: &str) {
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
