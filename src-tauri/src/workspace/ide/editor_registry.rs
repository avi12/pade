//! The editor catalog: project kinds, editor coverage, open styles and the registry of known IDEs.

/// The source-language families PADE can match against an editor's capabilities.
/// The string identifier crosses the IPC and settings boundaries; this enum is
/// the sole Rust definition of that closed set.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ProjectKind {
    Android,
    Web,
    Python,
    Php,
    Ruby,
    Go,
    Rust,
    Java,
    Cpp,
    Dotnet,
}

impl ProjectKind {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Web => "web",
            Self::Python => "python",
            Self::Php => "php",
            Self::Ruby => "ruby",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Java => "java",
            Self::Cpp => "cpp",
            Self::Dotnet => "dotnet",
        }
    }

    /// Whether source of `kind` is native to this declared project ecosystem.
    /// Android is a platform declaration whose implementation language is
    /// represented by the Java/Kotlin family; every language kind owns itself.
    pub(super) fn owns_source_kind(self, kind: Self) -> bool {
        match self {
            Self::Android => matches!(kind, Self::Java),
            _ => self == kind,
        }
    }
}

/// The languages an editor can handle without inspecting its per-user plugin
/// installation. General-purpose editors deliberately cover every PADE kind;
/// product-specific editors declare only their shipped language families.
#[derive(Clone, Copy)]
pub(super) enum EditorCoverage {
    EveryKind,
    Kinds(&'static [ProjectKind]),
}

impl EditorCoverage {
    pub(super) fn supports(self, kind: ProjectKind) -> bool {
        match self {
            Self::EveryKind => true,
            Self::Kinds(kinds) => kinds.contains(&kind),
        }
    }

    /// Among editors that cover the declared project kinds, prefer the
    /// least-general tool. It selects `RustRover` for a Rust project while a
    /// genuinely hybrid set of markers still leaves only a generalist eligible.
    pub(super) const fn breadth(self) -> usize {
        match self {
            Self::EveryKind => usize::MAX,
            Self::Kinds(kinds) => kinds.len(),
        }
    }
}

/// How a launcher wants a jump-to-line request shaped on the command line -
/// verified against each editor's official CLI docs.
#[derive(Clone, Copy)]
pub(super) enum OpenStyle {
    /// VS Code family: `-g file:line` jumps to the line. With a workspace
    /// folder the folder leads (`code <dir> -g file:line`) so the file opens in
    /// that project's window; without one, `-r` reuses the already-open window
    /// (so it navigates in place rather than spawning a new one).
    VsCode,
    /// `JetBrains` family: `--line <n> file` - flags first, path last, with an
    /// optional leading project directory (`idea <dir> --line <n> file`). The
    /// launcher hands the file to the running IDE instance when one is up.
    JetBrains,
    /// Zed / Sublime: a `file:line` colon suffix on the path, optionally after
    /// the workspace folder (both CLIs open all listed paths in one window);
    /// the CLI routes to the running editor instance.
    PathColon,
    /// Visual Studio: `/Edit file` opens the file in the running instance. There
    /// is no reliable CLI to also jump to a line (combining `/Edit` with a
    /// `/Command "Edit.Goto"` doesn't navigate), so the line is dropped.
    VisualStudio,
}

pub(super) struct IdeDefinition {
    pub(super) id: &'static str,
    pub(super) label: &'static str,
    /// CLI launcher that opens a path (file or directory) when given it.
    pub(super) command: &'static str,
    /// How this launcher phrases a jump-to-line.
    pub(super) style: OpenStyle,
    /// Source-language families this product can cover for ranking purposes.
    pub(super) coverage: EditorCoverage,
}

pub(super) const REGISTRY: &[IdeDefinition] = &[
    IdeDefinition {
        id: "vscode",
        label: "VS Code",
        command: "code",
        style: OpenStyle::VsCode,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDefinition {
        id: "cursor",
        label: "Cursor",
        command: "cursor",
        style: OpenStyle::VsCode,
        coverage: EditorCoverage::EveryKind,
    },
    // The popular VS Code forks are generalists exactly like their parent -
    // same launcher conventions, same any-language reach.
    IdeDefinition {
        id: "antigravity",
        label: "Antigravity",
        command: "antigravity",
        style: OpenStyle::VsCode,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDefinition {
        id: "windsurf",
        label: "Windsurf",
        command: "windsurf",
        style: OpenStyle::VsCode,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDefinition {
        id: "vscodium",
        label: "VSCodium",
        command: "codium",
        style: OpenStyle::VsCode,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDefinition {
        id: "webstorm",
        label: "WebStorm",
        command: "webstorm",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web]),
    },
    IdeDefinition {
        id: "idea",
        label: "IntelliJ IDEA",
        command: "idea",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Java]),
    },
    IdeDefinition {
        id: "pycharm",
        label: "PyCharm",
        command: "pycharm",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Python]),
    },
    IdeDefinition {
        id: "goland",
        label: "GoLand",
        command: "goland",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Go]),
    },
    IdeDefinition {
        id: "rustrover",
        label: "RustRover",
        command: "rustrover",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Rust]),
    },
    IdeDefinition {
        id: "rider",
        label: "Rider",
        command: "rider",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Dotnet]),
    },
    IdeDefinition {
        id: "clion",
        label: "CLion",
        command: "clion",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Cpp]),
    },
    IdeDefinition {
        id: "phpstorm",
        label: "PhpStorm",
        command: "phpstorm",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Php]),
    },
    IdeDefinition {
        id: "rubymine",
        label: "RubyMine",
        command: "rubymine",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Ruby]),
    },
    IdeDefinition {
        id: "androidstudio",
        label: "Android Studio",
        command: "studio",
        style: OpenStyle::JetBrains,
        // Android only: a plain Java/Kotlin project belongs to IDEA (or another
        // general-purpose editor), never Android Studio. An Android project's
        // Java/Kotlin source is subsumed by the Android kind (`owns_source_kind`).
        coverage: EditorCoverage::Kinds(&[ProjectKind::Android]),
    },
    IdeDefinition {
        id: "zed",
        label: "Zed",
        command: "zed",
        style: OpenStyle::PathColon,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDefinition {
        id: "sublime",
        label: "Sublime Text",
        command: "subl",
        style: OpenStyle::PathColon,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDefinition {
        id: "visualstudio",
        label: "Visual Studio",
        command: "devenv",
        style: OpenStyle::VisualStudio,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Cpp, ProjectKind::Dotnet]),
    },
];
