//! Just enough TOML and quoting to read a manifest's string arrays and values without a parser dependency.

/// Trim one matching pair of surrounding quotes.
pub(super) fn strip_quotes(raw: &str) -> &str {
    for quote in ['"', '\''] {
        let unquoted = raw
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote));
        if let Some(inner) = unquoted {
            return inner;
        }
    }
    raw
}

// ── Mini TOML scan ───────────────────────────────────────────────────────────
// Hand scan covering the shapes workspace manifests actually use (per
// minimize-dependencies, and matching `tasks.rs`'s precedent). The census
// confirms membership afterwards, so a permissive parse stays safe.

/// The string entries of `key = [ … ]` inside `[table]` - single-line or
/// multi-line arrays, `#` comments stripped quote-aware.
pub(super) fn toml_string_array(text: &str, table: &str, key: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut in_table = false;
    let mut in_array = false;
    for line in text.lines() {
        let trimmed = strip_toml_comment(line).trim();
        if in_array {
            entries.extend(quoted_strings(trimmed));
            if trimmed.contains(']') {
                in_array = false;
            }
            continue;
        }
        if let Some(is_wanted_table) = header_names_table(trimmed, table) {
            in_table = is_wanted_table;
            continue;
        }
        if !in_table {
            continue;
        }
        let Some(value) = key_value(trimmed, key) else {
            continue;
        };
        entries.extend(quoted_strings(value));
        in_array = value.contains('[') && !value.contains(']');
    }
    entries
}

/// The string value of `key = "…"` inside `[table]`.
pub(super) fn toml_string_value(text: &str, table: &str, key: &str) -> Option<String> {
    let mut in_table = false;
    for line in text.lines() {
        let trimmed = strip_toml_comment(line).trim();
        if let Some(is_wanted_table) = header_names_table(trimmed, table) {
            in_table = is_wanted_table;
            continue;
        }
        if !in_table {
            continue;
        }
        let Some(value) = key_value(trimmed, key) else {
            continue;
        };
        return quoted_strings(value).into_iter().next();
    }
    None
}

/// For a `[table]` header line, whether it opens `table`; `None` for any
/// other line. A `[` line that never closes opens no table at all.
pub(super) fn header_names_table(trimmed_line: &str, table: &str) -> Option<bool> {
    let header = trimmed_line.strip_prefix('[')?;
    Some(
        header
            .strip_suffix(']')
            .is_some_and(|name| name.trim() == table),
    )
}

/// The value part of `key = value` when the line assigns exactly `key`.
pub(super) fn key_value<'line>(line: &'line str, key: &str) -> Option<&'line str> {
    let (head, value) = line.split_once('=')?;
    (head.trim().trim_matches('"') == key).then_some(value)
}

/// Every `"…"` / `'…'` string literal in a line fragment.
pub(super) fn quoted_strings(fragment: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut characters = fragment.chars();
    while let Some(character) = characters.next() {
        if character != '"' && character != '\'' {
            continue;
        }
        let literal: String = characters
            .by_ref()
            .take_while(|&inner| inner != character)
            .collect();
        found.push(literal);
    }
    found
}

/// A TOML line with its trailing `#` comment removed (quote-aware, so a `#`
/// inside a pattern string survives).
pub(super) fn strip_toml_comment(line: &str) -> &str {
    let mut in_string: Option<char> = None;
    for (index, character) in line.char_indices() {
        match in_string {
            Some(quote) => {
                if character == quote {
                    in_string = None;
                }
            }
            None if character == '"' || character == '\'' => in_string = Some(character),
            None if character == '#' => return &line[..index],
            None => {}
        }
    }
    line
}
