//! Tests for the IDE modules.

use super::census::{
    census, git_linguist_attributes, source_content, SourceFileEvidence, SourceProfile,
};
use super::declarations::{project_declarations, required_project_kinds, ProjectDeclaration};
use super::editor_registry::{EditorCoverage, OpenStyle, ProjectKind, REGISTRY};
use super::editors::{editor_family, executable_basename};
use super::kinds::{ide_kinds, primary_declared_kind};
use super::languages::{
    cached_language_shares, language_shares, note_working_tree_change, project_key,
};
use super::open::{open_arguments, open_style};
use super::suggest::{
    editor_covers_project, preference_chain, ranked_editor_ids, suggestible_editor_ids,
};
use crate::test_support::assert_not_empty;

fn is_general_purpose_editor(id: &str) -> bool {
    REGISTRY
        .iter()
        .find(|editor| editor.id == id)
        .is_some_and(|editor| matches!(editor.coverage, EditorCoverage::EveryKind))
}

fn source_file(path: &str, kind: ProjectKind, bytes: u64) -> SourceFileEvidence {
    SourceFileEvidence {
        path: std::path::PathBuf::from(path),
        kind,
        language: kind.as_str(),
        bytes,
    }
}

#[test]
fn an_explicit_project_choice_leads_even_without_coverage() {
    let chain = preference_chain(
        Some("webstorm".to_string()),
        Some("idea".to_string()),
        Some("vscode".to_string()),
        |id| id != "webstorm",
    );
    assert_eq!(chain, ["webstorm", "idea", "vscode"]);
}

#[test]
fn rule_and_fallback_apply_only_while_they_cover_the_project() {
    let chain = preference_chain(
        None,
        Some("webstorm".to_string()),
        Some("vscode".to_string()),
        |id| id == "vscode",
    );
    assert_eq!(chain, ["vscode"]);
}

#[test]
fn coverage_ranking_prefers_a_specialist_for_one_language() {
    let source_bytes = std::collections::BTreeMap::from([(ProjectKind::Rust, 10_000)]);
    let ranked = ranked_editor_ids(&source_bytes, &[ProjectKind::Rust]);
    assert_eq!(ranked.first().map(String::as_str), Some("rustrover"));
}

#[test]
fn coverage_ranking_requires_full_coverage_for_a_hybrid() {
    let source_bytes =
        std::collections::BTreeMap::from([(ProjectKind::Web, 9_000), (ProjectKind::Rust, 1_000)]);
    let ranked = ranked_editor_ids(&source_bytes, &[ProjectKind::Web, ProjectKind::Rust]);
    assert!(ranked
        .first()
        .is_some_and(|id| is_general_purpose_editor(id)));
    assert!(
        ranked
            .iter()
            .position(|id| is_general_purpose_editor(id))
            .unwrap_or(usize::MAX)
            < ranked
                .iter()
                .position(|id| id == "webstorm")
                .unwrap_or(usize::MAX)
    );
}

#[test]
fn android_studio_covers_android_but_not_plain_java() {
    let java_sources = std::collections::BTreeMap::from([(ProjectKind::Java, 10_000)]);
    assert!(!editor_covers_project(
        "androidstudio",
        &java_sources,
        &[ProjectKind::Java]
    ));
    assert!(editor_covers_project(
        "androidstudio",
        &java_sources,
        &[ProjectKind::Android]
    ));
    assert!(editor_covers_project(
        "idea",
        &java_sources,
        &[ProjectKind::Java]
    ));
}

#[test]
fn a_web_rule_requires_complete_language_coverage() {
    let web_monorepo = std::collections::BTreeMap::from([(ProjectKind::Web, 10_000)]);
    assert!(editor_covers_project(
        "webstorm",
        &web_monorepo,
        &[ProjectKind::Web]
    ));

    let web_rust_project =
        std::collections::BTreeMap::from([(ProjectKind::Web, 9_000), (ProjectKind::Rust, 1_000)]);
    assert!(!editor_covers_project(
        "webstorm",
        &web_rust_project,
        &[ProjectKind::Web, ProjectKind::Rust]
    ));
    assert!(editor_covers_project(
        "vscode",
        &web_rust_project,
        &[ProjectKind::Web, ProjectKind::Rust]
    ));
}

#[test]
fn suggestions_omit_specialists_that_cannot_cover_the_project() {
    let web_rust_project =
        std::collections::BTreeMap::from([(ProjectKind::Web, 9_000), (ProjectKind::Rust, 1_000)]);
    let suggested =
        suggestible_editor_ids(&web_rust_project, &[ProjectKind::Web, ProjectKind::Rust]);
    assert!(suggested
        .first()
        .is_some_and(|id| is_general_purpose_editor(id)));
    assert!(!suggested.iter().any(|id| id == "webstorm"));
    assert!(!suggested.iter().any(|id| id == "androidstudio"));
}

#[test]
fn declared_project_kinds_ignore_ancillary_source_languages() {
    let cases = [
        (
            ProjectKind::Android,
            ProjectKind::Java,
            ProjectKind::Web,
            "androidstudio",
        ),
        (
            ProjectKind::Web,
            ProjectKind::Web,
            ProjectKind::Python,
            "webstorm",
        ),
        (
            ProjectKind::Python,
            ProjectKind::Python,
            ProjectKind::Rust,
            "pycharm",
        ),
        (
            ProjectKind::Php,
            ProjectKind::Php,
            ProjectKind::Rust,
            "phpstorm",
        ),
        (
            ProjectKind::Ruby,
            ProjectKind::Ruby,
            ProjectKind::Rust,
            "rubymine",
        ),
        (
            ProjectKind::Go,
            ProjectKind::Go,
            ProjectKind::Rust,
            "goland",
        ),
        (
            ProjectKind::Rust,
            ProjectKind::Rust,
            ProjectKind::Web,
            "rustrover",
        ),
        (
            ProjectKind::Java,
            ProjectKind::Java,
            ProjectKind::Rust,
            "idea",
        ),
        (
            ProjectKind::Cpp,
            ProjectKind::Cpp,
            ProjectKind::Python,
            "clion",
        ),
        (
            ProjectKind::Dotnet,
            ProjectKind::Dotnet,
            ProjectKind::Python,
            "rider",
        ),
    ];

    for (declared, primary_source, ancillary_source, expected) in cases {
        let source_bytes = std::collections::BTreeMap::from([
            (primary_source, 20_000),
            (ancillary_source, 30_000),
        ]);
        let suggested = suggestible_editor_ids(&source_bytes, &[declared]);

        assert_eq!(
            suggested.first().map(String::as_str),
            Some(expected),
            "declared kind: {declared:?}"
        );
        assert!(suggested.iter().any(|id| is_general_purpose_editor(id)));
    }
}

#[test]
fn a_declared_project_requires_every_language_in_its_main_source_branch() {
    let mut profile = SourceProfile::default();
    profile.record(source_file("project/src/app.ts", ProjectKind::Web, 10_000));
    profile.record(source_file(
        "project/src/native.rs",
        ProjectKind::Rust,
        2_000,
    ));
    profile.record(source_file(
        "project/scripts/automate.py",
        ProjectKind::Python,
        20_000,
    ));
    let declarations = [ProjectDeclaration {
        root: std::path::PathBuf::from("project"),
        kinds: vec![ProjectKind::Web],
    }];

    assert_eq!(
        required_project_kinds(&profile, &declarations),
        vec![ProjectKind::Web, ProjectKind::Rust]
    );
}

fn declaration(root: &str, kinds: &[ProjectKind]) -> ProjectDeclaration {
    ProjectDeclaration {
        root: std::path::PathBuf::from(root),
        kinds: kinds.to_vec(),
    }
}

#[test]
fn language_shares_drop_slivers_and_fill_the_bar_biggest_first() {
    let totals = std::collections::BTreeMap::from([
        ("Python", 310),
        ("TypeScript", 300),
        ("Svelte", 180),
        ("C#", 200),
        ("Rust", 10),
    ]);
    let shares: Vec<(String, String, u64)> = language_shares(&totals)
        .into_iter()
        .map(|share| (share.language, share.kind, share.per_mille))
        .collect();
    assert_eq!(
        shares,
        [
            ("Python".to_string(), "python".to_string(), 313),
            ("TypeScript".to_string(), "web".to_string(), 303),
            ("C#".to_string(), "dotnet".to_string(), 202),
            ("Svelte".to_string(), "web".to_string(), 181)
        ]
    );
    assert_eq!(language_shares(&std::collections::BTreeMap::new()), []);
}

#[test]
fn the_kind_most_project_roots_declare_is_the_primary_one() {
    let declarations = [
        declaration("app", &[ProjectKind::Dotnet]),
        declaration("lib", &[ProjectKind::Dotnet]),
        declaration("service", &[ProjectKind::Dotnet]),
        declaration("downloader-rs", &[ProjectKind::Rust]),
        declaration("updater-rs", &[ProjectKind::Rust]),
    ];
    assert_eq!(
        primary_declared_kind(&declarations),
        Some(ProjectKind::Dotnet)
    );
}

#[test]
fn an_android_app_is_not_outvoted_by_its_own_gradle_modules() {
    let declarations = [
        declaration("", &[ProjectKind::Android, ProjectKind::Java]),
        declaration("app", &[ProjectKind::Java]),
    ];
    assert_eq!(
        primary_declared_kind(&declarations),
        Some(ProjectKind::Android)
    );
}

#[test]
fn a_tie_between_declared_kinds_goes_to_registry_order() {
    let declarations = [
        declaration("", &[ProjectKind::Web]),
        declaration("src-tauri", &[ProjectKind::Rust]),
    ];
    assert_eq!(primary_declared_kind(&declarations), Some(ProjectKind::Web));
    assert_eq!(primary_declared_kind(&[]), None);
}

#[test]
fn a_browser_manifest_declares_web_without_a_package_manifest() {
    let directory =
        std::env::temp_dir().join(format!("pade-browser-manifest-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("test dir");
    std::fs::write(directory.join("manifest.json"), "{\"manifest_version\":3}")
        .expect("browser manifest");

    let declarations = project_declarations(&directory);
    assert!(declarations
        .iter()
        .any(|declaration| declaration.kinds.contains(&ProjectKind::Web)));

    std::fs::remove_dir_all(&directory).expect("cleanup");
}

#[test]
fn an_unrelated_json_manifest_does_not_declare_web() {
    let directory =
        std::env::temp_dir().join(format!("pade-generic-manifest-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("test dir");
    std::fs::write(directory.join("manifest.json"), "{\"name\":\"artifact\"}")
        .expect("generic manifest");

    assert!(!project_declarations(&directory)
        .iter()
        .any(|declaration| declaration.kinds.contains(&ProjectKind::Web)));

    std::fs::remove_dir_all(&directory).expect("cleanup");
}

#[test]
fn unmarked_mixed_source_still_requires_a_generalist() {
    let source_bytes =
        std::collections::BTreeMap::from([(ProjectKind::Web, 9_000), (ProjectKind::Rust, 1_000)]);
    let suggested = suggestible_editor_ids(&source_bytes, &[]);

    assert!(suggested
        .first()
        .is_some_and(|id| is_general_purpose_editor(id)));
    assert!(!suggested.iter().any(|id| id == "webstorm"));
    assert!(!suggested.iter().any(|id| id == "rustrover"));
}

#[test]
fn an_unclassified_project_only_suggests_general_purpose_editors() {
    let suggested = suggestible_editor_ids(&std::collections::BTreeMap::new(), &[]);

    assert_not_empty(&suggested);
    assert!(suggested.iter().all(|id| is_general_purpose_editor(id)));
}

#[test]
fn generated_content_sits_out_the_census_without_a_path_rule() {
    let directory = std::env::temp_dir().join(format!("pade-generated-{}", std::process::id()));
    let core = directory.join("src-tauri");
    std::fs::create_dir_all(&core).expect("test dirs");

    let web_source = "const value = 1;\n".repeat(200);
    std::fs::write(directory.join("app.ts"), &web_source).expect("web file");
    let rust_source = "fn main() {}\n".repeat(260);
    std::fs::write(core.join("main.rs"), &rust_source).expect("rust file");
    std::fs::write(directory.join("bundle.unknown"), vec![b'a'; 4_000_000]).expect("bundle");

    assert!(
        source_content(&directory.join("bundle.unknown"))
            .expect("bundle readable")
            .generated
    );
    let totals = census(&directory);
    assert_eq!(
        totals.get(&ProjectKind::Web),
        Some(&(web_source.len() as u64))
    );
    assert_eq!(
        totals.get(&ProjectKind::Rust),
        Some(&(rust_source.len() as u64))
    );

    std::fs::remove_dir_all(&directory).expect("cleanup");
}

#[test]
fn the_census_honors_git_attributes_and_ignores_ignored_junk() {
    let directory = std::env::temp_dir().join(format!("pade-gittracked-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("test dir");

    let git = |arguments: &[&str]| {
        crate::util::command("git")
            .arg("-C")
            .arg(&directory)
            .args(arguments)
            .output()
    };
    // A machine without git skips this rather than failing the suite.
    let Ok(init) = git(&["init", "-q"]) else {
        std::fs::remove_dir_all(&directory).ok();
        return;
    };
    if !init.status.success() {
        std::fs::remove_dir_all(&directory).ok();
        return;
    }

    let web_source = "const value = 1;\n".repeat(400);
    let overridden_source = "m".repeat(20_000);
    let rust_source = "fn main() {}\n".repeat(500);
    std::fs::write(directory.join("app.ts"), &web_source).expect("web file");
    std::fs::write(directory.join("custom.source"), &rust_source).expect("rust override");
    std::fs::write(directory.join("generated.ts"), "x".repeat(20_000)).expect("generated file");
    std::fs::write(directory.join("manual.ts"), &overridden_source).expect("manual file");
    std::fs::write(
        directory.join(".gitattributes"),
        "generated.ts linguist-generated\ncustom.source linguist-language=Rust\nmanual.ts -linguist-generated\n",
    )
    .expect("attributes");
    std::fs::write(directory.join(".gitignore"), "bundle.js\n").expect("ignore rules");
    let add = git(&["add", "."]).expect("git add");
    if !add.status.success() {
        std::fs::remove_dir_all(&directory).ok();
        return;
    }

    std::fs::write(directory.join("bundle.js"), "x = 1;\n".repeat(120_000)).expect("ignored blob");
    let dependencies = directory.join("node_modules").join("left-pad");
    std::fs::create_dir_all(&dependencies).expect("dependency dir");
    std::fs::write(dependencies.join("index.js"), "x = 1;\n".repeat(120_000))
        .expect("unignored dependency");
    let totals = census(&directory);
    assert_eq!(
        totals.get(&ProjectKind::Web),
        Some(&((web_source.len() + overridden_source.len()) as u64))
    );
    assert_eq!(
        totals.get(&ProjectKind::Rust),
        Some(&(rust_source.len() as u64))
    );

    std::fs::remove_dir_all(&directory).expect("cleanup");
}

#[test]
fn the_census_counts_an_untracked_file_nobody_has_staged_yet() {
    let directory = std::env::temp_dir().join(format!("pade-untracked-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("test dir");
    let git = |arguments: &[&str]| {
        crate::util::command("git")
            .arg("-C")
            .arg(&directory)
            .args(arguments)
            .output()
    };
    let Ok(init) = git(&["init", "-q"]) else {
        std::fs::remove_dir_all(&directory).ok();
        return;
    };
    if !init.status.success() {
        std::fs::remove_dir_all(&directory).ok();
        return;
    }

    let web_source = "const value = 1;\n".repeat(100);
    std::fs::write(directory.join("app.ts"), &web_source).expect("web file");
    let add = git(&["add", "."]).expect("git add");
    if !add.status.success() {
        std::fs::remove_dir_all(&directory).ok();
        return;
    }
    let rust_source = "fn main() {}\n".repeat(100);
    std::fs::write(directory.join("written-by-an-agent.rs"), &rust_source)
        .expect("untracked rust file");

    let totals = census(&directory);
    assert_eq!(
        totals.get(&ProjectKind::Web),
        Some(&(web_source.len() as u64))
    );
    assert_eq!(
        totals.get(&ProjectKind::Rust),
        Some(&(rust_source.len() as u64))
    );

    std::fs::remove_dir_all(&directory).expect("cleanup");
}

#[test]
fn attributes_for_more_paths_than_a_pipe_holds_come_back_without_a_deadlock() {
    let directory = std::env::temp_dir().join(format!("pade-check-attr-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("test dir");
    let Ok(init) = crate::util::command("git")
        .arg("-C")
        .arg(&directory)
        .args(["init", "-q"])
        .output()
    else {
        std::fs::remove_dir_all(&directory).ok();
        return;
    };
    if !init.status.success() {
        std::fs::remove_dir_all(&directory).ok();
        return;
    }
    std::fs::write(
        directory.join(".gitattributes"),
        "*.js linguist-generated\n",
    )
    .expect("attributes file");

    let paths: Vec<String> = (0..5000).map(|i| format!("src/file-{i}.js")).collect();
    let attributes = git_linguist_attributes(&directory, &paths);
    assert_eq!(attributes.len(), paths.len());
    assert_eq!(attributes[&paths[0]].generated, Some(true));

    std::fs::remove_dir_all(&directory).expect("cleanup");
}

#[test]
fn a_reported_working_tree_change_recounts_a_project_whose_index_never_moved() {
    let directory =
        std::env::temp_dir().join(format!("pade-language-cache-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("test dir");
    let git = |arguments: &[&str]| {
        crate::util::command("git")
            .arg("-C")
            .arg(&directory)
            .args(arguments)
            .output()
    };
    let Ok(init) = git(&["init", "-q"]) else {
        std::fs::remove_dir_all(&directory).ok();
        return;
    };
    if !init.status.success() {
        std::fs::remove_dir_all(&directory).ok();
        return;
    }
    std::fs::write(directory.join("app.ts"), "const value = 1;\n".repeat(100)).expect("web file");
    let add = git(&["add", "."]).expect("git add");
    if !add.status.success() {
        std::fs::remove_dir_all(&directory).ok();
        return;
    }
    let path = directory.to_string_lossy().into_owned();
    let languages = |shares: Vec<super::languages::LanguageShare>| -> Vec<String> {
        shares.into_iter().map(|share| share.language).collect()
    };
    assert_eq!(languages(cached_language_shares(&path)), ["TypeScript"]);

    std::fs::write(directory.join("main.rs"), "fn main() {}\n".repeat(100))
        .expect("untracked rust file");
    assert_eq!(
        languages(cached_language_shares(&path)),
        ["TypeScript"],
        "an unreported edit is served from the cache, one stat per row"
    );

    note_working_tree_change(&directory.join("."));
    assert_eq!(
        languages(cached_language_shares(&path)),
        ["TypeScript", "Rust"]
    );

    std::fs::remove_dir_all(&directory).expect("cleanup");
}

#[test]
fn a_project_key_ignores_how_the_path_was_spelled() {
    let project = std::env::temp_dir().join("pade-key");
    let trailing = format!("{}{}", project.display(), std::path::MAIN_SEPARATOR);
    assert_eq!(
        project_key(&project),
        project_key(std::path::Path::new(&trailing))
    );
}

#[test]
fn a_nested_project_census_does_not_expand_to_its_git_root() {
    let directory = std::env::temp_dir().join(format!("pade-git-scope-{}", std::process::id()));
    let nested = directory.join("core");
    std::fs::create_dir_all(&nested).expect("test dirs");

    let git = |arguments: &[&str]| {
        crate::util::command("git")
            .arg("-C")
            .arg(&directory)
            .args(arguments)
            .output()
    };
    let Ok(init) = git(&["init", "-q"]) else {
        std::fs::remove_dir_all(&directory).ok();
        return;
    };
    if !init.status.success() {
        std::fs::remove_dir_all(&directory).ok();
        return;
    }

    std::fs::write(directory.join("app.ts"), "const app = 1;\n").expect("web source");
    let rust_source = "fn main() {}\n";
    std::fs::write(nested.join("main.rs"), rust_source).expect("rust source");
    let add = git(&["add", "."]).expect("git add");
    if !add.status.success() {
        std::fs::remove_dir_all(&directory).ok();
        return;
    }

    let totals = census(&nested);
    assert_eq!(
        totals.get(&ProjectKind::Rust),
        Some(&(rust_source.len() as u64))
    );
    assert!(!totals.contains_key(&ProjectKind::Web));

    std::fs::remove_dir_all(&directory).expect("cleanup");
}

#[test]
fn executable_basename_strips_extension_and_lowercases() {
    assert_eq!(executable_basename("C:\\Program Files\\Code.exe"), "code");
    assert_eq!(executable_basename("/usr/bin/nvim"), "nvim");
    assert_eq!(executable_basename("notepad++.exe"), "notepad++");
}

#[test]
fn family_maps_a_jetbrains_editor_to_its_open_style() {
    let webstorm = editor_family("webstorm64").expect("supported");
    assert!(matches!(webstorm.style, Some(OpenStyle::JetBrains)));
}

#[test]
fn family_rejects_a_non_editor_executable() {
    assert!(editor_family("winrar").is_none());
}

#[test]
fn console_editors_are_flagged_terminal_but_gui_ones_are_not() {
    assert!(editor_family("nvim").expect("supported").terminal);
    assert!(editor_family("vi").expect("supported").terminal);
    assert!(!editor_family("code").expect("supported").terminal);
    assert!(!editor_family("gvim").expect("supported").terminal);
}

#[test]
fn new_jetbrains_registry_entries_resolve_their_open_style() {
    for command in ["rider", "clion", "phpstorm", "rubymine"] {
        assert!(
            matches!(open_style(command), Some(OpenStyle::JetBrains)),
            "{command} should open JetBrains-style"
        );
    }
}

#[test]
fn kinds_list_cpp_before_dotnet_and_android_first() {
    let kinds: Vec<String> = ide_kinds().into_iter().map(|info| info.kind).collect();
    assert_eq!(kinds.first().map(String::as_str), Some("android"));
    let position = |kind: &str| {
        kinds
            .iter()
            .position(|candidate| candidate == kind)
            .unwrap_or_else(|| panic!("{kind} is a recognised kind"))
    };
    assert!(position("cpp") < position("dotnet"));
}

#[test]
fn kind_signals_display_extension_markers_with_a_wildcard() {
    let dotnet = ide_kinds()
        .into_iter()
        .find(|info| info.kind == "dotnet")
        .expect("dotnet is a recognised kind");
    assert_eq!(dotnet.label, "C# / .NET");
    assert_eq!(
        dotnet.signals,
        ["global.json", "*.sln", "*.slnx", "*.csproj"]
    );
}

#[test]
fn open_style_resolves_an_added_editor_by_its_path() {
    assert!(matches!(
        open_style("C:\\Users\\me\\AppData\\Local\\Programs\\cursor\\Cursor.exe"),
        Some(OpenStyle::VsCode)
    ));
}

#[test]
fn vscode_style_reuses_the_window_and_jumps_to_the_line() {
    assert_eq!(
        open_arguments("code", "C:/p/file.ts".to_string(), Some(12), None),
        ["-r", "-g", "C:/p/file.ts:12"]
    );
}

#[test]
fn vscode_style_opens_the_file_inside_its_workspace() {
    assert_eq!(
        open_arguments("code", "C:/p/file.ts".to_string(), Some(12), Some("C:/p")),
        ["C:/p", "-g", "C:/p/file.ts:12"]
    );
    assert_eq!(
        open_arguments("code", "C:/p/file.ts".to_string(), None, Some("C:/p")),
        ["C:/p", "-g", "C:/p/file.ts"]
    );
}

#[test]
fn jetbrains_style_passes_the_line_flag_before_the_path() {
    assert_eq!(
        open_arguments("webstorm", "C:/p/file.ts".to_string(), Some(7), None),
        ["--line", "7", "C:/p/file.ts"]
    );
}

#[test]
fn jetbrains_style_leads_with_the_project_directory() {
    assert_eq!(
        open_arguments(
            "webstorm",
            "C:/p/file.ts".to_string(),
            Some(7),
            Some("C:/p")
        ),
        ["C:/p", "--line", "7", "C:/p/file.ts"]
    );
    assert_eq!(
        open_arguments("webstorm", "C:/p/file.ts".to_string(), None, Some("C:/p")),
        ["C:/p", "C:/p/file.ts"]
    );
}

#[test]
fn visual_studio_edits_the_file_and_drops_the_line_and_workspace() {
    assert_eq!(
        open_arguments("devenv", "C:/p/file.cs".to_string(), Some(42), None),
        ["/Edit", "C:/p/file.cs"]
    );
    // Its CLI has no trustworthy folder+file form, so the workspace is
    // deliberately ignored rather than guessed at.
    assert_eq!(
        open_arguments("devenv", "C:/p/file.cs".to_string(), Some(42), Some("C:/p")),
        ["/Edit", "C:/p/file.cs"]
    );
}

#[test]
fn path_colon_style_suffixes_the_line_onto_the_path() {
    assert_eq!(
        open_arguments("zed", "C:/p/file.ts".to_string(), Some(3), None),
        ["C:/p/file.ts:3"]
    );
}

#[test]
fn path_colon_style_opens_the_workspace_folder_alongside_the_file() {
    assert_eq!(
        open_arguments("zed", "C:/p/file.ts".to_string(), Some(3), Some("C:/p")),
        ["C:/p", "C:/p/file.ts:3"]
    );
}

#[test]
fn a_bare_path_passes_through_without_a_line() {
    assert_eq!(
        open_arguments("code", "C:/p".to_string(), None, None),
        ["C:/p"]
    );
}

#[test]
fn an_unknown_launcher_gets_the_plain_path_even_with_a_line_and_workspace() {
    assert_eq!(
        open_arguments("notepad", "C:/p/file.ts".to_string(), Some(9), None),
        ["C:/p/file.ts"]
    );
    // No known workspace+file form either - current behavior is kept.
    assert_eq!(
        open_arguments("notepad", "C:/p/file.ts".to_string(), Some(9), Some("C:/p")),
        ["C:/p/file.ts"]
    );
}
