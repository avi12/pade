//! JSON and JSONC files read leniently from disk.

use std::path::Path;

/// Parse a JSON file into a `serde_json::Value`, or `None` when it can't be read
/// or isn't valid JSON. The single home for the read-then-parse pair every local
/// credential / auth probe repeats (DRY) - an expected miss (no file, bad JSON)
/// is a plain `None`, never an error to handle.
pub fn read_json(path: &Path) -> Option<serde_json::Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// The raw value of the first `"key":"value"` occurrence in `json`, without a
/// full parse - for a JSON line too large or too private to model with serde
/// (a Codex rollout's meta line, a transcript entry) when only a flat string
/// field is wanted. Escaped quotes inside the value are not handled; escaped
/// backslashes come back still escaped, for the caller to undo.
pub(crate) fn json_string_field(json: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker)? + marker.len();
    let end = json[start..].find('"')?;
    Some(json[start..start + end].to_string())
}

/// Read a JSONC file - the dialect Windows Terminal and VS Code write, where
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
pub(super) fn strip_jsonc_sugar(raw: &str) -> String {
    let mut json = String::with_capacity(raw.len());
    let mut characters = raw.chars().peekable();
    let mut is_in_string = false;
    let mut is_escaped = false;

    while let Some(character) = characters.next() {
        if is_in_string {
            json.push(character);
            if is_escaped {
                is_escaped = false;
            } else if character == '\\' {
                is_escaped = true;
            } else if character == '"' {
                is_in_string = false;
            }
            continue;
        }

        match character {
            '"' => {
                is_in_string = true;
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
                let mut is_after_star = false;
                for skipped in characters.by_ref() {
                    if is_after_star && skipped == '/' {
                        break;
                    }
                    is_after_star = skipped == '*';
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
