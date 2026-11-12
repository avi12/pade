use crate::test_support::assert_empty;
use std::path::Path;

use super::dotnet::{declared_targets, dotnet_tasks, entry_point_targets, msbuild_tasks};
use super::jvm::{gradle_tasks, GRADLE, MAVEN};
use super::manifests::ManifestMatch;
use super::native_build::{
    cmake_preset_tasks, looks_like_qmake_project, make_target, make_tasks_from_text,
    text_declares_project,
};
use super::package_managers::{cargo_tasks, go_tasks, python_tasks_from_text, PackageManager};
use super::{
    quoted_file_name, relative_display, tasks_descriptors, Task, TaskKind, TaskManifestDescriptor,
};

fn names(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|task| task.name.as_str()).collect()
}

/// A scratch directory unique to this test process.
fn scratch_directory(name: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "pade-tasks-{name}-{process}",
        process = std::process::id()
    ));
    std::fs::create_dir_all(&directory).expect("scratch directory");
    directory
}

#[test]
fn descriptors_name_every_manifest_token_in_registry_order() {
    let descriptors = tasks_descriptors();

    assert_eq!(
        descriptors.first(),
        Some(&TaskManifestDescriptor::Name {
            value: "package.json"
        })
    );
    assert!(descriptors.contains(&TaskManifestDescriptor::Name {
        value: "CMakeLists.txt"
    }));
    assert!(descriptors.contains(&TaskManifestDescriptor::Extension { value: ".csproj" }));
    assert!(descriptors.contains(&TaskManifestDescriptor::Extension { value: ".vcxproj" }));
    assert!(descriptors.contains(&TaskManifestDescriptor::Name { value: "go.mod" }));
    assert!(descriptors.contains(&TaskManifestDescriptor::Name {
        value: "build.gradle.kts"
    }));
    assert!(descriptors.contains(&TaskManifestDescriptor::Name { value: "pom.xml" }));
}

#[test]
fn a_family_serializes_as_its_lowercase_name() {
    let names: Vec<serde_json::Value> = [TaskKind::Npm, TaskKind::Cmake, TaskKind::Msbuild]
        .into_iter()
        .map(|kind| serde_json::to_value(kind).expect("serializable"))
        .collect();
    assert_eq!(names, ["npm", "cmake", "msbuild"]);
}

#[test]
fn a_token_two_families_share_is_listed_once() {
    let descriptors = tasks_descriptors();
    let solutions = descriptors
        .iter()
        .filter(|descriptor| {
            matches!(descriptor, TaskManifestDescriptor::Extension { value } if *value == ".sln")
        })
        .count();

    assert_eq!(solutions, 1);
}

#[test]
fn a_manifest_is_recognised_whatever_its_case() {
    assert!(ManifestMatch::Names(&["Cargo.toml"]).matches("cargo.toml"));
    assert!(ManifestMatch::Extensions(&[".csproj"]).matches("renderer.csproj"));
    assert!(!ManifestMatch::Extensions(&[".csproj"]).matches("renderer.csproj.user"));
}

#[test]
fn a_command_names_the_manifest_it_builds_even_with_spaces_in_it() {
    assert_eq!(
        quoted_file_name(Path::new("src/My App.csproj")),
        "\"My App.csproj\""
    );
}

#[test]
fn a_build_root_declares_a_project_whatever_the_case_or_spacing() {
    assert!(text_declares_project(
        "cmake_minimum_required(VERSION 3.20)
PROJECT (demo)
"
    ));
    assert!(text_declares_project("project(demo LANGUAGES CXX)"));
    assert!(text_declares_project("project('demo', 'cpp')"));
    assert!(!text_declares_project("projection(demo)"));
    assert!(!text_declares_project("proj"));
    assert!(!text_declares_project("projé(demo)"));
}

#[test]
fn a_subdirectory_list_declares_no_project_of_its_own() {
    let text = "# part of the parent build
add_library(core STATIC core.cpp)
";
    assert!(!text_declares_project(text));
}

#[test]
fn cmake_presets_become_tasks_and_hidden_ones_do_not() {
    let directory = scratch_directory("cmake-presets");
    let presets = directory.join("CMakePresets.json");
    std::fs::write(
        &presets,
        r#"{
          "version": 6,
          "configurePresets": [
            { "name": "base", "hidden": true },
            { "name": "windows-debug" }
          ],
          "buildPresets": [{ "name": "windows-debug" }],
          "testPresets": [{ "name": "windows-debug" }]
        }"#,
    )
    .expect("write presets");

    let tasks = cmake_preset_tasks(&presets);
    assert_eq!(
        names(&tasks),
        [
            "configure (windows-debug)",
            "build (windows-debug)",
            "test (windows-debug)"
        ]
    );
    assert_eq!(tasks[1].command, "cmake --build --preset \"windows-debug\"");
    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn a_qt_project_file_is_told_apart_from_any_other_dot_pro() {
    assert!(looks_like_qmake_project("TEMPLATE = app"));
    assert!(looks_like_qmake_project("SOURCES += main.cpp"));
    assert!(!looks_like_qmake_project("parent(tom, bob)."));
}

#[test]
fn a_dotnet_project_can_be_run_and_watched_but_a_solution_cannot() {
    let project = dotnet_tasks(Path::new("app/Renderer.csproj"));
    assert_eq!(
        names(&project),
        [
            "build",
            "build (Release)",
            "run",
            "watch",
            "test",
            "publish",
            "restore",
            "clean"
        ]
    );
    assert_eq!(
        project[3].command,
        "dotnet watch --non-interactive --project \"Renderer.csproj\" run"
    );
    assert_eq!(
        project[5].command,
        "dotnet publish \"Renderer.csproj\" -c Release"
    );

    let directory = scratch_directory("managed-solution");
    let solution = directory.join("App.slnx");
    std::fs::write(
        &solution,
        "<Solution><Project Path=\"App.csproj\" /></Solution>",
    )
    .expect("write solution");
    assert_eq!(
        names(&dotnet_tasks(&solution)),
        [
            "build",
            "build (Release)",
            "test",
            "publish",
            "restore",
            "clean"
        ]
    );
    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn a_project_lists_its_own_targets_after_the_standard_verbs() {
    let directory = scratch_directory("project-targets");
    let project = directory.join("App.csproj");
    std::fs::write(
        &project,
        concat!(
            "<Project Sdk=\"Microsoft.NET.Sdk\">\n",
            "  <Import Project=\"Package.targets\" />\n",
            "  <Target Name=\"Package\">\n    <Message Text=\"pack\" />\n  </Target>\n",
            "  <Target\n      Name=\"PackageShare\"\n      DependsOnTargets=\"Package\">\n  </Target>\n",
            "  <Target Name=\"_Stamp\" />\n",
            "  <Target Name=\"VerifyNativePayload\" BeforeTargets=\"BeforeBuild\" />\n",
            "  <Target Name=\"Publish\" />\n",
            "  <Target Name=\"Package\" />\n",
            "</Project>\n"
        ),
    )
    .expect("write project");
    std::fs::write(
        directory.join("Package.targets"),
        "<Project><Target Name=\"Imported\" /></Project>",
    )
    .expect("write imported targets");

    let tasks = dotnet_tasks(&project);
    assert_eq!(
        names(&tasks),
        [
            "build",
            "build (Release)",
            "run",
            "watch",
            "test",
            "publish",
            "restore",
            "clean",
            "Package",
            "PackageShare"
        ]
    );
    assert_eq!(tasks[8].command, "dotnet build \"App.csproj\" -t:Package");
    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn a_target_hidden_in_an_imported_file_stays_out_because_nothing_watches_it() {
    let text = concat!(
        "<Project><Import Project=\"Package.targets\" />",
        "<Target Name=\"Own\" /></Project>"
    );

    assert_eq!(entry_point_targets(text), ["Own"]);
}

#[test]
fn an_attribute_is_matched_whole_and_may_hold_a_greater_than() {
    let text = concat!(
        "<Target Name=\"Ship\" Condition=\"'$(Version)' > '1'\" TargetName=\"decoy\" />",
        "<Target Name=\"After\" AfterTargets=\"Build\" />"
    );

    assert_eq!(entry_point_targets(text), ["Ship"]);
}

#[test]
fn a_project_that_cannot_be_read_contributes_no_targets() {
    assert_empty(&declared_targets(Path::new("nowhere/Absent.csproj")));
}

#[test]
fn a_native_solution_belongs_to_msbuild_and_a_managed_one_does_not() {
    let directory = scratch_directory("native-solution");
    let solution = directory.join("Engine.sln");
    std::fs::write(
        &solution,
        "Project(\"{8BC9CEB8}\") = \"Engine\", \"Engine/Engine.vcxproj\"",
    )
    .expect("write solution");

    assert!(dotnet_tasks(&solution).is_empty());
    assert_eq!(
        names(&msbuild_tasks(&solution)),
        ["build", "build (Release)", "rebuild", "clean"]
    );
    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn a_native_project_builds_the_configuration_visual_studio_opens_with() {
    let tasks = msbuild_tasks(Path::new("Engine.vcxproj"));
    assert_eq!(
        tasks[0].command,
        "msbuild \"Engine.vcxproj\" -restore -p:Configuration=Debug"
    );
}

#[test]
fn go_modules_offer_the_module_wide_verbs() {
    let tasks = go_tasks(Path::new("go.mod"));
    assert_eq!(names(&tasks), ["build", "run", "test", "vet"]);
    assert_eq!(tasks[2].command, "go test ./...");
}

#[test]
fn a_jvm_build_runs_its_committed_wrapper_and_falls_back_to_the_tool_on_path() {
    let directory = scratch_directory("gradle-wrapper");
    let script_name = if cfg!(windows) {
        GRADLE.windows_script
    } else {
        GRADLE.unix_script
    };
    let manifest = directory.join("build.gradle");
    std::fs::write(directory.join(script_name), "").expect("write wrapper");

    let separator = std::path::MAIN_SEPARATOR;
    assert_eq!(
        gradle_tasks(&manifest)[0].command,
        format!(".{separator}{script_name} build")
    );

    std::fs::remove_file(directory.join(script_name)).expect("remove wrapper");
    assert_eq!(
        gradle_tasks(&manifest)[0].command,
        format!("{} build", GRADLE.on_path)
    );
    assert_eq!(MAVEN.on_path, "mvn");
    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn make_target_reads_a_rule_head() {
    assert_eq!(make_target("build: src/main.rs"), Some("build".to_string()));
    assert_eq!(make_target("lint-all_v2:"), Some("lint-all_v2".to_string()));
}

#[test]
fn make_target_ignores_indented_recipe_bodies() {
    assert_eq!(make_target("\tcargo build"), None);
    assert_eq!(make_target("    echo done"), None);
}

#[test]
fn make_target_ignores_blank_and_comment_lines() {
    assert_eq!(make_target(""), None);
    assert_eq!(make_target("# a comment"), None);
}

#[test]
fn make_target_ignores_a_plain_variable_assignment() {
    assert_eq!(make_target("CC = gcc"), None);
}

#[test]
fn make_target_treats_a_colon_assignment_like_a_rule_head() {
    assert_eq!(make_target("VAR := value"), Some("VAR".to_string()));
}

#[test]
fn make_target_still_yields_dot_directives_for_the_caller_to_filter() {
    assert_eq!(make_target(".PHONY: build"), Some(".PHONY".to_string()));
}

#[test]
fn make_tasks_skip_dot_directives_and_duplicates() {
    let text = ".PHONY: build test\nbuild:\n\tcargo build\nbuild:\ntest:\n\tcargo test\n";
    let tasks = make_tasks_from_text(text);
    assert_eq!(names(&tasks), ["build", "test"]);
    assert!(tasks
        .iter()
        .all(|task| task.command == format!("make {}", task.name)));
}

#[test]
fn python_tasks_read_project_scripts_entries() {
    let text = "[project]\nname = \"demo\"\n\n[project.scripts]\nserve = \"demo.cli:serve\"\nmigrate = \"demo.cli:migrate\"\n";
    let tasks = python_tasks_from_text(text);
    assert_eq!(names(&tasks), ["serve", "migrate"]);
    assert!(tasks.iter().all(|task| task.command == task.name));
}

#[test]
fn python_tasks_read_poetry_scripts_entries() {
    let text = "[tool.poetry.scripts]\ncli = \"pkg:main\"\n";
    assert_eq!(names(&python_tasks_from_text(text)), ["cli"]);
}

#[test]
fn python_tasks_trim_quoted_script_keys() {
    let text = "[project.scripts]\n\"lint-all\" = \"demo.cli:lint\"\n";
    assert_eq!(names(&python_tasks_from_text(text)), ["lint-all"]);
}

#[test]
fn python_tasks_skip_comments_and_blank_lines_inside_the_table() {
    let text = "[project.scripts]\n# a comment\n\nserve = \"demo.cli:serve\"\n";
    assert_eq!(names(&python_tasks_from_text(text)), ["serve"]);
}

#[test]
fn python_tasks_ignore_non_script_tables() {
    let text = "[tool.ruff]\nline-length = 100\n\n[project.scripts]\nserve = \"x\"\n\n[tool.pytest.ini_options]\naddopts = \"-q\"\n";
    assert_eq!(names(&python_tasks_from_text(text)), ["serve"]);
}

#[test]
fn python_tasks_are_empty_without_a_scripts_table() {
    let text = "[project]\nname = \"demo\"\n";
    assert!(python_tasks_from_text(text).is_empty());
}

#[test]
fn npm_runs_scripts_through_npm_run_but_pnpm_and_yarn_take_the_bare_script() {
    assert_eq!(PackageManager::Npm.run_command("dev"), "npm run dev");
    assert_eq!(PackageManager::Pnpm.run_command("dev"), "pnpm dev");
    assert_eq!(PackageManager::Yarn.run_command("dev"), "yarn dev");
}

#[test]
fn cargo_manifests_always_offer_the_standard_verbs() {
    let tasks = cargo_tasks(Path::new("Cargo.toml"));
    assert_eq!(names(&tasks), ["build", "test", "run", "check", "clippy"]);
    assert!(tasks
        .iter()
        .all(|task| task.command == format!("cargo {}", task.name)));
}

#[test]
fn relative_display_strips_the_root_and_joins_with_forward_slashes() {
    let root = Path::new("repo");
    let path = root.join("crates").join("core").join("Cargo.toml");
    assert_eq!(relative_display(root, &path), "crates/core/Cargo.toml");
}

#[test]
fn relative_display_falls_back_to_the_full_path_outside_the_root() {
    let root = Path::new("alpha");
    let path = Path::new("beta").join("Makefile");
    assert_eq!(relative_display(root, &path), "beta/Makefile");
}
