//! External IDE integration — open the current project in the editor of choice.
//!
//! ADE is agentic-first, but you still reach for a full IDE sometimes. It
//! detects installed editors (by their CLI launcher) and opens the active
//! project directory in the one you pick.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::Stdio;

use serde::Serialize;

use crate::util::is_on_path;

/// The source-language families PADE can match against an editor's capabilities.
/// The string identifier crosses the IPC and settings boundaries; this enum is
/// the sole Rust definition of that closed set.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ProjectKind {
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
    const fn as_str(self) -> &'static str {
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

    fn from_extension(extension: &str) -> Option<Self> {
        KIND_REGISTRY
            .iter()
            .find(|definition| {
                definition
                    .extensions
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(extension))
            })
            .map(|definition| definition.kind)
    }

    fn from_linguist_language(language: &str) -> Option<Self> {
        KIND_REGISTRY
            .iter()
            .find(|definition| {
                definition
                    .linguist_languages
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(language))
            })
            .map(|definition| definition.kind)
    }
}

/// The languages an editor can handle without inspecting its per-user plugin
/// installation. General-purpose editors deliberately cover every PADE kind;
/// product-specific editors declare only their shipped language families.
#[derive(Clone, Copy)]
enum EditorCoverage {
    EveryKind,
    Kinds(&'static [ProjectKind]),
}

impl EditorCoverage {
    fn supports(self, kind: ProjectKind) -> bool {
        match self {
            Self::EveryKind => true,
            Self::Kinds(kinds) => kinds.contains(&kind),
        }
    }

    /// On equal source coverage, prefer the least-general tool that fully fits
    /// the project. It selects `RustRover` for a Rust-only project while leaving
    /// a general editor to win whenever no specialist covers the whole mix.
    const fn breadth(self) -> usize {
        match self {
            Self::EveryKind => usize::MAX,
            Self::Kinds(kinds) => kinds.len(),
        }
    }
}

/// How a launcher wants a jump-to-line request shaped on the command line —
/// verified against each editor's official CLI docs.
#[derive(Clone, Copy)]
enum OpenStyle {
    /// VS Code family: `-r -g file:line` — reuse the already-open window (so it
    /// navigates in place rather than spawning a new one) and go to the line.
    VsCode,
    /// `JetBrains` family: `--line <n> file` — flags first, path last. The
    /// launcher hands the file to the running IDE instance when one is up.
    JetBrains,
    /// Zed / Sublime: a `file:line` colon suffix on the path; the CLI routes to
    /// the running editor instance.
    PathColon,
    /// Visual Studio: `/Edit file` opens the file in the running instance. There
    /// is no reliable CLI to also jump to a line (combining `/Edit` with a
    /// `/Command "Edit.Goto"` doesn't navigate), so the line is dropped.
    VisualStudio,
}

struct IdeDef {
    id: &'static str,
    label: &'static str,
    /// CLI launcher that opens a path (file or directory) when given it.
    command: &'static str,
    /// How this launcher phrases a jump-to-line.
    style: OpenStyle,
    /// `JetBrains` `jetbrains://<tool>/…` protocol tool id, when this IDE is a
    /// `JetBrains` one. The CLI can't reliably route a file to the *correct open
    /// project window* (it drops it into the last-active one); the protocol
    /// targets a project by name, so opening a file lands in the right window.
    protocol: Option<&'static str>,
    /// Source-language families this product can cover for ranking purposes.
    coverage: EditorCoverage,
}

const REGISTRY: &[IdeDef] = &[
    IdeDef {
        id: "vscode",
        label: "VS Code",
        command: "code",
        style: OpenStyle::VsCode,
        protocol: None,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDef {
        id: "cursor",
        label: "Cursor",
        command: "cursor",
        style: OpenStyle::VsCode,
        protocol: None,
        coverage: EditorCoverage::EveryKind,
    },
    // The popular VS Code forks are generalists exactly like their parent —
    // same launcher conventions, same any-language reach.
    IdeDef {
        id: "antigravity",
        label: "Antigravity",
        command: "antigravity",
        style: OpenStyle::VsCode,
        protocol: None,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDef {
        id: "windsurf",
        label: "Windsurf",
        command: "windsurf",
        style: OpenStyle::VsCode,
        protocol: None,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDef {
        id: "vscodium",
        label: "VSCodium",
        command: "codium",
        style: OpenStyle::VsCode,
        protocol: None,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDef {
        id: "webstorm",
        label: "WebStorm",
        command: "webstorm",
        style: OpenStyle::JetBrains,
        protocol: Some("webstorm"),
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web]),
    },
    IdeDef {
        id: "idea",
        label: "IntelliJ IDEA",
        command: "idea",
        style: OpenStyle::JetBrains,
        protocol: Some("idea"),
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Java]),
    },
    IdeDef {
        id: "pycharm",
        label: "PyCharm",
        command: "pycharm",
        style: OpenStyle::JetBrains,
        protocol: Some("pycharm"),
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Python]),
    },
    IdeDef {
        id: "goland",
        label: "GoLand",
        command: "goland",
        style: OpenStyle::JetBrains,
        protocol: Some("goland"),
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Go]),
    },
    IdeDef {
        id: "rustrover",
        label: "RustRover",
        command: "rustrover",
        style: OpenStyle::JetBrains,
        protocol: Some("rustrover"),
        coverage: EditorCoverage::Kinds(&[ProjectKind::Rust]),
    },
    IdeDef {
        id: "rider",
        label: "Rider",
        command: "rider",
        style: OpenStyle::JetBrains,
        protocol: Some("rider"),
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Dotnet]),
    },
    IdeDef {
        id: "clion",
        label: "CLion",
        command: "clion",
        style: OpenStyle::JetBrains,
        protocol: Some("clion"),
        coverage: EditorCoverage::Kinds(&[ProjectKind::Cpp]),
    },
    IdeDef {
        id: "phpstorm",
        label: "PhpStorm",
        command: "phpstorm",
        style: OpenStyle::JetBrains,
        protocol: Some("phpstorm"),
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Php]),
    },
    IdeDef {
        id: "rubymine",
        label: "RubyMine",
        command: "rubymine",
        style: OpenStyle::JetBrains,
        protocol: Some("rubymine"),
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Ruby]),
    },
    IdeDef {
        id: "androidstudio",
        label: "Android Studio",
        command: "studio",
        style: OpenStyle::JetBrains,
        protocol: Some("studio"),
        coverage: EditorCoverage::Kinds(&[ProjectKind::Android, ProjectKind::Java]),
    },
    IdeDef {
        id: "zed",
        label: "Zed",
        command: "zed",
        style: OpenStyle::PathColon,
        protocol: None,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDef {
        id: "sublime",
        label: "Sublime Text",
        command: "subl",
        style: OpenStyle::PathColon,
        protocol: None,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDef {
        id: "visualstudio",
        label: "Visual Studio",
        command: "devenv",
        style: OpenStyle::VisualStudio,
        protocol: None,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Cpp, ProjectKind::Dotnet]),
    },
];

/// A project-kind marker — something in the project root that signals the kind,
/// and how it is probed on disk.
enum Marker {
    /// A file with this exact name exists in the root.
    Named(&'static str),
    /// Any direct child has this extension (solution/project file names vary
    /// per project, so they're matched by extension).
    Extension(&'static str),
}

impl Marker {
    fn is_present(&self, cwd: &std::path::Path) -> bool {
        match self {
            Self::Named(name) => cwd.join(name).exists(),
            Self::Extension(extension) => has_ext(cwd, extension),
        }
    }

    /// The marker as the UI displays it (`*.sln` for an extension probe).
    fn display(&self) -> String {
        match self {
            Self::Named(name) => (*name).to_string(),
            Self::Extension(extension) => format!("*.{extension}"),
        }
    }
}

/// One source-language family PADE recognises. A single row is the authoritative
/// mapping from project markers, filename extensions, and GitHub Linguist names
/// to one project kind. Editor support lives separately in [`REGISTRY`], so
/// adding a language never requires preference-list tuning.
struct KindDef {
    kind: ProjectKind,
    label: &'static str,
    markers: &'static [Marker],
    /// Filename extensions whose language classification is unambiguous in the
    /// PADE-supported set. Ambiguous extensions deliberately sit out.
    extensions: &'static [&'static str],
    /// Canonical names accepted by the `linguist-language` Git attribute.
    linguist_languages: &'static [&'static str],
}

const KIND_REGISTRY: &[KindDef] = &[
    // Android is checked first: an Android project is also "web"/"java"-ish,
    // but Android Studio is the right call when its markers are there.
    KindDef {
        kind: ProjectKind::Android,
        label: "Android",
        markers: &[
            Marker::Named("AndroidManifest.xml"),
            Marker::Named("gradlew"),
            Marker::Named("settings.gradle"),
        ],
        extensions: &[],
        linguist_languages: &[],
    },
    KindDef {
        kind: ProjectKind::Web,
        label: "Web / JavaScript",
        markers: &[
            Marker::Named("package.json"),
            Marker::Named("tsconfig.json"),
            Marker::Named("index.html"),
        ],
        extensions: &[
            "ts", "tsx", "js", "jsx", "mjs", "svelte", "vue", "html", "css", "scss",
        ],
        linguist_languages: &[
            "JavaScript",
            "TypeScript",
            "Svelte",
            "Vue",
            "HTML",
            "CSS",
            "SCSS",
        ],
    },
    KindDef {
        kind: ProjectKind::Python,
        label: "Python",
        markers: &[
            Marker::Named("pyproject.toml"),
            Marker::Named("requirements.txt"),
            Marker::Named("setup.py"),
        ],
        extensions: &["py"],
        linguist_languages: &["Python"],
    },
    KindDef {
        kind: ProjectKind::Php,
        label: "PHP",
        markers: &[Marker::Named("composer.json")],
        extensions: &["php"],
        linguist_languages: &["PHP"],
    },
    KindDef {
        kind: ProjectKind::Ruby,
        label: "Ruby",
        markers: &[Marker::Named("Gemfile")],
        extensions: &["rb"],
        linguist_languages: &["Ruby"],
    },
    KindDef {
        kind: ProjectKind::Go,
        label: "Go",
        markers: &[Marker::Named("go.mod")],
        extensions: &["go"],
        linguist_languages: &["Go"],
    },
    KindDef {
        kind: ProjectKind::Rust,
        label: "Rust",
        markers: &[Marker::Named("Cargo.toml")],
        extensions: &["rs"],
        linguist_languages: &["Rust"],
    },
    KindDef {
        kind: ProjectKind::Java,
        label: "Java",
        markers: &[Marker::Named("pom.xml"), Marker::Named("build.gradle")],
        extensions: &["java", "kt"],
        linguist_languages: &["Java", "Kotlin"],
    },
    // C/C++ before .NET: a Visual C++ solution also carries a .sln, and marker
    // files can't reliably separate C from C++, so one "cpp" kind covers both.
    KindDef {
        kind: ProjectKind::Cpp,
        label: "C / C++",
        markers: &[
            Marker::Named("CMakeLists.txt"),
            Marker::Named("meson.build"),
            Marker::Extension("vcxproj"),
        ],
        extensions: &["c", "cc", "cpp", "h", "hpp"],
        linguist_languages: &["C", "C++"],
    },
    KindDef {
        kind: ProjectKind::Dotnet,
        label: "C# / .NET",
        markers: &[
            Marker::Named("global.json"),
            Marker::Extension("sln"),
            Marker::Extension("csproj"),
        ],
        extensions: &["cs"],
        linguist_languages: &["C#"],
    },
];

/// Census walk bounds — enough to weigh a real repo, bounded so the suggestion
/// never stalls on a huge working tree.
const CENSUS_MAX_DEPTH: usize = 5;
const CENSUS_MAX_FILES: usize = 4000;

/// The first 8 KiB is enough to identify binary, minified, and generator-marked
/// content without turning editor suggestion into a full repository scan.
const SOURCE_SAMPLE_BYTES: usize = 8 * 1024;
const MINIFIED_MIN_SAMPLE_BYTES: usize = 1024;
const MINIFIED_AVG_LINE_BYTES: usize = 400;
const LINGUIST_GENERATED: &str = "linguist-generated";
const LINGUIST_VENDORED: &str = "linguist-vendored";
const LINGUIST_DOCUMENTATION: &str = "linguist-documentation";
const LINGUIST_DETECTABLE: &str = "linguist-detectable";
const LINGUIST_LANGUAGE: &str = "linguist-language";
const LINGUIST_ATTRIBUTES: &[&str] = &[
    LINGUIST_GENERATED,
    LINGUIST_VENDORED,
    LINGUIST_DOCUMENTATION,
    LINGUIST_DETECTABLE,
    LINGUIST_LANGUAGE,
];

/// The Git-authored exclusions and classification that apply to one tracked
/// file. `None` means the corresponding attribute was unspecified.
#[derive(Default)]
struct LinguistAttributes {
    language: Option<ProjectKind>,
    generated: Option<bool>,
    vendored: Option<bool>,
    documentation: Option<bool>,
    detectable: Option<bool>,
}

impl LinguistAttributes {
    fn excludes_from_census(&self) -> bool {
        self.detectable == Some(false)
            || self.generated == Some(true)
            || self.vendored == Some(true)
            || self.documentation == Some(true)
    }

    fn set(&mut self, attribute: &str, value: &str) {
        let boolean = match value {
            "set" => Some(true),
            "unset" => Some(false),
            _ => None,
        };
        match attribute {
            LINGUIST_GENERATED => self.generated = boolean,
            LINGUIST_VENDORED => self.vendored = boolean,
            LINGUIST_DOCUMENTATION => self.documentation = boolean,
            LINGUIST_DETECTABLE => self.detectable = boolean,
            LINGUIST_LANGUAGE => self.language = ProjectKind::from_linguist_language(value),
            _ => {}
        }
    }
}

/// Intrinsic source properties used before language classification. Binary data
/// and generated output never influence editor coverage, whatever their path.
struct SourceContent {
    binary: bool,
    generated: bool,
}

/// Read the small content sample used to exclude non-source files. A source
/// file is classified only when its bytes were readable; guessing around a read
/// failure would make the result depend on a transient filesystem error.
fn source_content(path: &std::path::Path) -> Option<SourceContent> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut sample = [0u8; SOURCE_SAMPLE_BYTES];
    let read = file.read(&mut sample).ok()?;
    let sample = &sample[..read];
    let binary = sample.contains(&0);
    if binary {
        return Some(SourceContent {
            binary,
            generated: false,
        });
    }

    let text = String::from_utf8_lossy(sample).to_ascii_lowercase();
    let declares_generated = text.contains("generated by")
        || text.contains("code generated")
        || text.contains("autogenerated")
        || (text.contains("generated") && text.contains("do not edit"))
        || text.contains("sourcemappingurl=");
    let line_count = sample.split(|&byte| byte == b'\n').count();
    let average_line_bytes = sample.len() / line_count.max(1);
    let minified =
        sample.len() >= MINIFIED_MIN_SAMPLE_BYTES && average_line_bytes > MINIFIED_AVG_LINE_BYTES;
    Some(SourceContent {
        binary,
        generated: declares_generated || minified,
    })
}

/// Fold one file's bytes into the language profile. Git attributes are the
/// project author's explicit statement, so they override intrinsic generated
/// detection and extension classification.
fn weigh_file(
    path: &std::path::Path,
    attributes: &LinguistAttributes,
    totals: &mut BTreeMap<ProjectKind, u64>,
) {
    if attributes.excludes_from_census() {
        return;
    }
    let Some(content) = source_content(path) else {
        return;
    };
    let is_generated = attributes.generated.unwrap_or(content.generated);
    if content.binary || is_generated {
        return;
    }
    let Some(kind) = attributes.language.or_else(|| {
        path.extension()
            .and_then(|value| value.to_str())
            .and_then(ProjectKind::from_extension)
    }) else {
        return;
    };
    let bytes = std::fs::metadata(path).map_or(0, |meta| meta.len());
    *totals.entry(kind).or_insert(0) += bytes;
}

/// The Git repository root and the tracked paths below PADE's active project
/// directory. `--full-name` keeps those paths rooted at the repository, so the
/// census can join them safely after Git scopes the listing to the active folder.
struct GitRepository {
    root: std::path::PathBuf,
    tracked_paths: Vec<String>,
}

fn git_repository(cwd: &std::path::Path) -> Option<GitRepository> {
    let root_output = crate::util::command("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !root_output.status.success() {
        return None;
    }
    let root = std::path::PathBuf::from(String::from_utf8_lossy(&root_output.stdout).trim());
    if root.as_os_str().is_empty() {
        return None;
    }
    let files_output = crate::util::command("git")
        .arg("-C")
        .arg(cwd)
        .args(["ls-files", "--cached", "--full-name", "-z"])
        .output()
        .ok()?;
    if !files_output.status.success() {
        return None;
    }
    let tracked_paths = String::from_utf8_lossy(&files_output.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect();
    Some(GitRepository {
        root,
        tracked_paths,
    })
}

/// Resolve the project's `.gitattributes` through Git itself, including nested
/// files and Git's path-matching semantics. The fallback map is empty if Git
/// cannot evaluate the attributes; the tracked-file census remains usable.
fn git_linguist_attributes(
    repository: &GitRepository,
    paths: &[String],
) -> BTreeMap<String, LinguistAttributes> {
    let Ok(mut child) = crate::util::command("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["check-attr", "-z", "--stdin"])
        .args(LINGUIST_ATTRIBUTES)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    else {
        return BTreeMap::new();
    };
    let Some(mut input) = child.stdin.take() else {
        return BTreeMap::new();
    };
    let wrote_all_paths = paths.iter().all(|path| {
        input
            .write_all(path.as_bytes())
            .and_then(|()| input.write_all(&[0]))
            .is_ok()
    });
    drop(input);
    let Ok(output) = child.wait_with_output() else {
        return BTreeMap::new();
    };
    if !wrote_all_paths || !output.status.success() {
        return BTreeMap::new();
    }

    let listing = String::from_utf8_lossy(&output.stdout);
    let values: Vec<&str> = listing.split('\0').collect();
    let mut attributes = BTreeMap::new();
    for record in values.chunks_exact(3) {
        attributes
            .entry(record[0].to_string())
            .or_insert_with(LinguistAttributes::default)
            .set(record[1], record[2]);
    }
    attributes
}

/// Bounded filesystem-walk fallback for a folder that isn't a git repo: sum
/// census bytes under `dir`, skipping the hidden and dependency/build dirs the
/// marker probe skips (untracked build output physically lives here, so the
/// exclusions matter). A Git repository goes through [`git_repository`] instead.
fn census_walk(
    dir: &std::path::Path,
    depth: usize,
    files_left: &mut usize,
    totals: &mut BTreeMap<ProjectKind, u64>,
) {
    if depth > CENSUS_MAX_DEPTH || *files_left == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if *files_left == 0 {
            return;
        }
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if !IGNORED_PROBE_DIRS.contains(&name) {
                census_walk(&path, depth + 1, files_left, totals);
            }
            continue;
        }
        *files_left -= 1;
        weigh_file(&path, &LinguistAttributes::default(), totals);
    }
}

/// Byte totals per census kind for `cwd`. The file list is git's tracked set
/// when `cwd` is a repo — so untracked junk and ignored build output never sway
/// the mix — and a bounded filesystem walk otherwise. Bounded to
/// [`CENSUS_MAX_FILES`] either way so a huge tree can't stall a suggestion.
fn census(cwd: &std::path::Path) -> BTreeMap<ProjectKind, u64> {
    let mut totals = BTreeMap::new();
    if let Some(repository) = git_repository(cwd) {
        let paths: Vec<String> = repository
            .tracked_paths
            .iter()
            .take(CENSUS_MAX_FILES)
            .cloned()
            .collect();
        let attributes = git_linguist_attributes(&repository, &paths);
        for path in &paths {
            weigh_file(
                &repository.root.join(path),
                attributes
                    .get(path)
                    .unwrap_or(&LinguistAttributes::default()),
                &mut totals,
            );
        }
    } else {
        let mut files_left = CENSUS_MAX_FILES;
        census_walk(cwd, 0, &mut files_left, &mut totals);
    }
    totals
}

fn lookup(id: &str) -> Option<Ide> {
    if let Some(i) = REGISTRY.iter().find(|i| i.id == id) {
        return Some(Ide {
            id: i.id.into(),
            label: i.label.into(),
            command: i.command.into(),
            terminal: false,
        });
    }
    // User-added editors are first-class too — resolve them by their stored id.
    added_editors().into_iter().find(|e| e.id == id)
}

/// A launchable editor family PADE recognises. Keyed off an executable's
/// lowercased basename so the "Add editor…" flow and jump-to-line launching of
/// an added editor share one authoritative table (DRY). `style` is `None` for
/// editors with no line-jump CLI (the path is passed as-is); `protocol` is the
/// `JetBrains` tool id for `JetBrains` IDEs. `terminal` marks console editors
/// (Neovim, Vim, Helix) that PADE opens inside a terminal tab rather than
/// spawning as a detached window.
struct Family {
    label: &'static str,
    style: Option<OpenStyle>,
    protocol: Option<&'static str>,
    terminal: bool,
}

fn family(basename: &str) -> Option<Family> {
    // (label, jump-to-line style, JetBrains protocol, runs-in-a-terminal)
    let (label, style, protocol, terminal) = match basename {
        "code" => ("VS Code", Some(OpenStyle::VsCode), None, false),
        "code - insiders" => ("VS Code Insiders", Some(OpenStyle::VsCode), None, false),
        "cursor" => ("Cursor", Some(OpenStyle::VsCode), None, false),
        "antigravity" => ("Antigravity", Some(OpenStyle::VsCode), None, false),
        "windsurf" => ("Windsurf", Some(OpenStyle::VsCode), None, false),
        "codium" | "vscodium" => ("VSCodium", Some(OpenStyle::VsCode), None, false),
        "zed" => ("Zed", Some(OpenStyle::PathColon), None, false),
        "sublime_text" | "subl" => ("Sublime Text", Some(OpenStyle::PathColon), None, false),
        "notepad++" => ("Notepad++", None, None, false),
        "gvim" => ("GVim", None, None, false),
        "nvim" => ("Neovim", None, None, true),
        "vim" | "vi" => ("Vim", None, None, true),
        "hx" => ("Helix", None, None, true),
        "webstorm" | "webstorm64" => (
            "WebStorm",
            Some(OpenStyle::JetBrains),
            Some("webstorm"),
            false,
        ),
        "idea" | "idea64" => (
            "IntelliJ IDEA",
            Some(OpenStyle::JetBrains),
            Some("idea"),
            false,
        ),
        "pycharm" | "pycharm64" => (
            "PyCharm",
            Some(OpenStyle::JetBrains),
            Some("pycharm"),
            false,
        ),
        "goland" | "goland64" => ("GoLand", Some(OpenStyle::JetBrains), Some("goland"), false),
        "rider" | "rider64" => ("Rider", Some(OpenStyle::JetBrains), Some("rider"), false),
        "clion" | "clion64" => ("CLion", Some(OpenStyle::JetBrains), Some("clion"), false),
        "phpstorm" | "phpstorm64" => (
            "PhpStorm",
            Some(OpenStyle::JetBrains),
            Some("phpstorm"),
            false,
        ),
        "rubymine" | "rubymine64" => (
            "RubyMine",
            Some(OpenStyle::JetBrains),
            Some("rubymine"),
            false,
        ),
        "rustrover" | "rustrover64" => (
            "RustRover",
            Some(OpenStyle::JetBrains),
            Some("rustrover"),
            false,
        ),
        _ => return None,
    };
    Some(Family {
        label,
        style,
        protocol,
        terminal,
    })
}

/// An executable path's lowercased basename with a known launcher extension
/// stripped (`Code.exe` → `code`, `notepad++.exe` → `notepad++`).
fn exe_basename(path: &str) -> String {
    let file = path
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_lowercase();
    for ext in [".exe", ".cmd", ".bat", ".sh", ".app"] {
        if let Some(stripped) = file.strip_suffix(ext) {
            return stripped.to_string();
        }
    }
    file
}

/// The user-added editors as `Ide`s (command = the stored executable path). A
/// console editor (Neovim, Vim, Helix) is flagged `terminal` so the UI opens it
/// in a PADE terminal tab instead of spawning it as a detached window.
fn added_editors() -> Vec<Ide> {
    crate::workspace::load()
        .prefs
        .added_editors
        .into_iter()
        .map(|e| Ide {
            terminal: family(&exe_basename(&e.path)).is_some_and(|f| f.terminal),
            id: e.id,
            label: e.label,
            command: e.path,
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ide {
    id: String,
    label: String,
    command: String,
    /// A console editor PADE runs inside a terminal tab (never a detached window).
    terminal: bool,
}

#[tauri::command]
pub fn ide_detect() -> Vec<Ide> {
    REGISTRY
        .iter()
        .filter(|i| is_on_path(i.command))
        .map(|i| Ide {
            id: i.id.into(),
            label: i.label.into(),
            command: i.command.into(),
            terminal: false,
        })
        .chain(added_editors())
        .collect()
}

/// Add an editor by the full path to its executable. Validates the basename
/// against the launchable families ({@link family}) — an unsupported executable
/// (e.g. `WinRAR.exe`) is rejected with a message the UI shows inline. On
/// success the editor is persisted and appears in every editor menu.
#[tauri::command]
pub fn ide_add_editor(path: String) -> Result<crate::workspace::Settings, String> {
    let path = path.trim().to_string();
    if path.is_empty() {
        return Err("Enter the full path to an editor executable.".to_string());
    }
    let file = path
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(&path)
        .to_string();
    let Some(fam) = family(&exe_basename(&path)) else {
        return Err(format!(
            "\u{201c}{file}\u{201d} isn\u{2019}t a supported editor. PADE can launch \
             VS Code and its forks (Cursor, Antigravity, Windsurf, VSCodium), Zed, \
             Sublime Text, Neovim and the JetBrains IDEs."
        ));
    };
    crate::workspace::add_editor(crate::workspace::AddedEditor {
        id: format!("added-{}", exe_basename(&path)),
        label: fam.label.to_string(),
        path,
    })
}

/// Whether any direct child of `dir` has the given extension (case-insensitive).
fn has_ext(dir: &std::path::Path, ext: &str) -> bool {
    std::fs::read_dir(dir).ok().is_some_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(ext))
        })
    })
}

/// Directories that never signal a project kind of their own: dependency and
/// build output, plus anything hidden. Skipped when probing markers a level
/// down, so `node_modules` or `vendor` can't smuggle in a false kind.
const IGNORED_PROBE_DIRS: &[&str] = &["node_modules", "target", "vendor", "dist", "build", "out"];

/// The project root plus its direct child directories — everywhere a kind
/// marker may live. A hybrid app or a small monorepo keeps each side's manifest
/// in its own folder (`src-tauri/Cargo.toml`, `backend/go.mod`), and a root-only
/// probe would miss that whole side of the project.
fn probe_roots(cwd: &std::path::Path) -> Vec<std::path::PathBuf> {
    let children = std::fs::read_dir(cwd)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| !name.starts_with('.') && !IGNORED_PROBE_DIRS.contains(&name))
        });
    std::iter::once(cwd.to_path_buf()).chain(children).collect()
}

/// Sniff the project kinds present in the current directory from the
/// [`KIND_REGISTRY`] marker files, in the registry's priority order. Markers
/// are probed in the root and one level down (see [`probe_roots`]); they provide
/// a second, source-free signal when a new project has no countable files yet.
fn detect_kinds(cwd: &std::path::Path) -> Vec<ProjectKind> {
    let roots = probe_roots(cwd);
    KIND_REGISTRY
        .iter()
        .filter(|def| {
            def.markers
                .iter()
                .any(|marker| roots.iter().any(|root| marker.is_present(root)))
        })
        .map(|def| def.kind)
        .collect()
}

/// The single best-matching project kind for `cwd` — the highest-priority marker
/// present (Android before web/java, etc.), or `None` for an unrecognised project.
fn primary_kind(cwd: &std::path::Path) -> Option<ProjectKind> {
    detect_kinds(cwd).first().copied()
}

/// The map key for the "any other folder" fallback options (not a project kind).
/// Matches the `key` the frontend's fallback editor-select uses.
const FALLBACK_KEY: &str = "fallback";

/// Order-preserving dedup: keep each id's first appearance only. No mutable
/// accumulator — an id survives just at the index of its first occurrence.
fn dedup_in_order(ids: Vec<String>) -> Vec<String> {
    ids.iter()
        .enumerate()
        .filter(|(index, id)| ids.iter().position(|other| other == *id) == Some(*index))
        .map(|(_, id)| id.clone())
        .collect()
}

/// Rank each registered editor by the source bytes it can cover. Marker coverage
/// breaks a byte tie (e.g. Android Studio for an Android Java project), then the
/// least-general fully fitting editor wins. Registry order remains the stable
/// final tie-breaker for equivalent editors such as VS Code and its forks.
fn ranked_editor_ids(
    source_bytes: &BTreeMap<ProjectKind, u64>,
    marker_kinds: &[ProjectKind],
) -> Vec<String> {
    let has_evidence = !source_bytes.is_empty() || !marker_kinds.is_empty();
    let mut editors: Vec<&IdeDef> = REGISTRY.iter().collect();
    editors.sort_by(|left, right| {
        if !has_evidence {
            return matches!(right.coverage, EditorCoverage::EveryKind)
                .cmp(&matches!(left.coverage, EditorCoverage::EveryKind));
        }
        let left_source = source_bytes
            .iter()
            .filter(|(kind, _)| left.coverage.supports(**kind))
            .map(|(_, bytes)| *bytes)
            .sum::<u64>();
        let right_source = source_bytes
            .iter()
            .filter(|(kind, _)| right.coverage.supports(**kind))
            .map(|(_, bytes)| *bytes)
            .sum::<u64>();
        let left_markers = marker_kinds
            .iter()
            .filter(|kind| left.coverage.supports(**kind))
            .count();
        let right_markers = marker_kinds
            .iter()
            .filter(|kind| right.coverage.supports(**kind))
            .count();
        right_source
            .cmp(&left_source)
            .then_with(|| right_markers.cmp(&left_markers))
            .then_with(|| left.coverage.breadth().cmp(&right.coverage.breadth()))
    });
    editors.iter().map(|editor| editor.id.to_string()).collect()
}

/// Whether a known editor can cover every source language and project marker
/// observed in the current project. A web rule is a useful preference for an
/// all-web monorepo, but must never force `WebStorm` ahead of a web/Rust hybrid.
/// User-added editors lack a capability declaration, so their explicit rule is
/// retained rather than guessing what the user's installation supports.
fn editor_covers_project(
    id: &str,
    source_bytes: &BTreeMap<ProjectKind, u64>,
    marker_kinds: &[ProjectKind],
) -> bool {
    let has_evidence = !source_bytes.is_empty() || !marker_kinds.is_empty();
    REGISTRY
        .iter()
        .find(|editor| editor.id == id)
        .is_none_or(|editor| {
            if !has_evidence {
                return matches!(editor.coverage, EditorCoverage::EveryKind);
            }
            source_bytes
                .keys()
                .chain(marker_kinds.iter())
                .all(|kind| editor.coverage.supports(*kind))
        })
}

/// The compatible editor subset, preserving the coverage ranking. The menu is
/// an offer of editors that can work on the current project, not every installed
/// editor that happens to launch; a zero-coverage specialist is omitted.
fn suggestible_editor_ids(
    source_bytes: &BTreeMap<ProjectKind, u64>,
    marker_kinds: &[ProjectKind],
) -> Vec<String> {
    ranked_editor_ids(source_bytes, marker_kinds)
        .into_iter()
        .filter(|id| editor_covers_project(id, source_bytes, marker_kinds))
        .collect()
}

/// Editor ids to offer per editor-rules row, installed-only. A kind's list is
/// derived from the same capability table and coverage scorer as suggestions,
/// so an unrelated IDE is never offered. Unknown folders get only universal
/// editors and user-added launchers.
#[tauri::command]
pub fn ide_kind_options() -> BTreeMap<String, Vec<String>> {
    let installed = ide_detect();
    let is_present = |id: &str| installed.iter().any(|editor| editor.id == id);
    let added_ids = installed
        .iter()
        .filter(|editor| editor.id.starts_with("added-"))
        .map(|editor| editor.id.clone());

    let general = dedup_in_order(
        REGISTRY
            .iter()
            .filter(|editor| matches!(editor.coverage, EditorCoverage::EveryKind))
            .map(|editor| editor.id.to_string())
            .chain(added_ids.clone())
            .filter(|id| is_present(id))
            .collect(),
    );

    KIND_REGISTRY
        .iter()
        .map(|def| {
            let source_bytes = BTreeMap::from([(def.kind, 1)]);
            let options = dedup_in_order(
                ranked_editor_ids(&source_bytes, &[def.kind])
                    .into_iter()
                    .filter(|id| {
                        REGISTRY
                            .iter()
                            .find(|editor| editor.id == id)
                            .is_some_and(|editor| editor.coverage.supports(def.kind))
                    })
                    .filter(|id| is_present(id))
                    .chain(added_ids.clone())
                    .collect(),
            );
            (def.kind.as_str().to_string(), options)
        })
        .chain(std::iter::once((FALLBACK_KEY.to_string(), general.clone())))
        .collect()
}

/// One project kind as the editor-rules UI renders it — its id, display label,
/// and the marker signals shown under the label.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindInfo {
    kind: String,
    label: String,
    signals: Vec<String>,
}

/// Every recognised project kind, in the [`KIND_REGISTRY`]'s priority/render
/// order. Drives the editor-rules rows in the picker's settings.
#[tauri::command]
pub fn ide_kinds() -> Vec<KindInfo> {
    KIND_REGISTRY
        .iter()
        .map(|def| KindInfo {
            kind: def.kind.as_str().to_string(),
            label: def.label.to_string(),
            signals: def.markers.iter().map(Marker::display).collect(),
        })
        .collect()
}

/// The current project's primary kind (e.g. `"rust"`, `"web"`), or `None` when no
/// marker file is recognised. Drives the editor-rules UI in settings.
#[tauri::command]
pub fn ide_project_kind() -> Option<String> {
    std::env::current_dir()
        .ok()
        .as_deref()
        .and_then(primary_kind)
        .map(ProjectKind::as_str)
        .map(str::to_string)
}

/// An id is worth suggesting only if it's actually launchable — a registry
/// launcher on PATH, or a user-added editor (its stored path is the launcher).
fn is_installed(id: &str) -> bool {
    if id.starts_with("added-") {
        return added_editors().iter().any(|e| e.id == id);
    }
    lookup(id).is_some_and(|i| is_on_path(&i.command))
}

/// Installed IDEs ranked for the current project, best match first. The
/// editor-rules engine takes precedence only when its editor covers the complete
/// observed project. A user rule for the primary marker kind is considered first,
/// then the configured fallback, then byte-weighted editor coverage. No
/// dominant-language threshold or framework-specific detection is involved.
#[tauri::command]
pub fn ide_suggest() -> Result<Vec<Ide>, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let kinds = detect_kinds(&cwd);
    let source_bytes = census(&cwd);
    let prefs = crate::workspace::load().prefs;

    // 1) Compatible primary-kind rule, 2) compatible fallback, 3) coverage ranking.
    let rule = primary_kind(&cwd).and_then(|kind| prefs.ide_rules.get(kind.as_str()).cloned());
    let configured = rule
        .into_iter()
        .chain(prefs.ide_fallback)
        .filter(|id| editor_covers_project(id, &source_bytes, &kinds));
    let auto = suggestible_editor_ids(&source_bytes, &kinds);

    let mut ordered: Vec<String> = Vec::new();
    for id in configured.chain(auto) {
        let is_new_and_installed = !ordered.contains(&id) && is_installed(&id);
        if is_new_and_installed {
            ordered.push(id);
        }
    }
    Ok(ordered.iter().filter_map(|id| lookup(id)).collect())
}

/// The jump-to-line style for a launcher command, or `None` for an unknown one.
/// An added editor's command is its executable path, so fall back to matching
/// the family by basename.
fn open_style(command: &str) -> Option<OpenStyle> {
    if let Some(i) = REGISTRY.iter().find(|i| i.command == command) {
        return Some(i.style);
    }
    family(&exe_basename(command)).and_then(|f| f.style)
}

/// The launcher arguments for opening `target` — jumping to `line` when one is
/// given and the launcher's style is known (otherwise the path is passed as-is,
/// which every editor accepts).
fn open_args(command: &str, target: String, line: Option<u32>) -> Vec<String> {
    match (line, open_style(command)) {
        (Some(n), Some(OpenStyle::VsCode)) => {
            vec!["-r".to_owned(), "-g".to_owned(), format!("{target}:{n}")]
        }
        (Some(n), Some(OpenStyle::JetBrains)) => vec!["--line".to_owned(), n.to_string(), target],
        (Some(n), Some(OpenStyle::PathColon)) => vec![format!("{target}:{n}")],
        // Visual Studio opens the file in the running instance; no line jump.
        (_, Some(OpenStyle::VisualStudio)) => vec!["/Edit".to_owned(), target],
        _ => vec![target],
    }
}

/// Open a path in the given IDE launcher. `path` defaults to the current project
/// directory when omitted (topbar); a `line` (only meaningful with a file path)
/// jumps the editor to that line, phrased per the launcher's own CLI.
#[tauri::command]
pub fn ide_open(command: String, path: Option<String>, line: Option<u32>) -> Result<(), String> {
    let has_path = path.is_some();
    let target = match path {
        Some(p) => p,
        None => std::env::current_dir()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned(),
    };
    // A line only applies when we were handed a file to jump into.
    let args = open_args(&command, target, line.filter(|_| has_path));

    // On Windows the JetBrains/VS Code launchers are .cmd shims, so go through
    // the shell to resolve them the way a terminal would. An added editor's
    // command is an absolute executable path, so spawn it directly instead.
    let is_path = command.contains('/') || command.contains('\\');
    let spawn = if cfg!(windows) && !is_path {
        crate::util::command("cmd")
            .arg("/C")
            .arg(&command)
            .args(&args)
            .spawn()
    } else {
        crate::util::command(&command).args(&args).spawn()
    };
    spawn
        .map(|_| ())
        .map_err(|e| format!("failed to open {command}: {e}"))
}

/// The `JetBrains` protocol tool id for a launcher command, if it is a
/// `JetBrains` IDE (else `None`, meaning "use the CLI").
fn protocol_id(command: &str) -> Option<&'static str> {
    if let Some(i) = REGISTRY.iter().find(|i| i.command == command) {
        return i.protocol;
    }
    family(&exe_basename(command)).and_then(|f| f.protocol)
}

/// The project's display name — its root folder's basename, which is how the
/// `JetBrains` protocol identifies an open project.
fn project_name(project: &str) -> String {
    project
        .replace('\\', "/")
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(project)
        .to_string()
}

/// `file` expressed relative to `project` (forward slashes), or the absolute
/// path when it isn't under the project. Matching is case-insensitive on `Windows`.
fn relative_path(project: &str, file: &str) -> String {
    let root = project.replace('\\', "/");
    let root = root.trim_end_matches('/');
    let path = file.replace('\\', "/");
    let under = if cfg!(windows) {
        path.to_lowercase().starts_with(&root.to_lowercase())
    } else {
        path.starts_with(root)
    };
    if under {
        path.get(root.len()..)
            .unwrap_or(&path)
            .trim_start_matches('/')
            .to_string()
    } else {
        path
    }
}

/// Open a file in the IDE so it lands in the window that already has `project`
/// open, jumping to `line` when given. A `JetBrains` IDE uses its `jetbrains://`
/// scheme (targets the project by name, unlike the CLI); other editors use the
/// CLI open (`VS Code` reuses its window with `-r`). `project` is the root dir.
#[tauri::command]
pub fn ide_open_file(
    command: String,
    project: String,
    file: String,
    line: Option<u32>,
) -> Result<(), String> {
    if let Some(tool) = protocol_id(&command) {
        let mut path = relative_path(&project, &file);
        if let Some(number) = line {
            path.push(':');
            path.push_str(&number.to_string());
        }
        // Keep `/` and `:` literal so the path and its `:line` suffix parse.
        let url = format!(
            "jetbrains://{tool}/navigate/reference?project={}&path={}",
            crate::util::percent_encode(&project_name(&project), b""),
            crate::util::percent_encode(&path, b"/:")
        );
        return crate::os::open_url(url);
    }

    // VS Code family and the rest: the CLI open handles jump-to-line and reuses
    // the running window (`-r`), which is a single-project-window model.
    ide_open(command, Some(file), line)
}

#[cfg(test)]
mod tests {
    use super::{
        census, editor_covers_project, exe_basename, family, ide_kinds, open_args, open_style,
        project_name, protocol_id, ranked_editor_ids, relative_path, source_content,
        suggestible_editor_ids, OpenStyle, ProjectKind,
    };

    #[test]
    fn coverage_ranking_prefers_a_specialist_for_one_language() {
        let source_bytes = std::collections::BTreeMap::from([(ProjectKind::Rust, 10_000)]);
        let ranked = ranked_editor_ids(&source_bytes, &[ProjectKind::Rust]);
        assert_eq!(ranked.first().map(String::as_str), Some("rustrover"));
    }

    #[test]
    fn coverage_ranking_requires_full_coverage_for_a_hybrid() {
        let source_bytes = std::collections::BTreeMap::from([
            (ProjectKind::Web, 9_000),
            (ProjectKind::Rust, 1_000),
        ]);
        let ranked = ranked_editor_ids(&source_bytes, &[ProjectKind::Web, ProjectKind::Rust]);
        assert_eq!(ranked.first().map(String::as_str), Some("vscode"));
        assert!(
            ranked.iter().position(|id| id == "vscode")
                < ranked.iter().position(|id| id == "webstorm")
        );
    }

    #[test]
    fn a_web_rule_requires_complete_language_coverage() {
        let web_monorepo = std::collections::BTreeMap::from([(ProjectKind::Web, 10_000)]);
        assert!(editor_covers_project(
            "webstorm",
            &web_monorepo,
            &[ProjectKind::Web]
        ));

        let web_rust_project = std::collections::BTreeMap::from([
            (ProjectKind::Web, 9_000),
            (ProjectKind::Rust, 1_000),
        ]);
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
        let web_rust_project = std::collections::BTreeMap::from([
            (ProjectKind::Web, 9_000),
            (ProjectKind::Rust, 1_000),
        ]);
        let suggested =
            suggestible_editor_ids(&web_rust_project, &[ProjectKind::Web, ProjectKind::Rust]);
        assert_eq!(suggested.first().map(String::as_str), Some("vscode"));
        assert!(!suggested.iter().any(|id| id == "webstorm"));
        assert!(!suggested.iter().any(|id| id == "androidstudio"));
    }

    #[test]
    fn generated_content_sits_out_the_census_without_a_path_rule() {
        let dir = std::env::temp_dir().join(format!("pade-generated-{}", std::process::id()));
        let core = dir.join("src-tauri");
        std::fs::create_dir_all(&core).expect("test dirs");

        let web_src = "const value = 1;\n".repeat(200);
        std::fs::write(dir.join("app.ts"), &web_src).expect("web file");
        let rust_src = "fn main() {}\n".repeat(260);
        std::fs::write(core.join("main.rs"), &rust_src).expect("rust file");
        std::fs::write(dir.join("bundle.unknown"), vec![b'a'; 4_000_000]).expect("bundle");

        assert!(
            source_content(&dir.join("bundle.unknown"))
                .expect("bundle readable")
                .generated
        );
        let totals = census(&dir);
        assert_eq!(totals.get(&ProjectKind::Web), Some(&(web_src.len() as u64)));
        assert_eq!(
            totals.get(&ProjectKind::Rust),
            Some(&(rust_src.len() as u64))
        );

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn the_census_honors_git_attributes_and_ignores_untracked_junk() {
        let dir = std::env::temp_dir().join(format!("pade-gittracked-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("test dir");

        let git = |args: &[&str]| {
            crate::util::command("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
        };
        // A machine without git skips this rather than failing the suite.
        let Ok(init) = git(&["init", "-q"]) else {
            std::fs::remove_dir_all(&dir).ok();
            return;
        };
        if !init.status.success() {
            std::fs::remove_dir_all(&dir).ok();
            return;
        }

        let web_src = "const value = 1;\n".repeat(400);
        let overridden_src = "m".repeat(20_000);
        let rust_src = "fn main() {}\n".repeat(500);
        std::fs::write(dir.join("app.ts"), &web_src).expect("web file");
        std::fs::write(dir.join("custom.source"), &rust_src).expect("rust override");
        std::fs::write(dir.join("generated.ts"), "x".repeat(20_000)).expect("generated file");
        std::fs::write(dir.join("manual.ts"), &overridden_src).expect("manual file");
        std::fs::write(
            dir.join(".gitattributes"),
            "generated.ts linguist-generated\ncustom.source linguist-language=Rust\nmanual.ts -linguist-generated\n",
        )
        .expect("attributes");
        let add = git(&["add", "."]).expect("git add");
        if !add.status.success() {
            std::fs::remove_dir_all(&dir).ok();
            return;
        }

        std::fs::write(dir.join("bundle.js"), "x = 1;\n".repeat(120_000)).expect("untracked blob");
        let totals = census(&dir);
        assert_eq!(
            totals.get(&ProjectKind::Web),
            Some(&((web_src.len() + overridden_src.len()) as u64))
        );
        assert_eq!(
            totals.get(&ProjectKind::Rust),
            Some(&(rust_src.len() as u64))
        );

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn a_nested_project_census_does_not_expand_to_its_git_root() {
        let dir = std::env::temp_dir().join(format!("pade-git-scope-{}", std::process::id()));
        let nested = dir.join("core");
        std::fs::create_dir_all(&nested).expect("test dirs");

        let git = |args: &[&str]| {
            crate::util::command("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
        };
        let Ok(init) = git(&["init", "-q"]) else {
            std::fs::remove_dir_all(&dir).ok();
            return;
        };
        if !init.status.success() {
            std::fs::remove_dir_all(&dir).ok();
            return;
        }

        std::fs::write(dir.join("app.ts"), "const app = 1;\n").expect("web source");
        let rust_src = "fn main() {}\n";
        std::fs::write(nested.join("main.rs"), rust_src).expect("rust source");
        let add = git(&["add", "."]).expect("git add");
        if !add.status.success() {
            std::fs::remove_dir_all(&dir).ok();
            return;
        }

        let totals = census(&nested);
        assert_eq!(
            totals.get(&ProjectKind::Rust),
            Some(&(rust_src.len() as u64))
        );
        assert!(!totals.contains_key(&ProjectKind::Web));

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn exe_basename_strips_extension_and_lowercases() {
        assert_eq!(exe_basename("C:\\Program Files\\Code.exe"), "code");
        assert_eq!(exe_basename("/usr/bin/nvim"), "nvim");
        assert_eq!(exe_basename("notepad++.exe"), "notepad++");
    }

    #[test]
    fn family_maps_a_jetbrains_editor_to_its_protocol() {
        let webstorm = family("webstorm64").expect("supported");
        assert!(matches!(webstorm.style, Some(OpenStyle::JetBrains)));
        assert_eq!(webstorm.protocol, Some("webstorm"));
    }

    #[test]
    fn family_rejects_a_non_editor_executable() {
        assert!(family("winrar").is_none());
    }

    #[test]
    fn console_editors_are_flagged_terminal_but_gui_ones_are_not() {
        assert!(family("nvim").expect("supported").terminal);
        assert!(family("vi").expect("supported").terminal);
        assert!(!family("code").expect("supported").terminal);
        assert!(!family("gvim").expect("supported").terminal);
    }

    #[test]
    fn new_jetbrains_registry_entries_resolve_style_and_protocol() {
        for command in ["rider", "clion", "phpstorm", "rubymine"] {
            assert!(
                matches!(open_style(command), Some(OpenStyle::JetBrains)),
                "{command} should open JetBrains-style"
            );
            assert_eq!(protocol_id(command), Some(command));
        }
    }

    #[test]
    fn kinds_list_cpp_before_dotnet_and_android_first() {
        let kinds: Vec<String> = ide_kinds().into_iter().map(|info| info.kind).collect();
        assert_eq!(kinds.first().map(String::as_str), Some("android"));
        let position = |kind: &str| {
            kinds
                .iter()
                .position(|k| k == kind)
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
        assert_eq!(dotnet.signals, ["global.json", "*.sln", "*.csproj"]);
    }

    #[test]
    fn open_style_resolves_an_added_editor_by_its_path() {
        assert!(matches!(
            open_style("C:\\Users\\me\\AppData\\Local\\Programs\\cursor\\Cursor.exe"),
            Some(OpenStyle::VsCode)
        ));
    }

    #[test]
    fn project_name_is_the_root_basename() {
        assert_eq!(project_name("C:\\repositories\\avi\\pade"), "pade");
        assert_eq!(project_name("/home/me/proj/"), "proj");
    }

    #[test]
    fn relative_path_strips_the_project_root() {
        assert_eq!(
            relative_path("C:\\repos\\pade", "C:\\repos\\pade\\src\\App.svelte"),
            "src/App.svelte"
        );
    }

    #[test]
    fn relative_path_keeps_an_outside_file_absolute() {
        assert_eq!(
            relative_path("C:/repos/pade", "D:/other/file.ts"),
            "D:/other/file.ts"
        );
    }

    #[test]
    fn vscode_style_reuses_the_window_and_jumps_to_the_line() {
        assert_eq!(
            open_args("code", "C:/p/file.ts".to_string(), Some(12)),
            ["-r", "-g", "C:/p/file.ts:12"]
        );
    }

    #[test]
    fn jetbrains_style_passes_the_line_flag_before_the_path() {
        assert_eq!(
            open_args("webstorm", "C:/p/file.ts".to_string(), Some(7)),
            ["--line", "7", "C:/p/file.ts"]
        );
    }

    #[test]
    fn visual_studio_edits_the_file_and_drops_the_line() {
        assert_eq!(
            open_args("devenv", "C:/p/file.cs".to_string(), Some(42)),
            ["/Edit", "C:/p/file.cs"]
        );
    }

    #[test]
    fn path_colon_style_suffixes_the_line_onto_the_path() {
        assert_eq!(
            open_args("zed", "C:/p/file.ts".to_string(), Some(3)),
            ["C:/p/file.ts:3"]
        );
    }

    #[test]
    fn a_bare_path_passes_through_without_a_line() {
        assert_eq!(open_args("code", "C:/p".to_string(), None), ["C:/p"]);
    }

    #[test]
    fn an_unknown_launcher_gets_the_plain_path_even_with_a_line() {
        assert_eq!(
            open_args("notepad", "C:/p/file.ts".to_string(), Some(9)),
            ["C:/p/file.ts"]
        );
    }
}
