//! Tests for member discovery.

use crate::test_support::assert_empty;
use std::fs;
use std::path::{Path, PathBuf};

use super::patterns::{
    glob_match, go_work_patterns, package_json_patterns, pnpm_workspace_patterns, MemberPatterns,
};
use super::toml_text::{toml_string_array, toml_string_value};
use super::{discover_members, package_name_from_text, Ecosystem};

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
    assert_empty(&patterns.exclude);
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
    let text = "[package]\nmembers = [\"nope\"]\n\n[tool.uv.workspace]\nmembers = [\"libs/*\"]\n";
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
        package_name_from_text(
            "module example.com/team/api\n\ngo 1.22\n",
            Ecosystem::Golang
        ),
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
fn a_nested_checkout_is_a_member_with_no_manifest_at_all() {
    // Nobody declares these anywhere - a folder of cloned repos has no
    // workspace file to list them - so `.git` alone has to confirm one.
    let root = scratch_root("nested-checkout");
    write(&root, "companion/.git/HEAD", "ref: refs/heads/main\n");
    write(&root, "docs/readme.md", "not a member");

    let members = discover_members(&root);
    let paths: Vec<&str> = members.iter().map(|member| member.path.as_str()).collect();
    assert_eq!(paths, ["", "companion"]);
    let companion = members
        .iter()
        .find(|member| member.path == "companion")
        .expect("companion member");
    assert!(companion.repository);
    assert_eq!(companion.name, None);
    assert_eq!(companion.ecosystem, None);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_worktree_or_submodule_dot_git_file_counts_too() {
    // A worktree and a submodule keep a `.git` FILE pointing at the real git
    // dir, so the walk tests the name and never the kind.
    let root = scratch_root("git-file");
    write(
        &root,
        "external/sdk/.git",
        "gitdir: ../../.git/modules/sdk\n",
    );

    let members = discover_members(&root);
    let paths: Vec<&str> = members.iter().map(|member| member.path.as_str()).collect();
    assert_eq!(paths, ["", "external/sdk"]);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_checkout_the_workspace_patterns_exclude_is_still_a_member() {
    // Cargo's `exclude` speaks about packages in ITS build, which says
    // nothing about a checkout's right to exist in the workspace.
    let root = scratch_root("excluded-checkout");
    write(
        &root,
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/legacy\"]\n",
    );
    write(
        &root,
        "crates/legacy/Cargo.toml",
        "[package]\nname = \"legacy\"\n",
    );
    write(&root, "crates/legacy/.git/HEAD", "ref: refs/heads/main\n");

    let members = discover_members(&root);
    let legacy = members
        .iter()
        .find(|member| member.path == "crates/legacy")
        .expect("legacy member");
    assert!(legacy.repository);
    assert_eq!(legacy.name.as_deref(), Some("legacy"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_package_that_is_also_a_checkout_reports_both() {
    let root = scratch_root("package-and-checkout");
    write(&root, "pnpm-workspace.yaml", "packages:\n  - frontend\n");
    write(&root, "package.json", r#"{ "name": "root" }"#);
    write(&root, ".git/HEAD", "ref: refs/heads/main\n");
    write(&root, "frontend/package.json", r#"{ "name": "web" }"#);
    write(&root, "frontend/.git/HEAD", "ref: refs/heads/main\n");

    let members = discover_members(&root);
    let root_member = members.first().expect("root member");
    assert_eq!(root_member.path, "");
    assert!(root_member.repository);

    let frontend = members
        .iter()
        .find(|member| member.path == "frontend")
        .expect("frontend member");
    assert!(frontend.repository);
    assert_eq!(frontend.name.as_deref(), Some("web"));
    assert_eq!(frontend.ecosystem, Some(Ecosystem::JavaScript));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_workspace_with_no_git_reports_a_root_that_is_no_repository() {
    let root = scratch_root("no-git");
    write(&root, "package.json", r#"{ "name": "solo" }"#);

    let members = discover_members(&root);
    assert_eq!(members.len(), 1);
    assert!(!members[0].repository);

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
fn the_highest_priority_manifest_names_a_directory_whatever_the_listing_order() {
    let root = scratch_root("manifest-priority");
    write(&root, "pyproject.toml", "[project]\nname = \"py\"\n");
    write(&root, "Cargo.toml", "[package]\nname = \"rs\"\n");
    write(&root, "package.json", r#"{ "name": "js" }"#);
    write(&root, "go.mod", "module example.com/go\n");

    let members = discover_members(&root);
    assert_eq!(members[0].ecosystem, Some(Ecosystem::JavaScript));
    assert_eq!(members[0].name.as_deref(), Some("js"));

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
    assert_eq!(api.ecosystem, Some(Ecosystem::Golang));
    assert_eq!(api.name.as_deref(), Some("api"));

    let _ = fs::remove_dir_all(&root);
}
