//! The project-kind and language catalogs: the markers that prove a kind and the extensions that name a language.

use super::declarations::has_extension;
use super::editor_registry::ProjectKind;

/// A project-kind marker - something in the project root that signals the kind,
/// and how it is probed on disk.
pub(super) enum Marker {
    /// A file with this exact name exists in the root.
    Named(&'static str),
    /// A JSON file contains this top-level key. This distinguishes ecosystem
    /// manifests from unrelated files that happen to share a generic name.
    JsonKey(&'static str, &'static str),
    /// Any direct child has this extension (solution/project file names vary
    /// per project, so they're matched by extension).
    Extension(&'static str),
}

impl Marker {
    pub(super) fn is_present(&self, cwd: &std::path::Path) -> bool {
        match self {
            Self::Named(name) => cwd.join(name).exists(),
            Self::JsonKey(name, key) => std::fs::read_to_string(cwd.join(name))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .is_some_and(|json| json.get(key).is_some()),
            Self::Extension(extension) => has_extension(cwd, extension),
        }
    }

    /// The marker as the UI displays it (`*.sln` for an extension probe).
    pub(super) fn display(&self) -> String {
        match self {
            Self::Named(name) | Self::JsonKey(name, _) => (*name).to_string(),
            Self::Extension(extension) => format!("*.{extension}"),
        }
    }
}

/// One source-language family PADE recognises: the project markers that declare
/// it. Its languages live in [`LANGUAGE_REGISTRY`]. Editor support lives
/// separately in [`REGISTRY`], so adding a language never requires
/// preference-list tuning.
pub(super) struct KindDefinition {
    pub(super) kind: ProjectKind,
    pub(super) label: &'static str,
    pub(super) markers: &'static [Marker],
}

/// One language as GitHub Linguist names it, the family it belongs to, and the
/// filename extensions whose classification is unambiguous in the PADE-supported
/// set (ambiguous ones deliberately sit out). The single home for
/// extension → language → kind; the language bar reads languages, editor
/// coverage reads their kinds.
#[derive(Debug)]
pub(super) struct LanguageDefinition {
    /// The Linguist name, also accepted by the `linguist-language` Git attribute.
    pub(super) name: &'static str,
    pub(super) kind: ProjectKind,
    extensions: &'static [&'static str],
}

pub(super) const LANGUAGE_REGISTRY: &[LanguageDefinition] = &[
    LanguageDefinition {
        name: "TypeScript",
        kind: ProjectKind::Web,
        extensions: &["ts", "tsx", "mts", "cts"],
    },
    LanguageDefinition {
        name: "JavaScript",
        kind: ProjectKind::Web,
        extensions: &["js", "jsx", "mjs", "cjs"],
    },
    LanguageDefinition {
        name: "Svelte",
        kind: ProjectKind::Web,
        extensions: &["svelte"],
    },
    LanguageDefinition {
        name: "Vue",
        kind: ProjectKind::Web,
        extensions: &["vue"],
    },
    LanguageDefinition {
        name: "HTML",
        kind: ProjectKind::Web,
        extensions: &["html"],
    },
    LanguageDefinition {
        name: "CSS",
        kind: ProjectKind::Web,
        extensions: &["css"],
    },
    LanguageDefinition {
        name: "SCSS",
        kind: ProjectKind::Web,
        extensions: &["scss"],
    },
    LanguageDefinition {
        name: "Python",
        kind: ProjectKind::Python,
        extensions: &["py"],
    },
    LanguageDefinition {
        name: "PHP",
        kind: ProjectKind::Php,
        extensions: &["php"],
    },
    LanguageDefinition {
        name: "Ruby",
        kind: ProjectKind::Ruby,
        extensions: &["rb"],
    },
    LanguageDefinition {
        name: "Go",
        kind: ProjectKind::Golang,
        extensions: &["go"],
    },
    LanguageDefinition {
        name: "Rust",
        kind: ProjectKind::Rust,
        extensions: &["rs"],
    },
    LanguageDefinition {
        name: "Java",
        kind: ProjectKind::Java,
        extensions: &["java"],
    },
    LanguageDefinition {
        name: "Kotlin",
        kind: ProjectKind::Java,
        extensions: &["kt"],
    },
    LanguageDefinition {
        name: "C",
        kind: ProjectKind::Cpp,
        extensions: &["c", "h"],
    },
    LanguageDefinition {
        name: "C++",
        kind: ProjectKind::Cpp,
        extensions: &["cc", "cpp", "hpp"],
    },
    LanguageDefinition {
        name: "C#",
        kind: ProjectKind::Dotnet,
        extensions: &["cs"],
    },
];

/// The language a filename extension unambiguously names.
pub(super) fn language_of_extension(extension: &str) -> Option<&'static LanguageDefinition> {
    LANGUAGE_REGISTRY.iter().find(|language| {
        language
            .extensions
            .iter()
            .any(|known| known.eq_ignore_ascii_case(extension))
    })
}

/// The language a `linguist-language` attribute names.
pub(super) fn language_named(name: &str) -> Option<&'static LanguageDefinition> {
    LANGUAGE_REGISTRY
        .iter()
        .find(|language| language.name.eq_ignore_ascii_case(name))
}

pub(super) const KIND_REGISTRY: &[KindDefinition] = &[
    // Android is checked first: an Android project is also "web"/"java"-ish,
    // but Android Studio is the right call when its markers are there.
    KindDefinition {
        kind: ProjectKind::Android,
        label: "Android",
        markers: &[
            Marker::Named("AndroidManifest.xml"),
            Marker::Named("gradlew"),
            Marker::Named("settings.gradle"),
            Marker::Named("settings.gradle.kts"),
        ],
    },
    KindDefinition {
        kind: ProjectKind::Web,
        label: "Web / JavaScript",
        markers: &[
            Marker::Named("package.json"),
            Marker::Named("tsconfig.json"),
            Marker::Named("index.html"),
            Marker::JsonKey("manifest.json", "manifest_version"),
        ],
    },
    KindDefinition {
        kind: ProjectKind::Python,
        label: "Python",
        markers: &[
            Marker::Named("pyproject.toml"),
            Marker::Named("requirements.txt"),
            Marker::Named("setup.py"),
        ],
    },
    KindDefinition {
        kind: ProjectKind::Php,
        label: "PHP",
        markers: &[Marker::Named("composer.json")],
    },
    KindDefinition {
        kind: ProjectKind::Ruby,
        label: "Ruby",
        markers: &[Marker::Named("Gemfile")],
    },
    KindDefinition {
        kind: ProjectKind::Golang,
        label: "Go",
        markers: &[Marker::Named("go.mod")],
    },
    KindDefinition {
        kind: ProjectKind::Rust,
        label: "Rust",
        markers: &[Marker::Named("Cargo.toml")],
    },
    KindDefinition {
        kind: ProjectKind::Java,
        label: "Java",
        markers: &[
            Marker::Named("pom.xml"),
            Marker::Named("build.gradle"),
            Marker::Named("build.gradle.kts"),
        ],
    },
    // C/C++ before .NET: a Visual C++ solution also carries a .sln, and marker
    // files can't reliably separate C from C++, so one "cpp" kind covers both.
    KindDefinition {
        kind: ProjectKind::Cpp,
        label: "C / C++",
        markers: &[
            Marker::Named("CMakeLists.txt"),
            Marker::Named("meson.build"),
            Marker::Extension("vcxproj"),
        ],
    },
    KindDefinition {
        kind: ProjectKind::Dotnet,
        label: "C# / .NET",
        markers: &[
            Marker::Named("global.json"),
            Marker::Extension("sln"),
            Marker::Extension("slnx"),
            Marker::Extension("csproj"),
        ],
    },
];
