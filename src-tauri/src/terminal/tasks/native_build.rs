//! Native build-system manifests: make, cmake, meson, ninja, bazel, xmake, premake, scons and qmake.

use std::collections::HashSet;
use std::path::Path;

use super::{quoted_file_name, task, Task};
use crate::util::{is_on_path, read_jsonc};

/// `Makefile` - each target line (`^name:`), skipping directives (`.PHONY` etc.)
/// and duplicates. Light scan, no makefile grammar.
pub(super) fn make_tasks(path: &Path) -> Vec<Task> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    make_tasks_from_text(&text)
}

/// The pure scan behind [`make_tasks`]: targets from Makefile text.
pub(super) fn make_tasks_from_text(text: &str) -> Vec<Task> {
    let mut seen = HashSet::new();
    let mut tasks = Vec::new();
    for line in text.lines() {
        let Some(target) = make_target(line) else {
            continue;
        };
        if target.starts_with('.') || !seen.insert(target.clone()) {
            continue;
        }
        tasks.push(Task {
            command: format!("make {target}"),
            name: target,
        });
    }
    tasks
}

/// Pull a target name from a Makefile line: the token before a `:` that is made
/// only of `[A-Za-z0-9_.-]` (a rule head, not a variable or recipe body).
pub(super) fn make_target(line: &str) -> Option<String> {
    if line.starts_with([' ', '\t']) {
        return None; // recipe body, not a rule head
    }
    let head = line.split(':').next()?.trim();
    let is_rule_head = !head.is_empty()
        && head.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
        });
    if !is_rule_head {
        return None;
    }
    Some(head.to_string())
}

/// `CMakeLists.txt` - configure into a `build/` tree, build it, run its tests.
/// Only a list that declares its own `project()` is a buildable root; the ones
/// `add_subdirectory` pulls in are part of their parent's build, and offering
/// to configure those would hand the user three commands that fail.
///
/// Release reaches the two generator families at different moments: a
/// single-config generator (Makefiles, Ninja) takes the build type at configure
/// time, a multi-config one (Visual Studio) at build time - and each ignores the
/// other's spelling, so both are offered. `ctest` needs the configuration named
/// on a multi-config tree, or it finds no tests at all.
pub(super) fn cmake_tasks(path: &Path) -> Vec<Task> {
    if !declares_project(path) {
        return Vec::new();
    }
    let test = if cfg!(windows) {
        "ctest --test-dir build -C Debug"
    } else {
        "ctest --test-dir build"
    };
    vec![
        task("configure", "cmake -S . -B build".to_string()),
        task(
            "configure (Release)",
            "cmake -S . -B build -DCMAKE_BUILD_TYPE=Release".to_string(),
        ),
        task("build", "cmake --build build".to_string()),
        task(
            "build (Release)",
            "cmake --build build --config Release".to_string(),
        ),
        task("test", test.to_string()),
    ]
}

/// `CMakePresets.json` / `CMakeUserPresets.json` - the configurations the
/// project itself declares. A preset already carries its generator, build type
/// and cache, so it is a truer "build this project" than the generic trio
/// above. Hidden presets are the building blocks other presets inherit from,
/// never a configuration anyone selects.
pub(super) fn cmake_preset_tasks(path: &Path) -> Vec<Task> {
    let Some(json) = read_jsonc(path) else {
        return Vec::new();
    };
    let mut tasks = Vec::new();
    for (field, verb, command) in [
        ("configurePresets", "configure", "cmake --preset"),
        ("buildPresets", "build", "cmake --build --preset"),
        ("testPresets", "test", "ctest --preset"),
    ] {
        let Some(presets) = json.get(field).and_then(serde_json::Value::as_array) else {
            continue;
        };
        for preset in presets {
            let is_hidden = preset
                .get("hidden")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let Some(name) = preset.get("name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if is_hidden || name.is_empty() {
                continue;
            }
            tasks.push(task(
                &format!("{verb} ({name})"),
                format!("{command} \"{name}\""),
            ));
        }
    }
    tasks
}

/// `meson.build` - set up a `build/` tree, compile it, run its tests. Same
/// shape as `CMake`, including the rule that a `subdir()` child is not a root.
pub(super) fn meson_tasks(path: &Path) -> Vec<Task> {
    if !declares_project(path) {
        return Vec::new();
    }
    vec![
        task("setup", "meson setup build".to_string()),
        task("compile", "meson compile -C build".to_string()),
        task("test", "meson test -C build".to_string()),
    ]
}

/// Whether a `CMake` or `Meson` file declares its own `project(…)` outside a
/// comment - both spell a build root the same way, are case-insensitive, and
/// allow a space before the parenthesis.
pub(super) fn declares_project(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    text_declares_project(&text)
}

/// The pure scan behind [`declares_project`].
pub(super) fn text_declares_project(text: &str) -> bool {
    const PROJECT_KEYWORD: &str = "project";
    text.lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let keyword = line.get(..PROJECT_KEYWORD.len())?;
            let is_project_call = keyword.eq_ignore_ascii_case(PROJECT_KEYWORD);
            is_project_call.then(|| &line[PROJECT_KEYWORD.len()..])
        })
        .any(|rest| rest.trim_start().starts_with('('))
}

/// A generated `build.ninja` - the build it describes, and its clean tool. It
/// usually sits in a build directory the scan prunes, so this surfaces only
/// where a project keeps its ninja file in the tree.
pub(super) fn ninja_tasks(_path: &Path) -> Vec<Task> {
    vec![
        task("build", "ninja".to_string()),
        task("clean", "ninja -t clean".to_string()),
    ]
}

/// `MODULE.bazel` / `WORKSPACE` - the workspace-wide verbs. `//...` is every
/// target in the workspace, which is what a workspace-level build means; `run`
/// needs one named target, so it stays the user's to type.
pub(super) fn bazel_tasks(_path: &Path) -> Vec<Task> {
    [
        ("build", "bazel build //..."),
        ("test", "bazel test //..."),
        ("clean", "bazel clean"),
    ]
    .into_iter()
    .map(|(name, command)| task(name, command.to_string()))
    .collect()
}

/// `xmake.lua` - xmake drives the whole cycle itself, and the build mode is its
/// own step: `xmake f -m release` sticks until it is changed again. `-y` answers
/// the package-install confirmation, which a project with `add_requires` stops
/// on - a one-click task must never wedge waiting on stdin.
pub(super) fn xmake_tasks(_path: &Path) -> Vec<Task> {
    [
        ("build", "xmake build -y"),
        ("switch to release", "xmake f -m release -y"),
        ("run", "xmake run -y"),
        ("test", "xmake test -y"),
        ("clean", "xmake clean"),
    ]
    .into_iter()
    .map(|(name, command)| task(name, command.to_string()))
    .collect()
}

/// `premake5.lua` - premake generates the real build files, so its tasks are
/// the generators, one per toolchain worth offering on either platform. There is
/// no `clean` action in premake5 (it was premake4's and was never ported), and
/// `gmake2` is only a deprecated alias of `gmake` since 5.0-beta5.
pub(super) fn premake_tasks(_path: &Path) -> Vec<Task> {
    [
        ("generate (gmake)", "premake5 gmake"),
        ("generate (vs2022)", "premake5 vs2022"),
    ]
    .into_iter()
    .map(|(name, command)| task(name, command.to_string()))
    .collect()
}

/// `SConstruct` - scons builds the default targets, and cleans with `-c`.
pub(super) fn scons_tasks(_path: &Path) -> Vec<Task> {
    vec![
        task("build", "scons".to_string()),
        task("clean", "scons -c".to_string()),
    ]
}

/// A Qt project file. `.pro` is a common extension outside Qt, so the file has
/// to look like one first: qmake writes the makefile, and make builds it -
/// `nmake` on Windows, where qmake targets MSVC.
pub(super) fn qmake_tasks(path: &Path) -> Vec<Task> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    if !looks_like_qmake_project(&text) {
        return Vec::new();
    }
    let make = make_program();
    vec![
        task("configure", format!("qmake {}", quoted_file_name(path))),
        task("build", make.to_string()),
        task("clean", format!("{make} clean")),
    ]
}

/// What builds a qmake-generated makefile here. On Windows that depends on the
/// Qt kit, not on the OS: MSVC's `nmake` lives in a developer prompt, while a
/// MinGW kit ships `mingw32-make` - so ask PATH rather than assume, and fall
/// back to the MSVC name it is normally built with.
pub(super) fn make_program() -> &'static str {
    if !cfg!(windows) {
        return "make";
    }

    if is_on_path("nmake") {
        return "nmake";
    }

    if is_on_path("mingw32-make") {
        return "mingw32-make";
    }

    "nmake"
}

/// The pure scan behind [`qmake_tasks`]: whether the file assigns one of
/// qmake's own variables, which a `.pro` from any other tool will not.
pub(super) fn looks_like_qmake_project(text: &str) -> bool {
    const QMAKE_VARIABLES: &[&str] = &["TEMPLATE", "TARGET", "QT", "SOURCES", "HEADERS", "CONFIG"];
    text.lines().map(str::trim_start).any(|line| {
        QMAKE_VARIABLES.iter().any(|variable| {
            line.strip_prefix(variable)
                .is_some_and(|rest| rest.trim_start().starts_with(['=', '+', '-']))
        })
    })
}
