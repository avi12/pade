//! The member include/exclude patterns a root declares in its workspace-defining files (pnpm, npm, Cargo, uv, Go).

use super::toml_text::strip_quotes;
use super::toml_text::toml_string_array;
use crate::workspace::glob::segments_match;
use std::fs;
use std::path::Path;

// ── Member patterns ──────────────────────────────────────────────────────────

/// The member include/exclude patterns the root's workspace-defining files
/// declare. Explicit path lists (`go.work`) ride along as zero-wildcard globs
/// so one matcher handles both.
#[derive(Default, Debug)]
pub(super) struct MemberPatterns {
    pub(super) include: Vec<String>,
    pub(super) exclude: Vec<String>,
}

impl MemberPatterns {
    /// File a declared pattern into include or exclude (a leading `!` negates).
    fn add(&mut self, raw: &str) {
        if let Some(negated) = raw.trim().strip_prefix('!') {
            self.subtract(negated);
            return;
        }
        if let Some(pattern) = normalize_pattern(raw) {
            self.include.push(pattern);
        }
    }

    /// File a pattern straight into exclude (Cargo/uv `exclude` arrays).
    fn subtract(&mut self, raw: &str) {
        if let Some(pattern) = normalize_pattern(raw) {
            self.exclude.push(pattern);
        }
    }
}

/// Normalize a declared member pattern to the census's repo-relative shape:
/// forward slashes, no leading `./`, no trailing `/`.
pub(super) fn normalize_pattern(raw: &str) -> Option<String> {
    let forward = raw.trim().replace('\\', "/");
    let body = forward.strip_prefix("./").unwrap_or(&forward);
    let body = body.trim_end_matches('/');
    if body.is_empty() {
        return None;
    }
    Some(body.to_string())
}

/// A pattern source's parser: one defining file's text → declared patterns.
pub(super) type PatternParser = fn(&str, &mut MemberPatterns);

/// Workspace-defining file at the root → its pattern parser. The seam a new
/// ecosystem plugs into: add a row, everything downstream is shared.
pub(super) const PATTERN_SOURCES: &[(&str, PatternParser)] = &[
    ("pnpm-workspace.yaml", pnpm_workspace_patterns),
    ("package.json", package_json_patterns),
    ("Cargo.toml", cargo_workspace_patterns),
    ("go.work", go_work_patterns),
    ("pyproject.toml", uv_workspace_patterns),
];

/// Gather member patterns from every workspace-defining file present at the
/// root - a polyglot repo can declare several at once.
pub(super) fn collect_patterns(root: &Path) -> MemberPatterns {
    let mut patterns = MemberPatterns::default();
    for (file, parse) in PATTERN_SOURCES {
        let Ok(text) = fs::read_to_string(root.join(file)) else {
            continue;
        };
        parse(&text, &mut patterns);
    }
    patterns
}

/// `pnpm-workspace.yaml`'s flat `packages:` block list - the only shape pnpm
/// documents for members, so a line scan beats a YAML dependency. `!` entries
/// subtract.
pub(super) fn pnpm_workspace_patterns(text: &str, patterns: &mut MemberPatterns) {
    let mut in_packages = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let is_top_level_key = !line.starts_with([' ', '\t']) && !trimmed.starts_with('-');
        if is_top_level_key {
            in_packages = trimmed == "packages:";
            continue;
        }
        if !in_packages {
            continue;
        }
        let Some(item) = trimmed.strip_prefix('-') else {
            continue;
        };
        patterns.add(strip_quotes(item.trim()));
    }
}

/// `package.json` `"workspaces"` - a glob array, or Yarn-classic's
/// `{ "packages": [...] }` object form. `!` entries subtract (Bun/Yarn).
pub(super) fn package_json_patterns(text: &str, patterns: &mut MemberPatterns) {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    let Some(workspaces) = json.get("workspaces") else {
        return;
    };
    let list = workspaces.as_array().or_else(|| {
        workspaces
            .get("packages")
            .and_then(serde_json::Value::as_array)
    });
    let Some(list) = list else {
        return;
    };
    for entry in list.iter().filter_map(serde_json::Value::as_str) {
        patterns.add(entry);
    }
}

/// `Cargo.toml` `[workspace]` `members` / `exclude` arrays (Cargo's globs are
/// `*`/`?`, already within the matcher's power).
pub(super) fn cargo_workspace_patterns(text: &str, patterns: &mut MemberPatterns) {
    for pattern in toml_string_array(text, "workspace", "members") {
        patterns.add(&pattern);
    }
    for pattern in toml_string_array(text, "workspace", "exclude") {
        patterns.subtract(&pattern);
    }
}

/// `pyproject.toml` `[tool.uv.workspace]` `members` / `exclude` glob arrays.
pub(super) fn uv_workspace_patterns(text: &str, patterns: &mut MemberPatterns) {
    for pattern in toml_string_array(text, "tool.uv.workspace", "members") {
        patterns.add(&pattern);
    }
    for pattern in toml_string_array(text, "tool.uv.workspace", "exclude") {
        patterns.subtract(&pattern);
    }
}

/// `go.work` `use` directives - literal paths (no globs), either a single
/// `use ./dir` or a `use ( … )` block; `//` comments stripped.
pub(super) fn go_work_patterns(text: &str, patterns: &mut MemberPatterns) {
    let mut in_use_block = false;
    for line in text.lines() {
        let uncommented = line.split("//").next().unwrap_or_default().trim();
        if in_use_block {
            if uncommented == ")" {
                in_use_block = false;
                continue;
            }
            if !uncommented.is_empty() {
                patterns.add(strip_quotes(uncommented));
            }
            continue;
        }
        if uncommented == "use (" || uncommented == "use(" {
            in_use_block = true;
            continue;
        }
        if let Some(path) = uncommented.strip_prefix("use ") {
            patterns.add(strip_quotes(path.trim()));
        }
    }
}

// ── Segment-aware glob match ─────────────────────────────────────────────────

/// Segment-aware glob match: `*`/`?` stay within one path segment, `**` spans
/// any number of segments - so `packages/*` is one level and `components/**`
/// recurses, matching the workspace tools' own semantics.
pub(super) fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern_segments: Vec<&str> = pattern.split('/').collect();
    let path_segments: Vec<&str> = path.split('/').collect();
    segments_match(&pattern_segments, &path_segments)
}
