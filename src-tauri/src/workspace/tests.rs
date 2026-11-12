//! Tests for the workspace modules.

use super::delete::is_deletable_project;
use super::labels::{apply_label, manual_label, LabelSource};
use super::patient_io::retry_through_sharing_race;
use super::preferences::apply_prefs_patch;
use super::recents::MILLISECONDS_PER_DAY;
use super::roots::{
    canonical_dedup, canonical_path, next_segment_completion, shared_leaf_prefix,
    validated_child_path,
};
use super::save::copy_tree_skipping_dependencies;
use super::store::{Prefs, Settings, SettingsRepository};
use super::temp::{expired_temps, is_temp_workspace_in};
use crate::test_support::assert_empty;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TEMP_WORKSPACE: &str = r"C:\Users\avi\AppData\Roaming\pade\workspaces\temp-42";
const NOW: u64 = 1000 * MILLISECONDS_PER_DAY;

fn cleanup_settings(days: Option<u32>, days_since_use: u64) -> Settings {
    let mut settings = Settings::default();
    settings.prefs.temp_cleanup_days = days;
    settings.workspace_last_used.insert(
        TEMP_WORKSPACE.to_string(),
        NOW - days_since_use * MILLISECONDS_PER_DAY,
    );
    settings
}

fn expired(settings: &Settings, open_projects: &[String]) -> Vec<String> {
    expired_temps(settings, &[TEMP_WORKSPACE.to_string()], open_projects, NOW)
}

fn matches_of(paths: &[PathBuf]) -> BTreeMap<String, PathBuf> {
    paths
        .iter()
        .map(|path| (path.to_string_lossy().to_lowercase(), path.clone()))
        .collect()
}

#[test]
fn leaf_prefixes_are_shared_case_insensitively_in_the_first_spelling() {
    assert_eq!(shared_leaf_prefix("Pade", "padel"), "Pade");
    assert_eq!(shared_leaf_prefix("alpha", "beta"), "");
}

#[test]
fn a_single_match_completes_into_the_folder() {
    let parent = Path::new("parent");
    let only = parent.join("repositories");
    assert_eq!(
        next_segment_completion(parent, &matches_of(std::slice::from_ref(&only))),
        format!("{}{}", only.display(), std::path::MAIN_SEPARATOR)
    );
}

#[test]
fn several_matches_complete_to_their_shared_prefix() {
    let parent = Path::new("parent");
    let matches = matches_of(&[
        parent.join("pade"),
        parent.join("padding"),
        parent.join("paddle"),
    ]);
    assert_eq!(
        next_segment_completion(parent, &matches),
        parent.join("pad").to_string_lossy()
    );
}

#[test]
fn no_match_completes_nothing() {
    assert_eq!(
        next_segment_completion(Path::new("parent"), &BTreeMap::new()),
        ""
    );
}

#[test]
fn a_temp_unused_past_the_window_expires() {
    assert_eq!(
        expired(&cleanup_settings(Some(30), 31), &[]),
        [TEMP_WORKSPACE]
    );
}

#[test]
fn a_temp_used_within_the_window_is_kept() {
    assert_empty(&expired(&cleanup_settings(Some(30), 29), &[]));
}

#[test]
fn cleanup_off_keeps_every_temp() {
    assert_empty(&expired(&cleanup_settings(None, 365), &[]));
}

#[test]
fn a_temp_never_seen_used_is_kept() {
    let mut settings = cleanup_settings(Some(1), 0);
    settings.workspace_last_used.clear();
    assert_empty(&expired(&settings, &[]));
}

#[test]
fn an_open_or_pinned_temp_is_kept_however_old() {
    let open = [TEMP_WORKSPACE.to_string()];
    assert_empty(&expired(&cleanup_settings(Some(1), 365), &open));

    let mut pinned = cleanup_settings(Some(1), 365);
    pinned.pinned_projects.push(TEMP_WORKSPACE.to_string());
    assert_empty(&expired(&pinned, &[]));
}

#[test]
fn a_manual_label_keeps_what_the_user_typed() {
    assert_eq!(
        manual_label("  Hebrew voice - take 2 ").as_deref(),
        Some("Hebrew voice - take 2")
    );
    assert_eq!(manual_label("קול עברי").as_deref(), Some("קול עברי"));
}

#[test]
fn a_manual_label_refuses_blank_multiline_and_overlong_text() {
    assert_eq!(manual_label("   "), None);
    assert_eq!(manual_label("first line\nsecond line"), None);
    assert_eq!(manual_label(&"a".repeat(61)), None);
    assert_eq!(manual_label(&"א".repeat(60)), Some("א".repeat(60)));
}

#[test]
fn an_auto_name_never_replaces_an_existing_label() {
    let mut labels = BTreeMap::from([(TEMP_WORKSPACE.to_string(), "my-name".to_string())]);

    apply_label(
        &mut labels,
        TEMP_WORKSPACE.to_string(),
        "brave-otter".to_string(),
        LabelSource::Auto,
    );

    assert_eq!(labels[TEMP_WORKSPACE], "my-name");
}

#[test]
fn an_auto_name_labels_an_unlabelled_workspace() {
    let mut labels = BTreeMap::new();

    apply_label(
        &mut labels,
        TEMP_WORKSPACE.to_string(),
        "brave-otter".to_string(),
        LabelSource::Auto,
    );

    assert_eq!(labels[TEMP_WORKSPACE], "brave-otter");
}

#[test]
fn a_manual_label_replaces_an_auto_name() {
    let mut labels = BTreeMap::from([(TEMP_WORKSPACE.to_string(), "brave-otter".to_string())]);

    apply_label(
        &mut labels,
        TEMP_WORKSPACE.to_string(),
        "Voice experiments".to_string(),
        LabelSource::Manual,
    );

    assert_eq!(labels[TEMP_WORKSPACE], "Voice experiments");
}

#[cfg(windows)]
#[test]
fn folds_doubled_and_forward_separators_to_one_spelling() {
    let canonical = canonical_path(r"C:\repositories\avi\sb-companion");
    assert_eq!(
        canonical_path(r"C:\\repositories\\avi\\sb-companion"),
        canonical
    );
    assert_eq!(
        canonical_path("C:/repositories/avi/sb-companion"),
        canonical
    );
    assert_eq!(
        canonical_path(r"C:\repositories\avi\sb-companion\"),
        canonical
    );
}

#[test]
fn dedup_collapses_the_same_folder_spelled_two_ways() {
    let deduped = canonical_dedup(&[
        r"C:\repositories\avi\sb-companion".to_string(),
        r"C:\\repositories\\avi\\sb-companion".to_string(),
        r"C:\repositories\avi\pade".to_string(),
    ]);
    // The doubled-backslash duplicate folds away; first-seen order is kept.
    assert_eq!(
        deduped,
        vec![
            canonical_path(r"C:\repositories\avi\sb-companion"),
            canonical_path(r"C:\repositories\avi\pade"),
        ]
    );
}

#[test]
fn child_paths_reject_traversal_and_absolute_names() {
    let root = Path::new("root");
    for name in [
        "../outside",
        "..\\outside",
        "/outside",
        "C:\\outside",
        ".",
        "",
    ] {
        assert!(validated_child_path(root, name).is_err(), "accepted {name}");
    }
    assert_eq!(
        validated_child_path(root, "project").expect("valid child"),
        root.join("project")
    );
}

#[cfg(windows)]
#[test]
fn child_paths_reject_windows_device_names_but_not_lookalikes() {
    let root = Path::new("root");
    for name in [
        "con",
        "Nul.txt",
        "COM1",
        "lpt9.log",
        "name:stream",
        "trailing.",
    ] {
        assert!(validated_child_path(root, name).is_err(), "accepted {name}");
    }
    for name in ["COM0", "COM10", "console", "LPT", "auxiliary"] {
        assert!(validated_child_path(root, name).is_ok(), "refused {name}");
    }
}

#[test]
fn only_a_temp_named_folder_directly_inside_the_workspaces_directory_is_temp() {
    let scratch = std::env::temp_dir().join(format!("pade-temp-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let workspaces = scratch.join("workspaces");
    for folder in ["temp-1", "project", "temp-2/temp-3"] {
        std::fs::create_dir_all(workspaces.join(folder)).expect("create folder");
    }
    std::fs::create_dir_all(scratch.join("elsewhere").join("temp-4")).expect("create folder");
    let canonical = std::fs::canonicalize(&workspaces).expect("canonical workspaces");
    let is_temp = |relative: &str| {
        is_temp_workspace_in(&scratch.join(relative).to_string_lossy(), &canonical)
    };

    assert!(is_temp("workspaces/temp-1"));
    assert!(!is_temp("workspaces/project"));
    assert!(!is_temp("workspaces/temp-2/temp-3"));
    assert!(!is_temp("elsewhere/temp-4"));
    assert!(!is_temp("workspaces/temp-missing"));
    std::fs::remove_dir_all(scratch).expect("scratch cleanup");
}

#[test]
fn a_project_scanned_under_a_root_can_be_deleted_from_the_home_page() {
    let base = std::env::temp_dir().join(format!("pade-delete-home-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("roots");
    let project = root.join("project");
    std::fs::create_dir_all(&project).expect("create project");
    std::fs::write(project.join("package.json"), "{}").expect("write manifest");
    let mut settings = Settings::default();
    settings.roots.push(root.to_string_lossy().into_owned());
    let canonical = std::fs::canonicalize(&project).expect("canonical project");

    assert!(is_deletable_project(
        &settings,
        &project.to_string_lossy(),
        &canonical
    ));

    std::fs::remove_dir_all(&base).expect("clean delete fixture");
}

#[test]
fn a_directory_the_home_page_never_shows_cannot_be_deleted() {
    let base = std::env::temp_dir().join(format!("pade-delete-unlisted-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("roots");
    let plain_folder = root.join("plain-folder");
    let outside = base.join("outside");
    std::fs::create_dir_all(&plain_folder).expect("create plain folder");
    std::fs::create_dir_all(&outside).expect("create outside folder");
    std::fs::write(outside.join("package.json"), "{}").expect("write manifest");
    let mut settings = Settings::default();
    settings.roots.push(root.to_string_lossy().into_owned());

    // A folder under a root without a project marker is never listed.
    let canonical_plain = std::fs::canonicalize(&plain_folder).expect("canonical plain");
    assert!(!is_deletable_project(
        &settings,
        &plain_folder.to_string_lossy(),
        &canonical_plain
    ));
    // A real project outside every root and list is never listed either.
    let canonical_outside = std::fs::canonicalize(&outside).expect("canonical outside");
    assert!(!is_deletable_project(
        &settings,
        &outside.to_string_lossy(),
        &canonical_outside
    ));
    // The root itself is a container, not one of its projects.
    let canonical_root = std::fs::canonicalize(&root).expect("canonical root");
    assert!(!is_deletable_project(
        &settings,
        &root.to_string_lossy(),
        &canonical_root
    ));

    std::fs::remove_dir_all(&base).expect("clean unlisted fixture");
}

#[test]
fn a_remembered_project_can_be_deleted_even_outside_every_root() {
    let base = std::env::temp_dir().join(format!("pade-delete-recent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let project = base.join("project");
    std::fs::create_dir_all(&project).expect("create project");
    let mut settings = Settings::default();
    settings
        .recent_projects
        .push(project.to_string_lossy().into_owned());
    let canonical = std::fs::canonicalize(&project).expect("canonical project");

    assert!(is_deletable_project(
        &settings,
        &project.to_string_lossy(),
        &canonical
    ));

    std::fs::remove_dir_all(&base).expect("clean recent fixture");
}

#[test]
fn the_sharing_race_retry_stops_at_the_first_success() {
    let mut calls = 0;
    let outcome = retry_through_sharing_race(|| {
        calls += 1;
        if calls < 3 {
            return Err(std::io::Error::other("still held"));
        }
        Ok(())
    });
    assert!(outcome.is_ok());
    assert_eq!(calls, 3);
}

#[test]
fn preference_patch_removes_a_null_optional_key() {
    let mut prefs = Prefs {
        passthrough: BTreeMap::from([(
            "themeMode".to_string(),
            serde_json::Value::String("dark".to_string()),
        )]),
        ..Prefs::default()
    };
    let patch = serde_json::Map::from_iter([("themeMode".to_string(), serde_json::Value::Null)]);

    apply_prefs_patch(&mut prefs, patch).expect("apply patch");

    assert!(!prefs.passthrough.contains_key("themeMode"));
}

#[test]
fn copy_skips_dependency_directories_and_keeps_the_rest() {
    let base = std::env::temp_dir().join(format!("pade-migrate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let from = base.join("temp-source");
    let to = base.join("saved");
    std::fs::create_dir_all(from.join("node_modules")).expect("create node_modules");
    std::fs::write(
        from.join("node_modules").join("left-pad.js"),
        "module.exports",
    )
    .expect("write dependency file");
    std::fs::create_dir_all(from.join("src")).expect("create src");
    std::fs::write(from.join("src").join("main.rs"), "fn main() {}").expect("write source");
    std::fs::create_dir_all(from.join(".git")).expect("create .git");
    std::fs::write(from.join(".git").join("HEAD"), "ref: refs/heads/main").expect("write git head");
    std::fs::write(from.join("package.json"), "{}").expect("write manifest");

    copy_tree_skipping_dependencies(&from, &to).expect("copy tree");

    assert!(
        !to.join("node_modules").exists(),
        "dependency dir was copied"
    );
    assert!(
        to.join("src").join("main.rs").is_file(),
        "source not copied"
    );
    assert!(to.join(".git").join("HEAD").is_file(), ".git not copied");
    assert!(to.join("package.json").is_file(), "manifest not copied");

    std::fs::remove_dir_all(&base).expect("clean migrate fixture");
}

#[test]
fn copy_of_an_empty_workspace_creates_the_destination_and_copies_nothing() {
    // Saving a workspace the user never prompted in is the same code path, not
    // a special case: an empty source copies nothing but still yields a valid
    // (empty) destination - no error, no missing folder.
    let base = std::env::temp_dir().join(format!("pade-migrate-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let from = base.join("temp-source");
    let to = base.join("saved");
    std::fs::create_dir_all(&from).expect("create empty source");

    copy_tree_skipping_dependencies(&from, &to).expect("copy empty tree");

    assert!(to.is_dir(), "destination not created for an empty source");
    assert_eq!(
        std::fs::read_dir(&to).expect("read destination").count(),
        0,
        "destination should be empty"
    );

    std::fs::remove_dir_all(&base).expect("clean empty migrate fixture");
}

#[test]
fn a_mutation_that_moves_nothing_reports_no_change() {
    let path = std::env::temp_dir().join(format!(
        "pade-settings-no-change-{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let repository = SettingsRepository {
        lock: std::sync::Mutex::new(()),
        path: Some(path.clone()),
    };

    let added = repository
        .update(|settings| {
            settings.roots.push("root".to_string());
            Ok(())
        })
        .expect("add a root");
    assert!(added.written.is_some(), "adding a root moves the settings");

    let pruned = repository.update(|_| Ok(())).expect("prune nothing");
    assert!(
        pruned.written.is_none(),
        "a prune with nothing to forget must stay silent, or two windows \
         refreshing off each other's announcements would never settle"
    );
    assert_eq!(pruned.settings.roots, ["root"]);

    std::fs::remove_file(path).expect("clean no-change fixture");
}

#[test]
fn repository_serializes_concurrent_updates_without_losing_fields() {
    let path = std::env::temp_dir().join(format!(
        "pade-settings-repository-{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let repository = Arc::new(SettingsRepository {
        lock: std::sync::Mutex::new(()),
        path: Some(path.clone()),
    });
    let roots_repository = Arc::clone(&repository);
    let roots = std::thread::spawn(move || {
        roots_repository
            .update(|settings| {
                settings.roots.push("root".to_string());
                Ok(())
            })
            .expect("update roots");
    });
    let pins_repository = Arc::clone(&repository);
    let pins = std::thread::spawn(move || {
        pins_repository
            .update(|settings| {
                settings.pinned_projects.push("project".to_string());
                Ok(())
            })
            .expect("update pins");
    });

    roots.join().expect("roots thread");
    pins.join().expect("pins thread");
    let settings = repository.load();
    assert_eq!(settings.roots, ["root"]);
    assert_eq!(settings.pinned_projects, ["project"]);
    std::fs::remove_file(path).expect("clean settings fixture");
}
