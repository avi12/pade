//! Tests for the watcher modules.

use super::arm::resolve_watch_root;
use super::change::{
    line_count, line_delta, read_authorized_file, read_preview_text, reclassify, summarize,
    surfaces, ChangeKind,
};
use super::events::record_first_touch_baseline;
use super::git_state::{
    drain_burst, git_directory_lifecycle_changed, git_state_directories, is_git_directory_entry,
    is_git_state_file, unique_directories, GitStateMessage,
};
use super::ignore::{
    is_under_static_ignore_directory, manifest_ignore_directories, static_ignore_directories,
};
use super::ignore::{IgnorePolicy, IgnoreRules};
use super::languages::{language_news, planned_language_routes, LanguageNews, LanguageRoute};
use super::mcp::{mcp_config_for_path, mcp_membership_changed};
use super::registry::WindowWatches;
use super::registry::{deepest_containing, watch_set_change, ProjectRootPayload, WatchSetChange};
use crate::platform::preview::{MAXIMUM_IMAGE_BYTES, MAXIMUM_PREVIEW_BYTES};
use std::sync::Arc;

/// `watch_projects` asks git before it locks, so a slow request can finish
/// after a newer one; only the newest may apply its set, or a quick switch
/// could leave the feed watching projects the window just let go of.
#[test]
fn only_the_newest_arm_request_may_arm() {
    let watches = WindowWatches::default();
    let older = watches.begin_arm_request();
    let newer = watches.begin_arm_request();
    assert!(!watches.is_latest_arm_request(older));
    assert!(watches.is_latest_arm_request(newer));
}

#[test]
fn a_fresh_window_is_watching_nothing() {
    let watches = WindowWatches::default();
    assert!(watches.find("C:/anything").is_none());
    assert!(watches.owner_of(Path::new("C:/anything/file.rs")).is_none());
}

fn roots(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

#[test]
fn a_new_set_arms_the_added_roots_and_drops_the_removed_ones() {
    let current = roots(&["C:/a", "C:/b"]);
    let change = watch_set_change(&current, &roots(&["C:/b", "C:/c"]));
    assert_eq!(
        change,
        WatchSetChange {
            to_arm: roots(&["C:/c"]),
            to_drop: roots(&["C:/a"]),
        }
    );
}

#[test]
fn an_unchanged_set_neither_arms_nor_drops_anything() {
    let current = roots(&["C:/a", "C:/b"]);
    let change = watch_set_change(&current, &roots(&["C:/b", "C:/a"]));
    assert_eq!(change, WatchSetChange::default());
}

#[test]
fn a_repeated_root_is_armed_once_and_an_empty_set_drops_everything() {
    let change = watch_set_change(&[], &roots(&["C:/a", "C:/a"]));
    assert_eq!(change.to_arm, roots(&["C:/a"]));

    let current = roots(&["C:/b", "C:/a"]);
    let change = watch_set_change(&current, &[]);
    assert_eq!(change.to_drop, roots(&["C:/a", "C:/b"]));
    assert_empty(&change.to_arm);
}

#[test]
fn a_path_belongs_to_the_deepest_root_that_contains_it() {
    let monorepo = Path::new("C:/work/monorepo");
    let package = Path::new("C:/work/monorepo/packages/app");
    let other = Path::new("C:/work/other");
    let candidates = [
        (monorepo, "monorepo"),
        (package, "package"),
        (other, "other"),
    ];

    let owner = |path: &str| deepest_containing(candidates, Path::new(path));
    assert_eq!(
        owner("C:/work/monorepo/packages/app/src/main.ts"),
        Some("package")
    );
    assert_eq!(owner("C:/work/monorepo/README.md"), Some("monorepo"));
    assert_eq!(owner("C:/work/other/lib.rs"), Some("other"));
    assert_eq!(owner("C:/elsewhere/file.txt"), None);
}

#[test]
fn per_project_events_carry_the_root_as_the_frontend_spelled_it() {
    let payload = ProjectRootPayload {
        root: "C:/work/app".to_string(),
    };
    assert_eq!(
        serde_json::to_string(&payload).expect("serialize payload"),
        r#"{"root":"C:/work/app"}"#
    );
}

#[test]
fn a_sibling_with_a_shared_name_prefix_is_not_contained() {
    let candidates = [(Path::new("C:/work/app"), "app")];
    assert_eq!(
        deepest_containing(candidates, Path::new("C:/work/app-old/file.txt")),
        None
    );
}
use crate::test_support::assert_empty;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::Mutex;
use std::time::Duration;

#[test]
fn a_summary_says_what_happened_without_repeating_the_file_name() {
    assert_eq!(summarize(ChangeKind::Created, 0, 0), "New file");
    assert_eq!(summarize(ChangeKind::Deleted, 0, 0), "Deleted");
    assert_eq!(summarize(ChangeKind::Modified, 42, 0), "Grew by 42 lines");
    assert_eq!(summarize(ChangeKind::Modified, 1, 0), "Grew by 1 line");
    assert_eq!(summarize(ChangeKind::Modified, 0, 3), "Trimmed 3 lines");
    assert_eq!(summarize(ChangeKind::Modified, 5, 2), "Edited");
}

#[test]
fn the_first_baseline_wins_and_a_recorded_path_is_never_snapshotted_again() {
    let baselines = Mutex::new(HashMap::new());
    let path = Path::new("/project/src/app.ts");

    record_first_touch_baseline(&baselines, path, || Some("first".to_string()));
    record_first_touch_baseline(&baselines, path, || {
        panic!("a recorded path must not be snapshotted again")
    });

    let recorded = baselines.lock().expect("baselines");
    assert_eq!(recorded.get(path), Some(&Some("first".to_string())));
}

#[test]
fn an_unsnapshottable_first_sighting_is_still_recorded() {
    let baselines = Mutex::new(HashMap::new());
    let path = Path::new("/project/assets/logo.png");
    record_first_touch_baseline(&baselines, path, || None);
    assert_eq!(baselines.lock().expect("baselines").get(path), Some(&None));
}

#[test]
fn an_mcp_config_matches_only_its_exact_root_relative_path() {
    let root = Path::new("/project");
    let config = mcp_config_for_path(root, &root.join(".mcp.json")).expect("root config");
    assert_eq!(config.relative_path, ".mcp.json");
    let nested = mcp_config_for_path(root, &root.join(".cursor").join("mcp.json")).expect("nested");
    assert_eq!(nested.relative_path, ".cursor/mcp.json");
    assert!(mcp_config_for_path(root, &root.join("src").join(".mcp.json")).is_none());
    assert!(mcp_config_for_path(root, Path::new("/elsewhere/.mcp.json")).is_none());
}

#[test]
fn change_kinds_serialize_to_the_lowercase_wire_strings() {
    let kinds = [
        ChangeKind::Created,
        ChangeKind::Modified,
        ChangeKind::Deleted,
    ];
    assert_eq!(
        serde_json::to_string(&kinds).expect("serialize kinds"),
        r#"["created","modified","deleted"]"#
    );
}

#[test]
fn creating_or_deleting_an_empty_mcp_file_changes_membership_state() {
    let missing = crate::workspace::mcp::McpSnapshot::default();
    let empty_file = crate::workspace::mcp::McpSnapshot {
        exists: true,
        ..crate::workspace::mcp::McpSnapshot::default()
    };
    assert!(mcp_membership_changed(&missing, &empty_file));
    assert!(mcp_membership_changed(&empty_file, &missing));
}

#[test]
fn editing_only_an_mcp_definition_does_not_change_membership_state() {
    let previous = crate::workspace::mcp::McpSnapshot {
        exists: true,
        names: ["github".to_string()].into_iter().collect(),
        content: "old".to_string(),
    };
    let current = crate::workspace::mcp::McpSnapshot {
        content: "new".to_string(),
        ..previous.clone()
    };
    assert!(!mcp_membership_changed(&previous, &current));
}

#[test]
fn a_created_or_modified_file_that_vanished_is_not_surfaced() {
    // An atomic-write scratch file is renamed away before it can be diffed.
    assert!(!surfaces(ChangeKind::Created, false));
    assert!(!surfaces(ChangeKind::Modified, false));
}

#[test]
fn a_created_or_modified_file_that_still_exists_is_surfaced() {
    assert!(surfaces(ChangeKind::Created, true));
    assert!(surfaces(ChangeKind::Modified, true));
}

#[test]
fn a_deletion_always_surfaces_since_the_file_being_gone_is_the_event() {
    assert!(surfaces(ChangeKind::Deleted, false));
}

#[test]
fn a_deleted_path_that_exists_again_was_an_atomic_save_and_reads_as_modified() {
    assert!(matches!(
        reclassify(ChangeKind::Deleted, true),
        ChangeKind::Modified
    ));
}

#[test]
fn a_genuinely_gone_path_keeps_its_deleted_kind() {
    assert!(matches!(
        reclassify(ChangeKind::Deleted, false),
        ChangeKind::Deleted
    ));
    assert!(matches!(
        reclassify(ChangeKind::Modified, true),
        ChangeKind::Modified
    ));
    assert!(matches!(
        reclassify(ChangeKind::Created, true),
        ChangeKind::Created
    ));
}

#[test]
fn a_deletion_of_a_tracked_file_removes_every_line_it_had() {
    assert_eq!(line_delta(Some(764), None), (0, 764));
}

#[test]
fn line_deltas_split_growth_shrink_first_sighting_and_untracked_deletion() {
    assert_eq!(line_delta(Some(10), Some(14)), (4, 0));
    assert_eq!(line_delta(Some(14), Some(10)), (0, 4));
    assert_eq!(line_delta(None, Some(9)), (9, 0));
    assert_eq!(line_delta(None, None), (0, 0));
}

/// A scratch directory unique to this test process; removed by the caller.
fn scratch(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("pade-watcher-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("create scratch directory");
    directory
}

#[test]
fn read_preview_text_returns_utf8_file_contents() {
    let directory = scratch("text");
    let file = directory.join("a.txt");
    fs::write(&file, b"line one\nline two\n").expect("write file");
    let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");
    assert_eq!(
        read_preview_text(&canonical_root, &file)
            .expect("read preview")
            .as_deref(),
        Some("line one\nline two\n"),
    );
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn read_preview_text_skips_binary_content() {
    let directory = scratch("binary");
    let file = directory.join("a.bin");
    fs::write(&file, b"before\x00after").expect("write file");
    let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");
    assert!(read_preview_text(&canonical_root, &file).is_err());
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn read_preview_text_skips_files_over_the_cap() {
    let directory = scratch("large");
    let file = directory.join("a.txt");
    let over_capacity = usize::try_from(MAXIMUM_PREVIEW_BYTES).expect("capacity fits usize") + 1;
    fs::write(&file, vec![b'x'; over_capacity]).expect("write file");
    let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");
    assert!(read_preview_text(&canonical_root, &file).is_err());
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn read_preview_text_is_none_for_a_missing_path() {
    let directory = scratch("missing");
    let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");
    assert_eq!(
        read_preview_text(&canonical_root, &directory.join("nope.txt")),
        Ok(None)
    );
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn preview_reads_reject_a_target_outside_the_watch_root() {
    let directory = scratch("authorized-root");
    let outside = scratch("authorized-outside").join("secret.txt");
    fs::write(&outside, b"secret").expect("write outside file");
    let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");

    assert!(read_authorized_file(&canonical_root, &outside, MAXIMUM_PREVIEW_BYTES).is_err());

    let outside_directory = outside.parent().expect("outside file has parent");
    let _ = fs::remove_dir_all(outside_directory);
    let _ = fs::remove_dir_all(&directory);
}

#[cfg(any(unix, windows))]
#[test]
fn preview_reads_reject_a_symlink_or_reparse_point_escape() {
    let directory = scratch("symlink-root");
    let outside_directory = scratch("symlink-outside");
    let outside = outside_directory.join("secret.txt");
    let link = directory.join("escape.txt");
    fs::write(&outside, b"secret").expect("write outside file");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).expect("create symlink");
    #[cfg(windows)]
    if std::os::windows::fs::symlink_file(&outside, &link).is_err() {
        let _ = fs::remove_dir_all(&outside_directory);
        let _ = fs::remove_dir_all(&directory);
        return;
    }

    let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");
    assert!(read_authorized_file(&canonical_root, &link, MAXIMUM_PREVIEW_BYTES).is_err());

    let _ = fs::remove_dir_all(&outside_directory);
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn preview_reads_enforce_the_image_cap_from_the_open_handle() {
    let directory = scratch("image-cap");
    let file = directory.join("large.png");
    let opened = fs::File::create(&file).expect("create image");
    opened
        .set_len(MAXIMUM_IMAGE_BYTES + 1)
        .expect("extend image beyond cap");
    let canonical_root = fs::canonicalize(&directory).expect("canonicalize root");

    assert!(read_authorized_file(&canonical_root, &file, MAXIMUM_IMAGE_BYTES).is_err());
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn line_count_matches_string_lines_and_rejects_invalid_utf8() {
    let directory = scratch("line-count");
    let text = directory.join("source.txt");
    fs::write(&text, b"one\ntwo\n\n").expect("write text file");
    assert_eq!(line_count(&text), Some(3));

    let binary = directory.join("binary.bin");
    fs::write(&binary, b"before\xffafter").expect("write binary file");
    assert_eq!(line_count(&binary), None);
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn resolve_watch_root_accepts_an_existing_directory() {
    let directory = scratch("root");
    let path = directory.to_string_lossy();
    assert!(resolve_watch_root(&path).is_ok_and(|root| root.is_dir()));
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn resolve_watch_root_rejects_a_missing_path() {
    let directory = scratch("root-missing");
    let path = directory.join("nope").to_string_lossy().into_owned();
    assert!(resolve_watch_root(&path).is_err());
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn resolve_watch_root_rejects_a_file() {
    let directory = scratch("root-file");
    let file = directory.join("a.txt");
    fs::write(&file, b"x").expect("write file");
    let path = file.to_string_lossy().into_owned();
    assert!(resolve_watch_root(&path).is_err());
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn a_node_manifest_maps_to_node_modules_and_friends() {
    let directories = manifest_ignore_directories("package.json");
    assert!(directories.contains(&"node_modules"));
    assert!(directories.contains(&"dist"));
    // A lockfile alone implies the same node ignore set.
    assert!(manifest_ignore_directories("pnpm-lock.yaml").contains(&"node_modules"));
}

#[test]
fn a_cargo_manifest_maps_to_target() {
    assert!(manifest_ignore_directories("Cargo.toml").contains(&"target"));
}

#[test]
fn a_dotnet_project_file_maps_to_bin_and_obj_by_extension() {
    let directories = manifest_ignore_directories("MyApp.csproj");
    assert!(directories.contains(&"bin"));
    assert!(directories.contains(&"obj"));
}

#[test]
fn an_unrecognized_file_implies_no_ignore_directories() {
    assert_empty(manifest_ignore_directories("README.md"));
}

#[test]
fn static_ignore_directories_unions_manifest_directories_with_the_baseline() {
    let directory = scratch("static-node");
    fs::write(directory.join("package.json"), b"{}").expect("write manifest");
    let directories = static_ignore_directories(&directory);
    assert!(directories.contains("node_modules"));
    // The baseline is always present, so static mode never weakens the
    // always-on pre-filter.
    assert!(directories.contains(".git"));
    assert!(directories.contains("target"));
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn static_ignore_directories_for_a_bare_folder_is_just_the_baseline() {
    let directory = scratch("static-bare");
    let directories = static_ignore_directories(&directory);
    assert!(directories.contains(".git"));
    assert!(directories.contains("node_modules"));
    // No manifest present, so nothing beyond the baseline is added.
    assert!(!directories.contains(".gradle"));
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn head_and_config_are_the_git_state_files() {
    assert!(is_git_state_file(Path::new(r"C:\repo\.git\HEAD")));
    assert!(is_git_state_file(Path::new(r"C:\repo\.git\config")));
}

#[test]
fn other_git_directory_entries_are_not_git_state_files() {
    assert!(!is_git_state_file(Path::new(r"C:\repo\.git\index")));
    assert!(!is_git_state_file(Path::new(r"C:\repo\.git\ORIG_HEAD")));
    assert!(!is_git_state_file(Path::new(r"C:\repo\.git\config.lock")));
    assert!(!is_git_state_file(Path::new(r"C:\repo\.git\packed-refs")));
}

#[test]
fn only_a_dot_git_entry_announces_the_repository_appearing() {
    assert!(is_git_directory_entry(Path::new(r"C:\repo\.git")));
    assert!(!is_git_directory_entry(Path::new(r"C:\repo\.gitignore")));
    assert!(!is_git_directory_entry(Path::new(r"C:\repo\src")));
}

#[test]
fn git_directory_creation_and_removal_both_rearm_the_detector() {
    let paths = vec![PathBuf::from(r"C:\repo\.git")];
    assert!(git_directory_lifecycle_changed(
        notify::EventKind::Create(notify::event::CreateKind::Any),
        &paths
    ));
    assert!(git_directory_lifecycle_changed(
        notify::EventKind::Remove(notify::event::RemoveKind::Any),
        &paths
    ));
    assert!(!git_directory_lifecycle_changed(
        notify::EventKind::Modify(notify::event::ModifyKind::Any),
        &paths
    ));
}

#[test]
fn unique_directories_trims_dedupes_and_skips_empty_lines() {
    let directories =
        unique_directories([" C:/repo/.git ", "", "C:/repo/.git", "C:/common/.git"].into_iter());
    assert_eq!(
        directories,
        vec![
            PathBuf::from("C:/repo/.git"),
            PathBuf::from("C:/common/.git")
        ]
    );
}

#[test]
fn a_workspace_with_a_git_directory_watches_exactly_that_directory() {
    let directory = scratch("git-state-repo");
    let git_directory = directory.join(".git");
    fs::create_dir_all(&git_directory).expect("create .git directory");
    assert_eq!(git_state_directories(&directory), vec![git_directory]);
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn a_workspace_without_git_yields_no_state_directories() {
    let directory = scratch("git-state-bare");
    assert_empty(&git_state_directories(&directory));
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn a_quiet_channel_returns_the_first_message() {
    let (_sender, receiver) = channel::<GitStateMessage>();
    let burst = drain_burst(
        &receiver,
        GitStateMessage::StateTouched,
        Duration::from_millis(5),
    );
    assert!(burst.state_touched);
    assert!(!burst.rearm);
    assert!(!burst.ignore_touched);
}

/// A busy burst asks for each distinct effect it saw - a `git init` re-arm
/// doesn't swallow a state emit, nor a `.gitignore` refresh.
#[test]
fn a_burst_coalesces_every_distinct_effect() {
    let (sender, receiver) = channel::<GitStateMessage>();
    sender
        .send(GitStateMessage::GitDirectoryChanged)
        .expect("send buffered message");
    sender
        .send(GitStateMessage::IgnoreRulesTouched)
        .expect("send buffered message");
    drop(sender);
    let burst = drain_burst(
        &receiver,
        GitStateMessage::StateTouched,
        Duration::from_millis(5),
    );
    assert!(burst.rearm);
    assert!(burst.state_touched);
    assert!(burst.ignore_touched);
}

#[test]
fn a_path_under_an_ignored_directory_is_excluded_in_static_mode() {
    let mut directories = HashSet::new();
    directories.insert("node_modules".to_string());
    let path = Path::new("project/node_modules/react/index.js");
    assert!(is_under_static_ignore_directory(path, &directories));
}

#[test]
fn a_normal_source_path_is_not_excluded_in_static_mode() {
    let mut directories = HashSet::new();
    directories.insert("node_modules".to_string());
    let path = Path::new("project/src/app.ts");
    assert!(!is_under_static_ignore_directory(path, &directories));
}

fn working_tree_route(root: &Path) -> LanguageRoute {
    let ignore_rules = IgnoreRules::default();
    ignore_rules.install(IgnorePolicy::Static {
        root: root.to_path_buf(),
        directories: static_ignore_directories(root),
        rules: crate::workspace::gitignore::Rules::parse("*.log\n"),
    });
    LanguageRoute::WorkingTree {
        project: root.to_string_lossy().into_owned(),
        git_directories: vec![root.join(".git")],
        ignore_rules: Arc::new(ignore_rules),
    }
}

#[test]
fn a_working_tree_edit_is_language_news_but_ignored_output_is_not() {
    let root = std::env::temp_dir().join(format!("pade-language-news-{}", std::process::id()));
    let route = working_tree_route(&root);
    let news = |relative: &str| language_news(&root, &route, &root.join(relative));

    assert_eq!(news("src/main.rs"), Some(LanguageNews::WorkingTree));
    assert_eq!(news(".git/index"), Some(LanguageNews::TrackedSet));
    assert_eq!(news(".git/objects/ab"), None);
    assert_eq!(news("node_modules/left-pad/index.js"), None);
    assert_eq!(news("debug.log"), None);
}

#[test]
fn a_git_folder_route_reports_only_its_tracked_set_files() {
    let git_directory = std::env::temp_dir().join("pade-language-git").join(".git");
    let route = LanguageRoute::GitDirectory {
        project: "project".to_string(),
    };
    let news = |name: &str| language_news(&git_directory, &route, &git_directory.join(name));

    assert_eq!(news("index"), Some(LanguageNews::TrackedSet));
    assert_eq!(news("HEAD"), Some(LanguageNews::TrackedSet));
    assert_eq!(news("config"), None);
}

#[test]
fn the_feeds_project_needs_only_its_git_folder_and_any_other_a_recursive_watch() {
    let root = std::env::temp_dir().join(format!("pade-language-plan-{}", std::process::id()));
    fs::create_dir_all(root.join(".git")).expect("git dir");
    fs::write(root.join(".git").join("index"), "").expect("index");
    let path = root.to_string_lossy().into_owned();
    let other_project = std::env::temp_dir().join("pade-language-plan-other");
    let feed_roots: HashSet<String> = [&root, &other_project]
        .into_iter()
        .map(|project| crate::workspace::ide::project_key(project))
        .collect();

    let covered = planned_language_routes(&path, &feed_roots, |_| None);
    assert!(matches!(
        covered.as_slice(),
        [(folder, LanguageRoute::GitDirectory { .. })] if *folder == root.join(".git")
    ));

    let existing = working_tree_route(&root);
    let uncovered = planned_language_routes(&path, &HashSet::new(), |_| Some(existing.clone()));
    let [(folder, LanguageRoute::WorkingTree { ignore_rules, .. })] = uncovered.as_slice() else {
        panic!("expected one recursive working-tree watch");
    };
    assert_eq!(*folder, root);
    let LanguageRoute::WorkingTree {
        ignore_rules: existing_rules,
        ..
    } = &existing
    else {
        unreachable!();
    };
    assert!(
        Arc::ptr_eq(ignore_rules, existing_rules),
        "a re-arm keeps the memoized rules"
    );

    fs::remove_dir_all(&root).expect("cleanup");
}
