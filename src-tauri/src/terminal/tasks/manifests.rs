//! The manifest registry: how each file is recognised and which extractor reads it.

use std::path::Path;

use super::dotnet::{
    dotnet_tasks, msbuild_tasks, NATIVE_PROJECT_EXTENSION, SOLUTION_EXTENSION,
    XML_SOLUTION_EXTENSION,
};
use super::jvm::{gradle_tasks, maven_tasks};
use super::native_build::{
    bazel_tasks, cmake_preset_tasks, cmake_tasks, make_tasks, meson_tasks, ninja_tasks,
    premake_tasks, qmake_tasks, scons_tasks, xmake_tasks,
};
use super::package_managers::{cargo_tasks, go_tasks, npm_tasks, python_tasks};
use super::{Task, TaskKind, TaskManifestDescriptor};

/// How a file in a scanned directory is recognised as a manifest. Most
/// ecosystems name their manifest exactly (`package.json`); the .NET and MSVC
/// ones name it after the project instead (`Renderer.csproj`), so those match
/// on extension and every matching file in the directory becomes its own group.
#[derive(Clone, Copy, Debug)]
pub(super) enum ManifestMatch {
    Names(&'static [&'static str]),
    Extensions(&'static [&'static str]),
}

impl ManifestMatch {
    /// Whether `file_name` (lowercased once by the caller) is this manifest.
    pub(super) fn matches(self, file_name: &str) -> bool {
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
    pub(super) fn descriptors(self) -> Vec<TaskManifestDescriptor> {
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
/// truth - the frontend's watched tokens and empty state are derived from it.
pub(super) struct ManifestDefinition {
    pub(super) matcher: ManifestMatch,
    pub(super) kind: TaskKind,
    pub(super) extract: fn(&Path) -> Vec<Task>,
}

pub(super) const MANIFESTS: &[ManifestDefinition] = &[
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["package.json"]),
        kind: TaskKind::Npm,
        extract: npm_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["Cargo.toml"]),
        kind: TaskKind::Cargo,
        extract: cargo_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["Makefile"]),
        kind: TaskKind::Make,
        extract: make_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["pyproject.toml"]),
        kind: TaskKind::Python,
        extract: python_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["CMakeLists.txt"]),
        kind: TaskKind::Cmake,
        extract: cmake_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["CMakePresets.json", "CMakeUserPresets.json"]),
        kind: TaskKind::Cmake,
        extract: cmake_preset_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["meson.build"]),
        kind: TaskKind::Meson,
        extract: meson_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["build.ninja"]),
        kind: TaskKind::Ninja,
        extract: ninja_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["MODULE.bazel", "WORKSPACE.bazel", "WORKSPACE"]),
        kind: TaskKind::Bazel,
        extract: bazel_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["xmake.lua"]),
        kind: TaskKind::Xmake,
        extract: xmake_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["premake5.lua"]),
        kind: TaskKind::Premake,
        extract: premake_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["SConstruct"]),
        kind: TaskKind::Scons,
        extract: scons_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Extensions(&[".pro"]),
        kind: TaskKind::Qmake,
        extract: qmake_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Extensions(&[
            SOLUTION_EXTENSION,
            XML_SOLUTION_EXTENSION,
            ".csproj",
            ".fsproj",
            ".vbproj",
        ]),
        kind: TaskKind::Dotnet,
        extract: dotnet_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Extensions(&[
            NATIVE_PROJECT_EXTENSION,
            SOLUTION_EXTENSION,
            XML_SOLUTION_EXTENSION,
        ]),
        kind: TaskKind::Msbuild,
        extract: msbuild_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["go.mod"]),
        kind: TaskKind::Golang,
        extract: go_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["build.gradle", "build.gradle.kts"]),
        kind: TaskKind::Gradle,
        extract: gradle_tasks,
    },
    ManifestDefinition {
        matcher: ManifestMatch::Names(&["pom.xml"]),
        kind: TaskKind::Maven,
        extract: maven_tasks,
    },
];
