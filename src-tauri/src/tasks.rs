//! Task runner: discover runnable tasks from a project's manifests.
//!
//! Scans the open project (bounded depth, skipping build/dep noise — the same
//! list `naming.rs` walks by) for the manifests it understands, extracts each
//! one's runnable tasks, and hands the frontend a run command per task.
//! Monorepo-aware (one group per manifest found) and multi-language: npm,
//! cargo, make, python, cmake (lists and presets), meson, ninja, bazel, xmake,
//! premake, scons, qmake, dotnet, msbuild, go, gradle and maven.
//! Read-only; nothing is executed here — the UI opens a terminal.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::util::{is_noise_directory, is_on_path, read_jsonc};

/// One runnable task: a display `name` and the shell `command` that runs it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    name: String,
    command: String,
}

/// The tasks extracted from one manifest.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGroup {
    /// Manifest path relative to the project root (`/`-joined).
    manifest: String,
    /// Absolute directory the tasks run in.
    dir: String,
    /// Manifest family: "npm" | "cargo" | "make" | "python" | "cmake" | "meson"
    /// | "ninja" | "bazel" | "xmake" | "premake" | "scons" | "qmake" | "dotnet"
    /// | "msbuild" | "go" | "gradle" | "maven".
    kind: String,
    tasks: Vec<Task>,
}

/// One token the frontend watches for (so a manifest landing on disk refreshes
/// the panel) and names in the empty state. A manifest is recognised either by
/// an exact file name or by an extension, and the frontend has to know which so
/// it can match a changed path the same way this module does. The registry
/// below remains the sole authority for both.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "match", rename_all = "camelCase")]
pub enum TaskManifestDescriptor {
    /// An exact file name: `package.json`.
    Name { value: &'static str },
    /// A file extension, dot included: `.csproj`.
    Extension { value: &'static str },
}

/// List every runnable task in `cwd`, grouped by manifest. The caller supplies
/// the window's workspace because multiple PADE windows share one process.
#[tauri::command]
pub async fn tasks_list(cwd: String) -> Result<Vec<TaskGroup>, String> {
    let root = PathBuf::from(cwd);
    if !root.is_dir() {
        return Err(format!(
            "task workspace is not a directory: {}",
            root.display()
        ));
    }
    let mut groups = Vec::new();
    for dir in manifest_directories(&root) {
        collect_group(&root, &dir, &mut groups);
    }
    Ok(groups)
}

/// Describe every manifest understood by [`tasks_list`], in discovery order.
/// Deduplicated: two families can recognise the same token — a solution is read
/// by both the .NET and the MSVC extractor — and the frontend wants the tokens,
/// not the registry rows.
#[tauri::command]
pub fn tasks_descriptors() -> Vec<TaskManifestDescriptor> {
    let mut descriptors: Vec<TaskManifestDescriptor> = Vec::new();
    for definition in MANIFESTS {
        for descriptor in definition.matcher.descriptors() {
            if !descriptors.contains(&descriptor) {
                descriptors.push(descriptor);
            }
        }
    }
    descriptors
}

/// Walk the project (bounded depth, skipping noise) and yield every directory
/// worth inspecting for manifests. Mirrors `naming.rs`'s iterative walk.
fn manifest_directories(root: &Path) -> Vec<PathBuf> {
    const MAXIMUM_DEPTH: u8 = 3;
    let mut directories = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0u8)];
    while let Some((dir, depth)) = stack.pop() {
        directories.push(dir.clone());
        if depth >= MAXIMUM_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if is_noise_directory(&name.to_string_lossy()) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, depth + 1));
            }
        }
    }
    directories
}

/// How a file in a scanned directory is recognised as a manifest. Most
/// ecosystems name their manifest exactly (`package.json`); the .NET and MSVC
/// ones name it after the project instead (`Renderer.csproj`), so those match
/// on extension and every matching file in the directory becomes its own group.
#[derive(Clone, Copy, Debug)]
enum ManifestMatch {
    Names(&'static [&'static str]),
    Extensions(&'static [&'static str]),
}

impl ManifestMatch {
    /// Whether `file_name` (lowercased once by the caller) is this manifest.
    fn matches(self, file_name: &str) -> bool {
        match self {
            ManifestMatch::Names(names) => names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(file_name)),
            ManifestMatch::Extensions(extensions) => extensions
                .iter()
                .any(|extension| file_name.ends_with(extension)),
        }
    }

    /// The tokens the frontend needs: one descriptor per name or extension.
    fn descriptors(self) -> Vec<TaskManifestDescriptor> {
        match self {
            ManifestMatch::Names(names) => names
                .iter()
                .map(|name| TaskManifestDescriptor::Name { value: name })
                .collect(),
            ManifestMatch::Extensions(extensions) => extensions
                .iter()
                .map(|extension| TaskManifestDescriptor::Extension { value: extension })
                .collect(),
        }
    }
}

/// One manifest ADE understands: how its file is recognised, its family, and
/// how to read its tasks. The registry (`MANIFESTS`) is the single source of
/// truth — the frontend's watched tokens and empty state are derived from it.
struct ManifestDefinition {
    matcher: ManifestMatch,
    kind: &'static str,
    extract: fn(&Path) -> Vec<Task>,
}

/// A solution names its projects instead of holding them, so both the .NET and
/// the MSVC family can claim one; each extractor reads it to decide. `.slnx` is
/// the newer XML form of the same file.
const SOLUTION_EXTENSION: &str = ".sln";
const SOLUTION_EXTENSIONS: &[&str] = &[SOLUTION_EXTENSION, ".slnx"];
/// Native C++ projects build through `MSBuild` — `dotnet` cannot build one.
const NATIVE_PROJECT_EXTENSION: &str = ".vcxproj";

const MANIFESTS: &[ManifestDefinition] = &[
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["package.json"]),
        kind: "npm",
        extract: npm_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["Cargo.toml"]),
        kind: "cargo",
        extract: cargo_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["Makefile"]),
        kind: "make",
        extract: make_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["pyproject.toml"]),
        kind: "python",
        extract: python_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["CMakeLists.txt"]),
        kind: "cmake",
        extract: cmake_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["CMakePresets.json", "CMakeUserPresets.json"]),
        kind: "cmake",
        extract: cmake_preset_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["meson.build"]),
        kind: "meson",
        extract: meson_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["build.ninja"]),
        kind: "ninja",
        extract: ninja_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["MODULE.bazel", "WORKSPACE.bazel", "WORKSPACE"]),
        kind: "bazel",
        extract: bazel_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["xmake.lua"]),
        kind: "xmake",
        extract: xmake_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["premake5.lua"]),
        kind: "premake",
        extract: premake_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["SConstruct"]),
        kind: "scons",
        extract: scons_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Extensions(&[".pro"]),
        kind: "qmake",
        extract: qmake_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Extensions(&[".sln", ".slnx", ".csproj", ".fsproj", ".vbproj"]),
        kind: "dotnet",
        extract: dotnet_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Extensions(&[
            NATIVE_PROJECT_EXTENSION,
            SOLUTION_EXTENSION,
            ".slnx",
        ]),
        kind: "msbuild",
        extract: msbuild_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["go.mod"]),
        kind: "go",
        extract: go_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["build.gradle", "build.gradle.kts"]),
        kind: "gradle",
        extract: gradle_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["pom.xml"]),
        kind: "maven",
        extract: maven_tasks,
    },
];

/// Extract this directory's manifests into `groups` (a monorepo dir can hold
/// several — e.g. a `package.json` and a `Cargo.toml` side by side, or three
/// sibling `.csproj` files). One `read_dir` serves every definition, since an
/// extension match has to see the directory's real file names anyway.
fn collect_group(root: &Path, dir: &Path, groups: &mut Vec<TaskGroup>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut file_names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    file_names.sort();
    for definition in MANIFESTS {
        for file_name in file_names
            .iter()
            .filter(|name| definition.matcher.matches(&name.to_ascii_lowercase()))
        {
            let path = dir.join(file_name);
            let tasks = (definition.extract)(&path);
            if tasks.is_empty() {
                continue;
            }
            groups.push(TaskGroup {
                manifest: relative_display(root, &path),
                dir: dir.to_string_lossy().into_owned(),
                kind: definition.kind.to_string(),
                tasks,
            });
        }
    }
}

/// A manifest's path relative to the project root, `/`-joined for display.
fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

// ── Manifest parsers ─────────────────────────────────────────────────────────

/// `package.json` — each `scripts` key is a task; the package manager is picked
/// from the lockfile in the same directory (pnpm/yarn/npm).
fn npm_tasks(path: &Path) -> Vec<Task> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = json.get("scripts").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let dir = path.parent().unwrap_or(path);
    let package_manager = PackageManager::detect(dir);
    scripts
        .keys()
        .map(|name| Task {
            command: package_manager.run_command(name),
            name: name.clone(),
        })
        .collect()
}

/// A JS package manager, the closed set ADE knows how to drive.
#[derive(Clone, Copy)]
enum PackageManager {
    Pnpm,
    Yarn,
    Npm,
}

impl PackageManager {
    /// Detect from a lockfile in `dir`: pnpm, then yarn, else npm.
    fn detect(dir: &Path) -> Self {
        if dir.join("pnpm-lock.yaml").exists() {
            PackageManager::Pnpm
        } else if dir.join("yarn.lock").exists() {
            PackageManager::Yarn
        } else {
            PackageManager::Npm
        }
    }

    /// The launcher binary name (the only place these literals live).
    fn executable(self) -> &'static str {
        match self {
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Npm => "npm",
        }
    }

    /// The command to run an npm script: `npm run <s>`, but `pnpm <s>` / `yarn <s>`
    /// for those managers (both accept the bare-script shorthand).
    fn run_command(self, script: &str) -> String {
        let executable = self.executable();
        match self {
            PackageManager::Pnpm | PackageManager::Yarn => format!("{executable} {script}"),
            PackageManager::Npm => format!("{executable} run {script}"),
        }
    }
}

/// `Cargo.toml` — the standard cargo verbs, always available for a crate.
fn cargo_tasks(_path: &Path) -> Vec<Task> {
    ["build", "test", "run", "check", "clippy"]
        .into_iter()
        .map(|verb| Task {
            name: verb.to_string(),
            command: format!("cargo {verb}"),
        })
        .collect()
}

/// `Makefile` — each target line (`^name:`), skipping directives (`.PHONY` etc.)
/// and duplicates. Light scan, no makefile grammar.
fn make_tasks(path: &Path) -> Vec<Task> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    make_tasks_from_text(&text)
}

/// The pure scan behind [`make_tasks`]: targets from Makefile text.
fn make_tasks_from_text(text: &str) -> Vec<Task> {
    let mut seen = Vec::new();
    let mut tasks = Vec::new();
    for line in text.lines() {
        let Some(target) = make_target(line) else {
            continue;
        };
        if target.starts_with('.') || seen.contains(&target) {
            continue;
        }
        seen.push(target.clone());
        tasks.push(Task {
            command: format!("make {target}"),
            name: target,
        });
    }
    tasks
}

/// Pull a target name from a Makefile line: the token before a `:` that is made
/// only of `[A-Za-z0-9_.-]` (a rule head, not a variable or recipe body).
fn make_target(line: &str) -> Option<String> {
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

/// `pyproject.toml` — console-script keys under `[project.scripts]` or
/// `[tool.poetry.scripts]`; the command is the script name itself. Light line
/// scan (no toml crate). Empty if neither table has entries.
fn python_tasks(path: &Path) -> Vec<Task> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    python_tasks_from_text(&text)
}

/// The pure scan behind [`python_tasks`]: script keys from pyproject text.
fn python_tasks_from_text(text: &str) -> Vec<Task> {
    let mut in_scripts = false;
    let mut tasks = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_scripts = matches!(trimmed, "[project.scripts]" | "[tool.poetry.scripts]");
            continue;
        }
        if !in_scripts || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(name) = trimmed
            .split('=')
            .next()
            .map(|key| key.trim().trim_matches('"'))
        else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        tasks.push(Task {
            name: name.to_string(),
            command: name.to_string(),
        });
    }
    tasks
}

/// One task, from the name shown in the panel and the command behind it.
fn task(name: &str, command: String) -> Task {
    Task {
        name: name.to_string(),
        command,
    }
}

/// The manifest's own file name, quoted for the command line: a directory can
/// hold several `.csproj` files, so every command names the one it builds, and
/// project names contain spaces often enough to need the quotes.
fn quoted_file_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    format!("\"{name}\"")
}

/// Whether a path is a solution file — the `.sln` a project list lives in, or
/// its newer `.slnx` form.
fn is_solution(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    let name = name.to_string_lossy().to_ascii_lowercase();
    SOLUTION_EXTENSIONS
        .iter()
        .any(|extension| name.ends_with(extension))
}

/// Whether a solution lists a native C++ project. A `.sln` is just a list of
/// project files, so this is what separates the two families that can claim
/// one: `MSBuild` builds the native solutions, `dotnet` the managed ones.
fn references_native_project(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    text.to_ascii_lowercase().contains(NATIVE_PROJECT_EXTENSION)
}

/// `CMakeLists.txt` — configure into a `build/` tree, build it, run its tests.
/// Only a list that declares its own `project()` is a buildable root; the ones
/// `add_subdirectory` pulls in are part of their parent's build, and offering
/// to configure those would hand the user three commands that fail.
///
/// Release reaches the two generator families at different moments: a
/// single-config generator (Makefiles, Ninja) takes the build type at configure
/// time, a multi-config one (Visual Studio) at build time — and each ignores the
/// other's spelling, so both are offered. `ctest` needs the configuration named
/// on a multi-config tree, or it finds no tests at all.
fn cmake_tasks(path: &Path) -> Vec<Task> {
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

/// `CMakePresets.json` / `CMakeUserPresets.json` — the configurations the
/// project itself declares. A preset already carries its generator, build type
/// and cache, so it is a truer "build this project" than the generic trio
/// above. Hidden presets are the building blocks other presets inherit from,
/// never a configuration anyone selects.
fn cmake_preset_tasks(path: &Path) -> Vec<Task> {
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

/// `meson.build` — set up a `build/` tree, compile it, run its tests. Same
/// shape as `CMake`, including the rule that a `subdir()` child is not a root.
fn meson_tasks(path: &Path) -> Vec<Task> {
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
/// comment — both spell a build root the same way, are case-insensitive, and
/// allow a space before the parenthesis.
fn declares_project(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    text_declares_project(&text)
}

/// The pure scan behind [`declares_project`].
fn text_declares_project(text: &str) -> bool {
    text.lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("project")
                .map(str::to_string)
        })
        .any(|rest| rest.trim_start().starts_with('('))
}

/// A generated `build.ninja` — the build it describes, and its clean tool. It
/// usually sits in a build directory the scan prunes, so this surfaces only
/// where a project keeps its ninja file in the tree.
fn ninja_tasks(_path: &Path) -> Vec<Task> {
    vec![
        task("build", "ninja".to_string()),
        task("clean", "ninja -t clean".to_string()),
    ]
}

/// `MODULE.bazel` / `WORKSPACE` — the workspace-wide verbs. `//...` is every
/// target in the workspace, which is what a workspace-level build means; `run`
/// needs one named target, so it stays the user's to type.
fn bazel_tasks(_path: &Path) -> Vec<Task> {
    [
        ("build", "bazel build //..."),
        ("test", "bazel test //..."),
        ("clean", "bazel clean"),
    ]
    .into_iter()
    .map(|(name, command)| task(name, command.to_string()))
    .collect()
}

/// `xmake.lua` — xmake drives the whole cycle itself, and the build mode is its
/// own step: `xmake f -m release` sticks until it is changed again. `-y` answers
/// the package-install confirmation, which a project with `add_requires` stops
/// on — a one-click task must never wedge waiting on stdin.
fn xmake_tasks(_path: &Path) -> Vec<Task> {
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

/// `premake5.lua` — premake generates the real build files, so its tasks are
/// the generators, one per toolchain worth offering on either platform. There is
/// no `clean` action in premake5 (it was premake4's and was never ported), and
/// `gmake2` is only a deprecated alias of `gmake` since 5.0-beta5.
fn premake_tasks(_path: &Path) -> Vec<Task> {
    [
        ("generate (gmake)", "premake5 gmake"),
        ("generate (vs2022)", "premake5 vs2022"),
    ]
    .into_iter()
    .map(|(name, command)| task(name, command.to_string()))
    .collect()
}

/// `SConstruct` — scons builds the default targets, and cleans with `-c`.
fn scons_tasks(_path: &Path) -> Vec<Task> {
    vec![
        task("build", "scons".to_string()),
        task("clean", "scons -c".to_string()),
    ]
}

/// A Qt project file. `.pro` is a common extension outside Qt, so the file has
/// to look like one first: qmake writes the makefile, and make builds it —
/// `nmake` on Windows, where qmake targets MSVC.
fn qmake_tasks(path: &Path) -> Vec<Task> {
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
/// MinGW kit ships `mingw32-make` — so ask PATH rather than assume, and fall
/// back to the MSVC name it is normally built with.
fn make_program() -> &'static str {
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
fn looks_like_qmake_project(text: &str) -> bool {
    const QMAKE_VARIABLES: &[&str] = &["TEMPLATE", "TARGET", "QT", "SOURCES", "HEADERS", "CONFIG"];
    text.lines().map(str::trim_start).any(|line| {
        QMAKE_VARIABLES.iter().any(|variable| {
            line.strip_prefix(variable)
                .is_some_and(|rest| rest.trim_start().starts_with(['=', '+', '-']))
        })
    })
}

/// A solution or a `.csproj` / `.fsproj` / `.vbproj` — the standard `dotnet`
/// verbs against that one file, in both configurations. A solution has no
/// single entry point to `run` or watch, and a solution of native C++ projects
/// is `MSBuild`'s, not the SDK's.
fn dotnet_tasks(path: &Path) -> Vec<Task> {
    let solution = is_solution(path);
    if solution && references_native_project(path) {
        return Vec::new();
    }
    let target = quoted_file_name(path);
    let mut tasks = vec![
        task("build", format!("dotnet build {target}")),
        task(
            "build (Release)",
            format!("dotnet build {target} -c Release"),
        ),
    ];
    if !solution {
        tasks.push(task("run", format!("dotnet run --project {target}")));
        tasks.push(task(
            "watch",
            format!("dotnet watch --non-interactive --project {target} run"),
        ));
    }
    tasks.push(task("test", format!("dotnet test {target}")));
    tasks.push(task(
        "publish",
        format!("dotnet publish {target} -c Release"),
    ));
    tasks.push(task("restore", format!("dotnet restore {target}")));
    tasks.push(task("clean", format!("dotnet clean {target}")));
    for name in declared_targets(path) {
        tasks.push(task(&name, format!("dotnet build {target} -t:{name}")));
    }
    tasks
}

/// The verbs above, under the target names `MSBuild` answers them by — a project
/// that overrides one of these is offering nothing the list does not have.
const STANDARD_TARGETS: &[&str] = &[
    "build", "rebuild", "clean", "restore", "publish", "pack", "test", "run", "watch",
];

/// The targets a project declares for itself, which are what a `.csproj` has
/// instead of npm's scripts: `dotnet build <project> -t:<name>` runs one.
///
/// Read out of the manifest itself — the file the panel already watches — and
/// deliberately NOT out of what it imports: the registry decides which files the
/// frontend watches, and a `Directory.Build.targets` beside the project is not
/// one of them, so a target read from there would be listed once and then never
/// refresh when it changed. A project that wants its target on the panel
/// declares it in the project file.
fn declared_targets(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    entry_point_targets(&text)
}

/// The pure scan behind [`declared_targets`]: one project file's own entry
/// points, in the order it declares them. A target is an entry point — something
/// a person would click — unless it is internal (`_` by `MSBuild`'s own
/// convention), a hook that already runs as part of another build
/// (`BeforeTargets` / `AfterTargets`), or a standard verb the group already
/// offers.
fn entry_point_targets(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for element in elements(text, "Target") {
        let Some(name) = attribute(&element, "Name") else {
            continue;
        };
        let is_entry_point = !name.starts_with('_')
            && attribute(&element, "BeforeTargets").is_none()
            && attribute(&element, "AfterTargets").is_none()
            && !STANDARD_TARGETS.contains(&name.to_ascii_lowercase().as_str())
            && !names.contains(&name);
        if is_entry_point {
            names.push(name);
        }
    }
    names
}

/// Every named element's attribute text, tag by tag rather than line by line:
/// `MSBuild` lets one element's attributes span several lines. The closing `>` is
/// looked for outside quotes, since XML leaves `>` legal inside an attribute
/// value (a `Condition` comparing versions writes one).
fn elements(text: &str, name: &str) -> Vec<String> {
    let opening = format!("<{name}");
    let mut elements = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&opening) {
        let after = &rest[start + opening.len()..];
        let is_this_element = after.starts_with([' ', '\t', '\r', '\n', '>', '/']);
        let Some(end) = element_end(after) else {
            break;
        };
        if is_this_element {
            elements.push(after[..end].to_string());
        }
        rest = &after[end + 1..];
    }
    elements
}

/// Where an element's attribute text ends: the first `>` that is not inside a
/// quoted attribute value.
fn element_end(text: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (index, character) in text.char_indices() {
        let Some(open) = quote else {
            if character == '>' {
                return Some(index);
            }

            if matches!(character, '"' | '\'') {
                quote = Some(character);
            }

            continue;
        };
        if character == open {
            quote = None;
        }
    }
    None
}

/// One attribute's value out of an element's attribute text, matching the whole
/// name so `Name` never reads `BeforeTargets`' value or its own `TargetName`
/// neighbour. Both quote styles are legal XML.
fn attribute(element: &str, name: &str) -> Option<String> {
    let mut rest = element;
    while let Some(start) = rest.find(name) {
        let is_whole_name = rest[..start]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        let after = rest[start + name.len()..].trim_start();
        rest = &rest[start + name.len()..];
        if !is_whole_name {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let Some(quote) = value
            .chars()
            .next()
            .filter(|character| matches!(character, '"' | '\''))
        else {
            continue;
        };
        let value = &value[quote.len_utf8()..];
        let end = value.find(quote)?;
        return Some(value[..end].trim().to_string()).filter(|value| !value.is_empty());
    }
    None
}

/// A `.vcxproj`, or the solution that carries one: native C++ builds through
/// `MSBuild`. `Debug` is the configuration Visual Studio opens with, so it is the
/// one a "build" here means.
fn msbuild_tasks(path: &Path) -> Vec<Task> {
    if is_solution(path) && !references_native_project(path) {
        return Vec::new();
    }
    let target = quoted_file_name(path);
    vec![
        task(
            "build",
            format!("msbuild {target} -restore -p:Configuration=Debug"),
        ),
        task(
            "build (Release)",
            format!("msbuild {target} -restore -p:Configuration=Release"),
        ),
        task(
            "rebuild",
            format!("msbuild {target} -restore -t:Rebuild -p:Configuration=Debug"),
        ),
        task("clean", format!("msbuild {target} -t:Clean")),
    ]
}

/// `go.mod` — the module-wide verbs. `./...` is every package under the module,
/// which is what a module-level build or test means.
fn go_tasks(_path: &Path) -> Vec<Task> {
    [
        ("build", "go build ./..."),
        ("run", "go run ."),
        ("test", "go test ./..."),
        ("vet", "go vet ./..."),
    ]
    .into_iter()
    .map(|(name, command)| task(name, command.to_string()))
    .collect()
}

/// A build tool whose projects usually commit a wrapper script that pins the
/// tool's version — the JVM pair. Running the wrapper is the point of shipping
/// it, so it wins over whatever is on PATH.
struct WrappedTool {
    windows_script: &'static str,
    unix_script: &'static str,
    on_path: &'static str,
}

const GRADLE: WrappedTool = WrappedTool {
    windows_script: "gradlew.bat",
    unix_script: "gradlew",
    on_path: "gradle",
};

const MAVEN: WrappedTool = WrappedTool {
    windows_script: "mvnw.cmd",
    unix_script: "mvnw",
    on_path: "mvn",
};

impl WrappedTool {
    /// What to type in `dir`: the committed wrapper, named with an explicit
    /// relative path because the working directory is not on PATH, else the
    /// tool itself.
    fn launcher(&self, dir: &Path) -> String {
        let script = if cfg!(windows) {
            self.windows_script
        } else {
            self.unix_script
        };
        if dir.join(script).is_file() {
            return format!(".{}{script}", std::path::MAIN_SEPARATOR);
        }
        self.on_path.to_string()
    }
}

/// `build.gradle` / `build.gradle.kts` — the lifecycle tasks every Gradle build
/// has, whatever plugins it applies.
fn gradle_tasks(path: &Path) -> Vec<Task> {
    tool_tasks(path, &GRADLE, &["build", "test", "clean"])
}

/// `pom.xml` — the Maven lifecycle phases a build always answers to.
fn maven_tasks(path: &Path) -> Vec<Task> {
    tool_tasks(path, &MAVEN, &["compile", "test", "package", "clean"])
}

/// The shared shape of both JVM extractors: `<launcher> <verb>`, resolved
/// against the manifest's own directory.
fn tool_tasks(path: &Path, tool: &WrappedTool, verbs: &[&str]) -> Vec<Task> {
    let dir = path.parent().unwrap_or(path);
    let launcher = tool.launcher(dir);
    verbs
        .iter()
        .map(|verb| task(verb, format!("{launcher} {verb}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        cargo_tasks, cmake_preset_tasks, declared_targets, dotnet_tasks, entry_point_targets,
        go_tasks, gradle_tasks, looks_like_qmake_project, make_target, make_tasks_from_text,
        msbuild_tasks, python_tasks_from_text, quoted_file_name, relative_display,
        tasks_descriptors, text_declares_project, ManifestMatch, PackageManager, Task,
        TaskManifestDescriptor, GRADLE, MAVEN,
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
        assert!(declared_targets(Path::new("nowhere/Absent.csproj")).is_empty());
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
}
