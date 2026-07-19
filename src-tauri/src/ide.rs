//! External IDE integration — open the current project in the editor of choice.
//!
//! ADE is agentic-first, but you still reach for a full IDE sometimes. It
//! detects installed editors (by their CLI launcher) and opens the active
//! project directory in the one you pick.

use std::collections::{BTreeMap, BTreeSet};
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

    /// Whether source of `kind` is native to this declared project ecosystem.
    /// Android is a platform declaration whose implementation language is
    /// represented by the Java/Kotlin family; every language kind owns itself.
    fn owns_source_kind(self, kind: Self) -> bool {
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

    /// Among editors that cover the declared project kinds, prefer the
    /// least-general tool. It selects `RustRover` for a Rust project while a
    /// genuinely hybrid set of markers still leaves only a generalist eligible.
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
    /// Source-language families this product can cover for ranking purposes.
    coverage: EditorCoverage,
}

const REGISTRY: &[IdeDef] = &[
    IdeDef {
        id: "vscode",
        label: "VS Code",
        command: "code",
        style: OpenStyle::VsCode,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDef {
        id: "cursor",
        label: "Cursor",
        command: "cursor",
        style: OpenStyle::VsCode,
        coverage: EditorCoverage::EveryKind,
    },
    // The popular VS Code forks are generalists exactly like their parent —
    // same launcher conventions, same any-language reach.
    IdeDef {
        id: "antigravity",
        label: "Antigravity",
        command: "antigravity",
        style: OpenStyle::VsCode,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDef {
        id: "windsurf",
        label: "Windsurf",
        command: "windsurf",
        style: OpenStyle::VsCode,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDef {
        id: "vscodium",
        label: "VSCodium",
        command: "codium",
        style: OpenStyle::VsCode,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDef {
        id: "webstorm",
        label: "WebStorm",
        command: "webstorm",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web]),
    },
    IdeDef {
        id: "idea",
        label: "IntelliJ IDEA",
        command: "idea",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Java]),
    },
    IdeDef {
        id: "pycharm",
        label: "PyCharm",
        command: "pycharm",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Python]),
    },
    IdeDef {
        id: "goland",
        label: "GoLand",
        command: "goland",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Go]),
    },
    IdeDef {
        id: "rustrover",
        label: "RustRover",
        command: "rustrover",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Rust]),
    },
    IdeDef {
        id: "rider",
        label: "Rider",
        command: "rider",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Dotnet]),
    },
    IdeDef {
        id: "clion",
        label: "CLion",
        command: "clion",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Cpp]),
    },
    IdeDef {
        id: "phpstorm",
        label: "PhpStorm",
        command: "phpstorm",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Php]),
    },
    IdeDef {
        id: "rubymine",
        label: "RubyMine",
        command: "rubymine",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Web, ProjectKind::Ruby]),
    },
    IdeDef {
        id: "androidstudio",
        label: "Android Studio",
        command: "studio",
        style: OpenStyle::JetBrains,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Android, ProjectKind::Java]),
    },
    IdeDef {
        id: "zed",
        label: "Zed",
        command: "zed",
        style: OpenStyle::PathColon,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDef {
        id: "sublime",
        label: "Sublime Text",
        command: "subl",
        style: OpenStyle::PathColon,
        coverage: EditorCoverage::EveryKind,
    },
    IdeDef {
        id: "visualstudio",
        label: "Visual Studio",
        command: "devenv",
        style: OpenStyle::VisualStudio,
        coverage: EditorCoverage::Kinds(&[ProjectKind::Cpp, ProjectKind::Dotnet]),
    },
];

/// A project-kind marker — something in the project root that signals the kind,
/// and how it is probed on disk.
enum Marker {
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
    fn is_present(&self, cwd: &std::path::Path) -> bool {
        match self {
            Self::Named(name) => cwd.join(name).exists(),
            Self::JsonKey(name, key) => std::fs::read_to_string(cwd.join(name))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .is_some_and(|json| json.get(key).is_some()),
            Self::Extension(extension) => has_ext(cwd, extension),
        }
    }

    /// The marker as the UI displays it (`*.sln` for an extension probe).
    fn display(&self) -> String {
        match self {
            Self::Named(name) | Self::JsonKey(name, _) => (*name).to_string(),
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
            Marker::Named("settings.gradle.kts"),
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
            Marker::JsonKey("manifest.json", "manifest_version"),
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
        markers: &[
            Marker::Named("pom.xml"),
            Marker::Named("build.gradle"),
            Marker::Named("build.gradle.kts"),
        ],
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

/// One countable source file, retaining its location so project-root ownership
/// can distinguish core source from unrelated tools and documentation.
#[derive(Clone, Debug)]
struct SourceFileEvidence {
    path: std::path::PathBuf,
    kind: ProjectKind,
    bytes: u64,
}

/// Both aggregate language weights and the file-level evidence behind them.
/// Ranking consumes the totals; project-shape analysis consumes the locations.
#[derive(Default)]
struct SourceProfile {
    totals: BTreeMap<ProjectKind, u64>,
    files: Vec<SourceFileEvidence>,
}

impl SourceProfile {
    fn record(&mut self, file: SourceFileEvidence) {
        *self.totals.entry(file.kind).or_insert(0) += file.bytes;
        self.files.push(file);
    }
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

/// Classify one source file. Git attributes are the project author's explicit
/// statement, so they override intrinsic generated detection and extension
/// classification.
fn source_file_evidence(
    path: &std::path::Path,
    attributes: &LinguistAttributes,
) -> Option<SourceFileEvidence> {
    if attributes.excludes_from_census() {
        return None;
    }
    let content = source_content(path)?;
    let is_generated = attributes.generated.unwrap_or(content.generated);
    if content.binary || is_generated {
        return None;
    }
    let kind = attributes.language.or_else(|| {
        path.extension()
            .and_then(|value| value.to_str())
            .and_then(ProjectKind::from_extension)
    })?;
    let bytes = std::fs::metadata(path).map_or(0, |meta| meta.len());
    Some(SourceFileEvidence {
        path: path.to_path_buf(),
        kind,
        bytes,
    })
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
    profile: &mut SourceProfile,
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
                census_walk(&path, depth + 1, files_left, profile);
            }
            continue;
        }
        *files_left -= 1;
        if let Some(file) = source_file_evidence(&path, &LinguistAttributes::default()) {
            profile.record(file);
        }
    }
}

/// Source evidence for `cwd`. The file list is git's tracked set when `cwd` is a
/// repo — so untracked junk and ignored build output never sway the mix — and a
/// bounded filesystem walk otherwise. Bounded to [`CENSUS_MAX_FILES`] either
/// way so a huge tree can't stall a suggestion.
fn source_profile(cwd: &std::path::Path) -> SourceProfile {
    let mut profile = SourceProfile::default();
    if let Some(repository) = git_repository(cwd) {
        let paths: Vec<String> = repository
            .tracked_paths
            .iter()
            .take(CENSUS_MAX_FILES)
            .cloned()
            .collect();
        let attributes = git_linguist_attributes(&repository, &paths);
        for path in &paths {
            let absolute_path = repository.root.join(path);
            if let Some(file) = source_file_evidence(
                &absolute_path,
                attributes
                    .get(path)
                    .unwrap_or(&LinguistAttributes::default()),
            ) {
                profile.record(file);
            }
        }
    } else {
        let mut files_left = CENSUS_MAX_FILES;
        census_walk(cwd, 0, &mut files_left, &mut profile);
    }
    profile
}

/// Byte totals retained as a small compatibility wrapper for census-focused
/// tests; runtime suggestion uses [`source_profile`] once for both views.
#[cfg(test)]
fn census(cwd: &std::path::Path) -> BTreeMap<ProjectKind, u64> {
    source_profile(cwd).totals
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
/// editors with no line-jump CLI (the path is passed as-is). `terminal` marks
/// console editors (Neovim, Vim, Helix) that PADE opens inside a terminal tab
/// rather than spawning as a detached window.
struct Family {
    label: &'static str,
    style: Option<OpenStyle>,
    terminal: bool,
}

fn family(basename: &str) -> Option<Family> {
    // (label, jump-to-line style, runs-in-a-terminal)
    let (label, style, terminal) = match basename {
        "code" => ("VS Code", Some(OpenStyle::VsCode), false),
        "code - insiders" => ("VS Code Insiders", Some(OpenStyle::VsCode), false),
        "cursor" => ("Cursor", Some(OpenStyle::VsCode), false),
        "antigravity" => ("Antigravity", Some(OpenStyle::VsCode), false),
        "windsurf" => ("Windsurf", Some(OpenStyle::VsCode), false),
        "codium" | "vscodium" => ("VSCodium", Some(OpenStyle::VsCode), false),
        "zed" => ("Zed", Some(OpenStyle::PathColon), false),
        "sublime_text" | "subl" => ("Sublime Text", Some(OpenStyle::PathColon), false),
        "notepad++" => ("Notepad++", None, false),
        "gvim" => ("GVim", None, false),
        "nvim" => ("Neovim", None, true),
        "vim" | "vi" => ("Vim", None, true),
        "hx" => ("Helix", None, true),
        "webstorm" | "webstorm64" => ("WebStorm", Some(OpenStyle::JetBrains), false),
        "idea" | "idea64" => ("IntelliJ IDEA", Some(OpenStyle::JetBrains), false),
        "pycharm" | "pycharm64" => ("PyCharm", Some(OpenStyle::JetBrains), false),
        "goland" | "goland64" => ("GoLand", Some(OpenStyle::JetBrains), false),
        "rider" | "rider64" => ("Rider", Some(OpenStyle::JetBrains), false),
        "clion" | "clion64" => ("CLion", Some(OpenStyle::JetBrains), false),
        "phpstorm" | "phpstorm64" => ("PhpStorm", Some(OpenStyle::JetBrains), false),
        "rubymine" | "rubymine64" => ("RubyMine", Some(OpenStyle::JetBrains), false),
        "rustrover" | "rustrover64" => ("RustRover", Some(OpenStyle::JetBrains), false),
        _ => return None,
    };
    Some(Family {
        label,
        style,
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

/// Remove a user-added editor by its id (an `added-…` id from [`ide_add_editor`]).
/// Auto-detected editors carry no stored entry, so removing one is a no-op. Returns
/// the refreshed settings so every editor menu drops it at once.
#[tauri::command]
pub fn ide_remove_editor(id: String) -> Result<crate::workspace::Settings, String> {
    crate::workspace::remove_editor(&id)
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

/// One independently declared project unit within the active workspace. Keeping
/// its root lets source files belong to the nearest unit instead of one nested
/// package contaminating its parent profile.
#[derive(Clone, Debug)]
struct ProjectDeclaration {
    root: std::path::PathBuf,
    kinds: Vec<ProjectKind>,
}

/// Every marker-bearing project root in the workspace root or a direct child.
/// This depth captures small monorepos and hybrid applications while leaving a
/// nested support utility owned by its enclosing project unless explicitly
/// opened as the active workspace.
fn project_declarations(cwd: &std::path::Path) -> Vec<ProjectDeclaration> {
    probe_roots(cwd)
        .into_iter()
        .filter_map(|root| {
            let kinds: Vec<ProjectKind> = KIND_REGISTRY
                .iter()
                .filter(|definition| {
                    definition
                        .markers
                        .iter()
                        .any(|marker| marker.is_present(&root))
                })
                .map(|definition| definition.kind)
                .collect();
            (!kinds.is_empty()).then_some(ProjectDeclaration { root, kinds })
        })
        .collect()
}

/// Sniff the project kinds present in the current directory from the
/// [`KIND_REGISTRY`] marker files, in the registry's priority order. Markers
/// are probed in the root and one level down (see [`probe_roots`]); they provide
/// a second, source-free signal when a new project has no countable files yet.
fn kinds_from_declarations(declarations: &[ProjectDeclaration]) -> Vec<ProjectKind> {
    KIND_REGISTRY
        .iter()
        .filter(|definition| {
            declarations
                .iter()
                .any(|declaration| declaration.kinds.contains(&definition.kind))
        })
        .map(|definition| definition.kind)
        .collect()
}

/// Source evidence grouped by its first branch below a declared project root.
/// A root-level file uses an empty branch. Counts lead bytes so one large static
/// document cannot look more representative than a real source tree.
#[derive(Default)]
struct BranchEvidence {
    kinds: BTreeSet<ProjectKind>,
    by_kind: BTreeMap<ProjectKind, (usize, u64)>,
}

impl BranchEvidence {
    fn record(&mut self, file: &SourceFileEvidence) {
        self.kinds.insert(file.kind);
        let evidence = self.by_kind.entry(file.kind).or_insert((0, 0));
        evidence.0 += 1;
        evidence.1 += file.bytes;
    }

    fn declared_strength(&self, declared: ProjectKind) -> Option<(usize, u64)> {
        self.by_kind
            .iter()
            .filter(|(source_kind, _)| declared.owns_source_kind(**source_kind))
            .map(|(_, evidence)| *evidence)
            .max()
    }
}

fn source_branch(
    declaration_root: &std::path::Path,
    file: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let relative = file.strip_prefix(declaration_root).ok()?;
    let mut components = relative.components();
    let first = components.next()?;
    if components.next().is_none() {
        return Some(std::path::PathBuf::new());
    }
    Some(std::path::PathBuf::from(first.as_os_str()))
}

fn declaration_owns_file(
    declaration: &ProjectDeclaration,
    file: &std::path::Path,
    declarations: &[ProjectDeclaration],
) -> bool {
    file.starts_with(&declaration.root)
        && !declarations.iter().any(|nested| {
            nested.root != declaration.root
                && nested.root.starts_with(&declaration.root)
                && file.starts_with(&nested.root)
        })
}

/// Required editor capabilities derived without framework names. Each declared
/// project unit selects the branch that best represents its native source. A
/// conventional `src` branch leads, then root-level source, then the branch with
/// the most native files. Every language co-located in that branch is required;
/// unrelated branches remain ancillary. With no declarations, every observed
/// language is required because there is no stronger ownership signal.
fn required_project_kinds(
    profile: &SourceProfile,
    declarations: &[ProjectDeclaration],
) -> Vec<ProjectKind> {
    if declarations.is_empty() {
        return profile.totals.keys().copied().collect();
    }

    let mut required: BTreeSet<ProjectKind> = declarations
        .iter()
        .flat_map(|declaration| declaration.kinds.iter().copied())
        .collect();

    for declaration in declarations {
        let mut branches: BTreeMap<std::path::PathBuf, BranchEvidence> = BTreeMap::new();
        for file in profile
            .files
            .iter()
            .filter(|file| declaration_owns_file(declaration, &file.path, declarations))
        {
            let Some(branch) = source_branch(&declaration.root, &file.path) else {
                continue;
            };
            branches.entry(branch).or_default().record(file);
        }

        for declared in &declaration.kinds {
            let selected = branches
                .iter()
                .filter_map(|(branch, evidence)| {
                    let (files, bytes) = evidence.declared_strength(*declared)?;
                    let conventional_source = branch == std::path::Path::new("src");
                    let root_level = branch.as_os_str().is_empty();
                    Some(((conventional_source, root_level, files, bytes), evidence))
                })
                .max_by_key(|(strength, _)| *strength);
            if let Some((_, evidence)) = selected {
                required.extend(&evidence.kinds);
            }
        }
    }

    KIND_REGISTRY
        .iter()
        .map(|definition| definition.kind)
        .filter(|kind| required.contains(kind))
        .collect()
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

/// Rank each registered editor against the project's independent evidence.
/// Required kinds from declarations and their main source branches lead;
/// capability breadth breaks their tie, so a specialist is not displaced by
/// unrelated documentation or a small support tool. Source coverage leads only
/// for an unmarked folder and otherwise breaks ties between equally specific
/// editors. Registry order remains stable for equivalent editor families.
fn ranked_editor_ids(
    source_bytes: &BTreeMap<ProjectKind, u64>,
    required_kinds: &[ProjectKind],
) -> Vec<String> {
    let has_evidence = !source_bytes.is_empty() || !required_kinds.is_empty();
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
        let left_required = required_kinds
            .iter()
            .filter(|kind| left.coverage.supports(**kind))
            .count();
        let right_required = required_kinds
            .iter()
            .filter(|kind| right.coverage.supports(**kind))
            .count();
        if required_kinds.is_empty() {
            right_source
                .cmp(&left_source)
                .then_with(|| left.coverage.breadth().cmp(&right.coverage.breadth()))
        } else {
            right_required
                .cmp(&left_required)
                .then_with(|| left.coverage.breadth().cmp(&right.coverage.breadth()))
                .then_with(|| right_source.cmp(&left_source))
        }
    });
    editors.iter().map(|editor| editor.id.to_string()).collect()
}

/// Whether a known editor covers the project's required capabilities. Declared
/// project kinds plus languages in their main source branch distinguish a real
/// hybrid from incidental tracked source such as documentation and support
/// tools. An unmarked folder requires every source kind the census found.
/// User-added editors lack a capability declaration, so their explicit rule is
/// retained rather than guessing what the user's installation supports.
fn editor_covers_project(
    id: &str,
    source_bytes: &BTreeMap<ProjectKind, u64>,
    required_kinds: &[ProjectKind],
) -> bool {
    let has_evidence = !source_bytes.is_empty() || !required_kinds.is_empty();
    REGISTRY
        .iter()
        .find(|editor| editor.id == id)
        .is_none_or(|editor| {
            if !has_evidence {
                return matches!(editor.coverage, EditorCoverage::EveryKind);
            }
            if required_kinds.is_empty() {
                source_bytes
                    .keys()
                    .all(|kind| editor.coverage.supports(*kind))
            } else {
                required_kinds
                    .iter()
                    .all(|kind| editor.coverage.supports(*kind))
            }
        })
}

/// The compatible editor subset, preserving the coverage ranking. The menu is
/// an offer of editors that can work on the current project, not every installed
/// editor that happens to launch; a zero-coverage specialist is omitted.
fn suggestible_editor_ids(
    source_bytes: &BTreeMap<ProjectKind, u64>,
    required_kinds: &[ProjectKind],
) -> Vec<String> {
    ranked_editor_ids(source_bytes, required_kinds)
        .into_iter()
        .filter(|id| editor_covers_project(id, source_bytes, required_kinds))
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

/// Primary detected kind for each of `paths` — the marker-declared kind highest
/// in the registry's priority order — for the switcher's per-project language
/// logo. Paths with no recognised markers are omitted, so the caller falls back
/// to a folder glyph. Marker probing only (root + one level down), so it stays
/// cheap enough to run for a menuful of recent projects at once.
#[tauri::command]
pub fn ide_project_kinds(paths: Vec<String>) -> BTreeMap<String, String> {
    paths
        .into_iter()
        .filter_map(|path| {
            let project_path = std::path::Path::new(&path);
            let kind = kinds_from_declarations(&project_declarations(project_path))
                .into_iter()
                .next()?;
            // A web project that uses TypeScript shows the TS badge, not JavaScript.
            let icon_key = if kind == ProjectKind::Web && uses_typescript(project_path) {
                "typescript".to_string()
            } else {
                kind.as_str().to_string()
            };
            Some((path, icon_key))
        })
        .collect()
}

/// Whether a web project uses TypeScript — a `tsconfig.json`, or a `.ts`/`.tsx`
/// source in the project root, a direct child, or a `src` dir. Narrows the
/// per-project switcher icon from the JavaScript badge to the TypeScript one.
fn uses_typescript(cwd: &std::path::Path) -> bool {
    probe_roots(cwd).iter().any(|root| {
        root.join("tsconfig.json").exists()
            || has_ext(root, "ts")
            || has_ext(root, "tsx")
            || has_ext(&root.join("src"), "ts")
            || has_ext(&root.join("src"), "tsx")
    })
}

/// An id is worth suggesting only if it's actually launchable — a registry
/// launcher on PATH, or a user-added editor (its stored path is the launcher).
fn is_installed(id: &str) -> bool {
    if id.starts_with("added-") {
        return added_editors().iter().any(|e| e.id == id);
    }
    lookup(id).is_some_and(|i| is_on_path(&i.command))
}

/// The user-configured editor ids that lead the suggestion ranking, in winning
/// order. An explicit per-project choice always wins — the user pointed at that
/// editor for this very project, so no coverage rule may displace it. The
/// primary-kind rule and the fallback follow, but only while they cover the
/// project's required kinds.
fn preference_chain(
    choice: Option<String>,
    rule: Option<String>,
    fallback: Option<String>,
    covers: impl Fn(&str) -> bool,
) -> Vec<String> {
    choice
        .into_iter()
        .chain(rule.into_iter().chain(fallback).filter(|id| covers(id)))
        .collect()
}

/// Installed IDEs ranked for `cwd`, best match first. The caller supplies the
/// active project explicitly because multiple PADE windows and workspace
/// switches do not share one reliable process working directory. This command
/// is the single source of the project's editor: every surface (top-bar
/// launcher, Change Feed reveal, picker rows) treats its first entry as *the*
/// editor. An explicit per-project pick ([`ide_choose_editor`]) leads; then the
/// editor-rules engine, which takes precedence only when its editor covers
/// every required kind derived from project declarations and source ownership —
/// a user rule for the primary declared kind first, then the configured
/// fallback, then evidence-weighted editor coverage. No dominant-language
/// threshold or framework-specific detection is involved.
#[tauri::command]
pub fn ide_suggest(cwd: String) -> Result<Vec<Ide>, String> {
    let canonical_project = crate::workspace::canonical_path(&cwd);
    let cwd = std::path::Path::new(&cwd);
    if !cwd.is_dir() {
        return Err("project directory does not exist".to_string());
    }
    let declarations = project_declarations(cwd);
    let kinds = kinds_from_declarations(&declarations);
    let profile = source_profile(cwd);
    let required_kinds = required_project_kinds(&profile, &declarations);
    let prefs = crate::workspace::load().prefs;

    // 1) Explicit per-project pick, 2) compatible primary-kind rule,
    // 3) compatible fallback, 4) coverage ranking.
    let choice = prefs.ide_project_choices.get(&canonical_project).cloned();
    let rule = kinds
        .first()
        .and_then(|kind| prefs.ide_rules.get(kind.as_str()).cloned());
    let configured = preference_chain(choice, rule, prefs.ide_fallback, |id| {
        editor_covers_project(id, &profile.totals, &required_kinds)
    });
    let auto = suggestible_editor_ids(&profile.totals, &required_kinds);

    let mut ordered: Vec<String> = Vec::new();
    for id in configured.into_iter().chain(auto) {
        let is_new_and_installed = !ordered.contains(&id) && is_installed(&id);
        if is_new_and_installed {
            ordered.push(id);
        }
    }
    Ok(ordered.iter().filter_map(|id| lookup(id)).collect())
}

/// Remember an explicit editor pick for the project at `cwd`. The pick becomes
/// the project's editor everywhere at once, because every surface resolves
/// through [`ide_suggest`], which puts it first. Only a known editor id (a
/// registry entry or a user-added editor) is persisted.
#[tauri::command]
pub fn ide_choose_editor(cwd: String, id: String) -> Result<crate::workspace::Settings, String> {
    let is_known_editor =
        REGISTRY.iter().any(|editor| editor.id == id) || added_editors().iter().any(|e| e.id == id);
    if !is_known_editor {
        return Err(format!("unknown editor id \u{201c}{id}\u{201d}"));
    }
    crate::workspace::set_project_editor(&cwd, &id)
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

/// Reveal a file in the given editor, jumping to `line` when given. This routes
/// through the same launcher as "Open in editor" ([`ide_open`]): every editor
/// family's launcher (`JetBrains` `--line n file`, VS Code `-r -g file:line`,
/// Zed/Sublime `file:line`) both **starts a cold IDE** and hands the file to an
/// already-running one, so a reveal opens the file at the line whether or not
/// the editor was running. The owning project window is resolved by the launcher
/// from the file's own path; `_project` rides the IPC contract but isn't needed
/// here.
#[tauri::command]
pub fn ide_open_file(
    command: String,
    _project: String,
    file: String,
    line: Option<u32>,
) -> Result<(), String> {
    ide_open(command, Some(file), line)
}

#[cfg(test)]
mod tests {
    use super::{
        census, editor_covers_project, exe_basename, family, ide_kinds, open_args, open_style,
        preference_chain, project_declarations, ranked_editor_ids, required_project_kinds,
        source_content, suggestible_editor_ids, EditorCoverage, OpenStyle, ProjectDeclaration,
        ProjectKind, SourceFileEvidence, SourceProfile, REGISTRY,
    };

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
        let source_bytes = std::collections::BTreeMap::from([
            (ProjectKind::Web, 9_000),
            (ProjectKind::Rust, 1_000),
        ]);
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

    #[test]
    fn a_browser_manifest_declares_web_without_a_package_manifest() {
        let dir =
            std::env::temp_dir().join(format!("pade-browser-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("test dir");
        std::fs::write(dir.join("manifest.json"), "{\"manifest_version\":3}")
            .expect("browser manifest");

        let declarations = project_declarations(&dir);
        assert!(declarations
            .iter()
            .any(|declaration| declaration.kinds.contains(&ProjectKind::Web)));

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn an_unrelated_json_manifest_does_not_declare_web() {
        let dir =
            std::env::temp_dir().join(format!("pade-generic-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("test dir");
        std::fs::write(dir.join("manifest.json"), "{\"name\":\"artifact\"}")
            .expect("generic manifest");

        assert!(!project_declarations(&dir)
            .iter()
            .any(|declaration| declaration.kinds.contains(&ProjectKind::Web)));

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn unmarked_mixed_source_still_requires_a_generalist() {
        let source_bytes = std::collections::BTreeMap::from([
            (ProjectKind::Web, 9_000),
            (ProjectKind::Rust, 1_000),
        ]);
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

        assert!(!suggested.is_empty());
        assert!(suggested.iter().all(|id| is_general_purpose_editor(id)));
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
    fn family_maps_a_jetbrains_editor_to_its_open_style() {
        let webstorm = family("webstorm64").expect("supported");
        assert!(matches!(webstorm.style, Some(OpenStyle::JetBrains)));
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
