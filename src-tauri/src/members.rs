//! Manifest-driven workspace member discovery for the Change Feed's grouping.
//!
//! A folder is a package IFF it holds its own manifest — never because of its
//! name. One walk builds a manifest census (every directory holding a real
//! package manifest, dependency/build/VCS noise pruned); the root's
//! workspace-defining files (`pnpm-workspace.yaml`, `package.json`
//! `"workspaces"`, `Cargo.toml` `[workspace]`, `go.work`, `pyproject.toml`
//! `[tool.uv.workspace]`) contribute include/exclude member patterns; and a
//! census directory becomes a member only when an include pattern matches it
//! and no exclude does. The root itself is always a member, so every changed
//! file has an enclosing bucket. Read-only; the frontend does the
//! longest-prefix file→member assignment (`change-groups.ts`).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

/// The package ecosystems whose manifests confirm a directory as a package.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    JavaScript,
    Rust,
    Go,
    Python,
}

/// Manifest filename → the ecosystem it confirms. Order is priority when one
/// directory holds several manifests (a JS app with a `Cargo.toml` sidecar
/// reads its name from `package.json`).
const MANIFESTS: &[(&str, Ecosystem)] = &[
    ("package.json", Ecosystem::JavaScript),
    ("Cargo.toml", Ecosystem::Rust),
    ("go.mod", Ecosystem::Go),
    ("pyproject.toml", Ecosystem::Python),
];

/// Directories never worth walking (build output, deps, vendored code);
/// dot-directories (`.git`, `.venv`, …) are pruned separately.
const PRUNED_DIRS: &[&str] = &["node_modules", "target", "dist", "build", "vendor"];

/// One confirmed workspace member, ready for the frontend's longest-prefix
/// bucket assignment.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    /// Repo-relative `/`-joined directory (`""` = the repo root).
    pub path: String,
    /// The package name its manifest declares, when it declares one.
    pub name: Option<String>,
    /// The manifest family that confirmed it; `None` only for a root that has
    /// no manifest at all (kept so every file still has a bucket).
    pub ecosystem: Option<Ecosystem>,
}

/// Manifest-confirmed members of the workspace at `root` (the open project),
/// for the Change Feed's manifest-driven grouping. The root is always first.
#[tauri::command]
pub fn members_list(root: String) -> Result<Vec<Member>, String> {
    let root = Path::new(&root);
    if !root.is_dir() {
        return Err(format!("not a directory: {}", root.display()));
    }
    Ok(discover_members(root))
}

/// Every confirmed member: the root, plus each census directory that the
/// declared member patterns include (all includes first, then excludes
/// subtract — negation is order-sensitive in every ecosystem).
fn discover_members(root: &Path) -> Vec<Member> {
    let census = manifest_census(root);
    let patterns = collect_patterns(root);

    let root_manifest = census.get("");
    let mut members = vec![Member {
        path: String::new(),
        name: root_manifest.and_then(|found| found.name.clone()),
        ecosystem: root_manifest.map(|found| found.ecosystem),
    }];
    for (path, found) in &census {
        if path.is_empty() {
            continue;
        }
        let included = patterns
            .include
            .iter()
            .any(|pattern| glob_match(pattern, path));
        let excluded = patterns
            .exclude
            .iter()
            .any(|pattern| glob_match(pattern, path));
        if included && !excluded {
            members.push(Member {
                path: path.clone(),
                name: found.name.clone(),
                ecosystem: Some(found.ecosystem),
            });
        }
    }
    members
}

/// A directory the census confirmed as a package.
struct FoundManifest {
    ecosystem: Ecosystem,
    name: Option<String>,
}

/// Walk once from the root recording every directory that holds a real package
/// manifest, pruning noise directories and hidden directories during the walk
/// (a greedy member glob must never sweep in `node_modules`).
fn manifest_census(root: &Path) -> BTreeMap<String, FoundManifest> {
    let mut found = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut file_names = Vec::new();
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy().into_owned();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                let is_noise = name.starts_with('.') || PRUNED_DIRS.contains(&name.as_str());
                if !is_noise {
                    stack.push(entry.path());
                }
                continue;
            }
            file_names.push(name);
        }
        let manifest = MANIFESTS
            .iter()
            .find(|(manifest, _)| file_names.iter().any(|name| name == manifest));
        let Some((manifest_name, ecosystem)) = manifest else {
            continue;
        };
        found.insert(
            relative_display(root, &dir),
            FoundManifest {
                ecosystem: *ecosystem,
                name: package_name(&dir.join(manifest_name), *ecosystem),
            },
        );
    }
    found
}

/// A directory's path relative to the root, `/`-joined (`""` for the root).
fn relative_display(root: &Path, dir: &Path) -> String {
    dir.strip_prefix(root)
        .unwrap_or(dir)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The declared package name inside one manifest file.
fn package_name(manifest: &Path, ecosystem: Ecosystem) -> Option<String> {
    let text = fs::read_to_string(manifest).ok()?;
    package_name_from_text(&text, ecosystem)
}

/// The pure read behind [`package_name`], per manifest family.
fn package_name_from_text(text: &str, ecosystem: Ecosystem) -> Option<String> {
    match ecosystem {
        Ecosystem::JavaScript => {
            let json: serde_json::Value = serde_json::from_str(text).ok()?;
            json.get("name")?.as_str().map(str::to_string)
        }
        Ecosystem::Rust => toml_string_value(text, "package", "name"),
        Ecosystem::Go => go_module_name(text),
        Ecosystem::Python => toml_string_value(text, "project", "name")
            .or_else(|| toml_string_value(text, "tool.poetry", "name")),
    }
}

/// `go.mod`'s `module example.com/team/api` → the short display name `api`.
fn go_module_name(text: &str) -> Option<String> {
    let module_path = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("module "))?;
    let short = module_path.trim().trim_matches('"').rsplit('/').next()?;
    if short.is_empty() {
        return None;
    }
    Some(short.to_string())
}

// ── Member patterns ──────────────────────────────────────────────────────────

/// The member include/exclude patterns the root's workspace-defining files
/// declare. Explicit path lists (`go.work`) ride along as zero-wildcard globs
/// so one matcher handles both.
#[derive(Default, Debug)]
struct MemberPatterns {
    include: Vec<String>,
    exclude: Vec<String>,
}

impl MemberPatterns {
    /// File a declared pattern into include or exclude (a leading `!` negates).
    fn add(&mut self, raw: &str) {
        match raw.trim().strip_prefix('!') {
            Some(negated) => self.subtract(negated),
            None => {
                if let Some(pattern) = normalize_pattern(raw) {
                    self.include.push(pattern);
                }
            }
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
fn normalize_pattern(raw: &str) -> Option<String> {
    let forward = raw.trim().replace('\\', "/");
    let body = forward.strip_prefix("./").unwrap_or(&forward);
    let body = body.trim_end_matches('/');
    if body.is_empty() {
        return None;
    }
    Some(body.to_string())
}

/// A pattern source's parser: one defining file's text → declared patterns.
type PatternParser = fn(&str, &mut MemberPatterns);

/// Workspace-defining file at the root → its pattern parser. The seam a new
/// ecosystem plugs into: add a row, everything downstream is shared.
const PATTERN_SOURCES: &[(&str, PatternParser)] = &[
    ("pnpm-workspace.yaml", pnpm_workspace_patterns),
    ("package.json", package_json_patterns),
    ("Cargo.toml", cargo_workspace_patterns),
    ("go.work", go_work_patterns),
    ("pyproject.toml", uv_workspace_patterns),
];

/// Gather member patterns from every workspace-defining file present at the
/// root — a polyglot repo can declare several at once.
fn collect_patterns(root: &Path) -> MemberPatterns {
    let mut patterns = MemberPatterns::default();
    for (file, parse) in PATTERN_SOURCES {
        let Ok(text) = fs::read_to_string(root.join(file)) else {
            continue;
        };
        parse(&text, &mut patterns);
    }
    patterns
}

/// `pnpm-workspace.yaml`'s flat `packages:` block list — the only shape pnpm
/// documents for members, so a line scan beats a YAML dependency. `!` entries
/// subtract.
fn pnpm_workspace_patterns(text: &str, patterns: &mut MemberPatterns) {
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

/// `package.json` `"workspaces"` — a glob array, or Yarn-classic's
/// `{ "packages": [...] }` object form. `!` entries subtract (Bun/Yarn).
fn package_json_patterns(text: &str, patterns: &mut MemberPatterns) {
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
fn cargo_workspace_patterns(text: &str, patterns: &mut MemberPatterns) {
    for pattern in toml_string_array(text, "workspace", "members") {
        patterns.add(&pattern);
    }
    for pattern in toml_string_array(text, "workspace", "exclude") {
        patterns.subtract(&pattern);
    }
}

/// `pyproject.toml` `[tool.uv.workspace]` `members` / `exclude` glob arrays.
fn uv_workspace_patterns(text: &str, patterns: &mut MemberPatterns) {
    for pattern in toml_string_array(text, "tool.uv.workspace", "members") {
        patterns.add(&pattern);
    }
    for pattern in toml_string_array(text, "tool.uv.workspace", "exclude") {
        patterns.subtract(&pattern);
    }
}

/// `go.work` `use` directives — literal paths (no globs), either a single
/// `use ./dir` or a `use ( … )` block; `//` comments stripped.
fn go_work_patterns(text: &str, patterns: &mut MemberPatterns) {
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

/// Trim one matching pair of surrounding quotes.
fn strip_quotes(raw: &str) -> &str {
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

/// The string entries of `key = [ … ]` inside `[table]` — single-line or
/// multi-line arrays, `#` comments stripped quote-aware.
fn toml_string_array(text: &str, table: &str, key: &str) -> Vec<String> {
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
        if let Some(header) = trimmed.strip_prefix('[') {
            in_table = header
                .strip_suffix(']')
                .is_some_and(|name| name.trim() == table);
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
fn toml_string_value(text: &str, table: &str, key: &str) -> Option<String> {
    let mut in_table = false;
    for line in text.lines() {
        let trimmed = strip_toml_comment(line).trim();
        if let Some(header) = trimmed.strip_prefix('[') {
            in_table = header
                .strip_suffix(']')
                .is_some_and(|name| name.trim() == table);
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

/// The value part of `key = value` when the line assigns exactly `key`.
fn key_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let (head, value) = line.split_once('=')?;
    if head.trim().trim_matches('"') == key {
        Some(value)
    } else {
        None
    }
}

/// Every `"…"` / `'…'` string literal in a line fragment.
fn quoted_strings(fragment: &str) -> Vec<String> {
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
fn strip_toml_comment(line: &str) -> &str {
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

// ── Segment-aware glob matcher ───────────────────────────────────────────────

/// Segment-aware glob match: `*`/`?` stay within one path segment, `**` spans
/// any number of segments — so `packages/*` is one level and `components/**`
/// recurses, matching the workspace tools' own semantics.
fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern_segments: Vec<&str> = pattern.split('/').collect();
    let path_segments: Vec<&str> = path.split('/').collect();
    segments_match(&pattern_segments, &path_segments)
}

fn segments_match(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| segments_match(rest, &path[skip..])),
        Some((head, rest)) => path.split_first().is_some_and(|(first, tail)| {
            segment_matches(head, first) && segments_match(rest, tail)
        }),
    }
}

/// One path segment against one pattern segment: `*` any run of characters,
/// `?` exactly one — neither ever crosses a `/`.
fn segment_matches(pattern: &str, segment: &str) -> bool {
    let pattern_characters: Vec<char> = pattern.chars().collect();
    let segment_characters: Vec<char> = segment.chars().collect();
    characters_match(&pattern_characters, &segment_characters)
}

fn characters_match(pattern: &[char], text: &[char]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((&'*', rest)) => (0..=text.len()).any(|skip| characters_match(rest, &text[skip..])),
        Some((&'?', rest)) => text
            .split_first()
            .is_some_and(|(_, tail)| characters_match(rest, tail)),
        Some((head, rest)) => text
            .split_first()
            .is_some_and(|(first, tail)| first == head && characters_match(rest, tail)),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{
        discover_members, glob_match, go_work_patterns, package_json_patterns,
        package_name_from_text, pnpm_workspace_patterns, toml_string_array, toml_string_value,
        Ecosystem, MemberPatterns,
    };

    // ── glob matcher ─────────────────────────────────────────────────────────

    #[test]
    fn a_single_star_matches_exactly_one_level() {
        assert!(glob_match("packages/*", "packages/ui"));
        assert!(!glob_match("packages/*", "packages/ui/button"));
        assert!(!glob_match("packages/*", "packages"));
    }

    #[test]
    fn a_double_star_recurses_to_any_depth() {
        assert!(glob_match("components/**", "components/form/input"));
        assert!(glob_match("components/**", "components"));
        assert!(!glob_match("components/**", "apps/web"));
    }

    #[test]
    fn a_double_star_can_sit_mid_pattern() {
        assert!(glob_match("apps/**/plugins", "apps/web/deep/plugins"));
        assert!(glob_match("apps/**/plugins", "apps/plugins"));
        assert!(!glob_match("apps/**/plugins", "apps/web/plugins/auth"));
    }

    #[test]
    fn wildcards_never_cross_a_segment_boundary() {
        assert!(glob_match("apps/web-*", "apps/web-admin"));
        assert!(!glob_match("apps/web*", "apps/web/admin"));
        assert!(glob_match("crates/?", "crates/a"));
        assert!(!glob_match("crates/?", "crates/ab"));
    }

    #[test]
    fn a_literal_pattern_matches_only_itself() {
        assert!(glob_match("frontend", "frontend"));
        assert!(!glob_match("frontend", "frontend/app"));
        assert!(!glob_match("apps/web", "apps/web-admin"));
    }

    // ── pattern sources ──────────────────────────────────────────────────────

    #[test]
    fn pnpm_packages_list_yields_includes_and_bang_excludes() {
        let text = "packages:\n  # members\n  - \"apps/*\"\n  - packages/**\n  - '!**/test/**'\ncatalog:\n  react: ^19.0.0\n";
        let mut patterns = MemberPatterns::default();
        pnpm_workspace_patterns(text, &mut patterns);
        assert_eq!(patterns.include, ["apps/*", "packages/**"]);
        assert_eq!(patterns.exclude, ["**/test/**"]);
    }

    #[test]
    fn pnpm_list_items_under_another_key_are_ignored() {
        let text = "onlyBuiltDependencies:\n  - esbuild\npackages:\n  - frontend\n  - backend\n";
        let mut patterns = MemberPatterns::default();
        pnpm_workspace_patterns(text, &mut patterns);
        assert_eq!(patterns.include, ["frontend", "backend"]);
    }

    #[test]
    fn package_json_workspaces_reads_the_array_form() {
        let text = r#"{ "name": "root", "workspaces": ["packages/*", "!packages/private"] }"#;
        let mut patterns = MemberPatterns::default();
        package_json_patterns(text, &mut patterns);
        assert_eq!(patterns.include, ["packages/*"]);
        assert_eq!(patterns.exclude, ["packages/private"]);
    }

    #[test]
    fn package_json_workspaces_reads_yarn_classics_object_form() {
        let text = r#"{ "workspaces": { "packages": ["apps/*"], "nohoist": ["**/react"] } }"#;
        let mut patterns = MemberPatterns::default();
        package_json_patterns(text, &mut patterns);
        assert_eq!(patterns.include, ["apps/*"]);
        assert!(patterns.exclude.is_empty());
    }

    #[test]
    fn go_work_reads_single_and_block_use_directives() {
        let text = "go 1.22\n\nuse ./api // the server\nuse (\n\t./web\n\t./shared\n)\n";
        let mut patterns = MemberPatterns::default();
        go_work_patterns(text, &mut patterns);
        assert_eq!(patterns.include, ["api", "web", "shared"]);
    }

    #[test]
    fn toml_string_array_reads_single_line_arrays() {
        let text = "[workspace]\nmembers = [\"crates/*\", \"tools\"] # trailing\n";
        assert_eq!(
            toml_string_array(text, "workspace", "members"),
            ["crates/*", "tools"]
        );
    }

    #[test]
    fn toml_string_array_reads_multi_line_arrays_with_comments() {
        let text = "[workspace]\nmembers = [\n  \"crates/core\", # the core\n  'crates/cli',\n]\nexclude = [\"crates/legacy\"]\n";
        assert_eq!(
            toml_string_array(text, "workspace", "members"),
            ["crates/core", "crates/cli"]
        );
        assert_eq!(
            toml_string_array(text, "workspace", "exclude"),
            ["crates/legacy"]
        );
    }

    #[test]
    fn toml_string_array_ignores_the_same_key_in_another_table() {
        let text =
            "[package]\nmembers = [\"nope\"]\n\n[tool.uv.workspace]\nmembers = [\"libs/*\"]\n";
        assert_eq!(
            toml_string_array(text, "tool.uv.workspace", "members"),
            ["libs/*"]
        );
    }

    #[test]
    fn toml_string_value_reads_a_tables_key() {
        let text = "[package]\nname = \"core\" # crate\nversion = \"0.1.0\"\n";
        assert_eq!(
            toml_string_value(text, "package", "name"),
            Some("core".to_string())
        );
        assert_eq!(toml_string_value(text, "package", "missing"), None);
    }

    // ── package names ────────────────────────────────────────────────────────

    #[test]
    fn package_names_come_from_each_manifest_family() {
        assert_eq!(
            package_name_from_text(r#"{ "name": "@scope/web" }"#, Ecosystem::JavaScript),
            Some("@scope/web".to_string())
        );
        assert_eq!(
            package_name_from_text("[package]\nname = \"engine\"\n", Ecosystem::Rust),
            Some("engine".to_string())
        );
        assert_eq!(
            package_name_from_text("module example.com/team/api\n\ngo 1.22\n", Ecosystem::Go),
            Some("api".to_string())
        );
        assert_eq!(
            package_name_from_text("[project]\nname = \"poller\"\n", Ecosystem::Python),
            Some("poller".to_string())
        );
    }

    // ── census + discovery (filesystem fixture) ──────────────────────────────

    fn scratch_root(test: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("pade-members-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("scratch root");
        root
    }

    fn write(root: &Path, relative: &str, content: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture dir");
        }
        fs::write(path, content).expect("fixture file");
    }

    #[test]
    fn a_root_level_pnpm_workspace_confirms_each_member() {
        let root = scratch_root("root-level");
        write(
            &root,
            "pnpm-workspace.yaml",
            "packages:\n  - frontend\n  - backend\n  - shared\n",
        );
        write(&root, "package.json", r#"{ "name": "poll-avi" }"#);
        write(
            &root,
            "frontend/package.json",
            r#"{ "name": "@poll/frontend" }"#,
        );
        write(
            &root,
            "backend/package.json",
            r#"{ "name": "@poll/backend" }"#,
        );
        write(
            &root,
            "shared/package.json",
            r#"{ "name": "@poll/shared" }"#,
        );
        write(&root, "docs/readme.md", "no manifest here");

        let members = discover_members(&root);
        let paths: Vec<&str> = members.iter().map(|member| member.path.as_str()).collect();
        assert_eq!(paths, ["", "backend", "frontend", "shared"]);
        let frontend = members
            .iter()
            .find(|member| member.path == "frontend")
            .expect("frontend member");
        assert_eq!(frontend.name.as_deref(), Some("@poll/frontend"));
        assert_eq!(frontend.ecosystem, Some(Ecosystem::JavaScript));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_glob_hit_without_a_manifest_is_not_a_member() {
        let root = scratch_root("census-confirms");
        write(&root, "pnpm-workspace.yaml", "packages:\n  - \"apps/*\"\n");
        write(&root, "apps/web/package.json", r#"{ "name": "web" }"#);
        write(&root, "apps/assets/logo.svg", "<svg/>");

        let members = discover_members(&root);
        let paths: Vec<&str> = members.iter().map(|member| member.path.as_str()).collect();
        assert_eq!(paths, ["", "apps/web"]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_census_prunes_dependency_and_hidden_directories() {
        let root = scratch_root("pruning");
        write(&root, "pnpm-workspace.yaml", "packages:\n  - \"**\"\n");
        write(&root, "packages/ui/package.json", r#"{ "name": "ui" }"#);
        write(
            &root,
            "node_modules/left-pad/package.json",
            r#"{ "name": "left-pad" }"#,
        );
        write(
            &root,
            "packages/ui/node_modules/x/package.json",
            r#"{ "name": "x" }"#,
        );
        write(&root, ".hidden/package.json", r#"{ "name": "hidden" }"#);
        write(
            &root,
            "target/debug/package.json",
            r#"{ "name": "artifact" }"#,
        );

        let members = discover_members(&root);
        let paths: Vec<&str> = members.iter().map(|member| member.path.as_str()).collect();
        assert_eq!(paths, ["", "packages/ui"]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn excludes_subtract_after_includes() {
        let root = scratch_root("excludes");
        write(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/legacy\"]\n",
        );
        write(
            &root,
            "crates/core/Cargo.toml",
            "[package]\nname = \"core\"\n",
        );
        write(
            &root,
            "crates/legacy/Cargo.toml",
            "[package]\nname = \"legacy\"\n",
        );

        let members = discover_members(&root);
        let paths: Vec<&str> = members.iter().map(|member| member.path.as_str()).collect();
        assert_eq!(paths, ["", "crates/core"]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_workspace_with_no_declarations_yields_only_the_root() {
        let root = scratch_root("undeclared");
        write(&root, "package.json", r#"{ "name": "solo" }"#);
        write(&root, "frontend/package.json", r#"{ "name": "front" }"#);

        let members = discover_members(&root);
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].path, "");
        assert_eq!(members[0].name.as_deref(), Some("solo"));
        assert_eq!(members[0].ecosystem, Some(Ecosystem::JavaScript));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_polyglot_root_collects_patterns_from_every_defining_file() {
        let root = scratch_root("polyglot");
        write(&root, "pnpm-workspace.yaml", "packages:\n  - web\n");
        write(&root, "go.work", "go 1.22\nuse ./api\n");
        write(&root, "web/package.json", r#"{ "name": "web" }"#);
        write(&root, "api/go.mod", "module example.com/api\n");

        let members = discover_members(&root);
        let paths: Vec<&str> = members.iter().map(|member| member.path.as_str()).collect();
        assert_eq!(paths, ["", "api", "web"]);
        let api = members
            .iter()
            .find(|member| member.path == "api")
            .expect("api member");
        assert_eq!(api.ecosystem, Some(Ecosystem::Go));
        assert_eq!(api.name.as_deref(), Some("api"));

        let _ = fs::remove_dir_all(&root);
    }
}
