//! External IDE integration — open the current project in the editor of choice.
//!
//! ADE is agentic-first, but you still reach for a full IDE sometimes. It
//! detects installed editors (by their CLI launcher) and opens the active
//! project directory in the one you pick.

use std::collections::BTreeMap;
use std::io::Read;

use serde::Serialize;

use crate::util::is_on_path;

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
}

const REGISTRY: &[IdeDef] = &[
    IdeDef {
        id: "vscode",
        label: "VS Code",
        command: "code",
        style: OpenStyle::VsCode,
        protocol: None,
    },
    IdeDef {
        id: "cursor",
        label: "Cursor",
        command: "cursor",
        style: OpenStyle::VsCode,
        protocol: None,
    },
    // The popular VS Code forks are generalists exactly like their parent —
    // same launcher conventions, same any-language reach.
    IdeDef {
        id: "antigravity",
        label: "Antigravity",
        command: "antigravity",
        style: OpenStyle::VsCode,
        protocol: None,
    },
    IdeDef {
        id: "windsurf",
        label: "Windsurf",
        command: "windsurf",
        style: OpenStyle::VsCode,
        protocol: None,
    },
    IdeDef {
        id: "vscodium",
        label: "VSCodium",
        command: "codium",
        style: OpenStyle::VsCode,
        protocol: None,
    },
    IdeDef {
        id: "webstorm",
        label: "WebStorm",
        command: "webstorm",
        style: OpenStyle::JetBrains,
        protocol: Some("webstorm"),
    },
    IdeDef {
        id: "idea",
        label: "IntelliJ IDEA",
        command: "idea",
        style: OpenStyle::JetBrains,
        protocol: Some("idea"),
    },
    IdeDef {
        id: "pycharm",
        label: "PyCharm",
        command: "pycharm",
        style: OpenStyle::JetBrains,
        protocol: Some("pycharm"),
    },
    IdeDef {
        id: "goland",
        label: "GoLand",
        command: "goland",
        style: OpenStyle::JetBrains,
        protocol: Some("goland"),
    },
    IdeDef {
        id: "rustrover",
        label: "RustRover",
        command: "rustrover",
        style: OpenStyle::JetBrains,
        protocol: Some("rustrover"),
    },
    IdeDef {
        id: "rider",
        label: "Rider",
        command: "rider",
        style: OpenStyle::JetBrains,
        protocol: Some("rider"),
    },
    IdeDef {
        id: "clion",
        label: "CLion",
        command: "clion",
        style: OpenStyle::JetBrains,
        protocol: Some("clion"),
    },
    IdeDef {
        id: "phpstorm",
        label: "PhpStorm",
        command: "phpstorm",
        style: OpenStyle::JetBrains,
        protocol: Some("phpstorm"),
    },
    IdeDef {
        id: "rubymine",
        label: "RubyMine",
        command: "rubymine",
        style: OpenStyle::JetBrains,
        protocol: Some("rubymine"),
    },
    IdeDef {
        id: "androidstudio",
        label: "Android Studio",
        command: "studio",
        style: OpenStyle::JetBrains,
        protocol: Some("studio"),
    },
    IdeDef {
        id: "zed",
        label: "Zed",
        command: "zed",
        style: OpenStyle::PathColon,
        protocol: None,
    },
    IdeDef {
        id: "sublime",
        label: "Sublime Text",
        command: "subl",
        style: OpenStyle::PathColon,
        protocol: None,
    },
    IdeDef {
        id: "visualstudio",
        label: "Visual Studio",
        command: "devenv",
        style: OpenStyle::VisualStudio,
        protocol: None,
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

/// One project kind PADE recognises. A single row wires everything a kind
/// needs — its detection markers, UI label, and purpose-built editors — so
/// adding a language is one entry here. Table order is the detection priority
/// (the first matching kind is a project's primary kind) and the UI's render
/// order.
struct KindDef {
    kind: &'static str,
    label: &'static str,
    markers: &'static [Marker],
    /// The IDEs purpose-built for this kind, best first. Generalist editors
    /// are appended to every suggestion list as a fallback.
    preferred: &'static [&'static str],
}

const KIND_REGISTRY: &[KindDef] = &[
    // Android is checked first: an Android project is also "web"/"java"-ish,
    // but Android Studio is the right call when its markers are there.
    KindDef {
        kind: "android",
        label: "Android",
        markers: &[
            Marker::Named("AndroidManifest.xml"),
            Marker::Named("gradlew"),
            Marker::Named("settings.gradle"),
        ],
        preferred: &["androidstudio", "idea"],
    },
    KindDef {
        kind: "web",
        label: "Web / JavaScript",
        markers: &[
            Marker::Named("package.json"),
            Marker::Named("tsconfig.json"),
            Marker::Named("index.html"),
        ],
        preferred: &["webstorm", "vscode", "cursor"],
    },
    KindDef {
        kind: "python",
        label: "Python",
        markers: &[
            Marker::Named("pyproject.toml"),
            Marker::Named("requirements.txt"),
            Marker::Named("setup.py"),
        ],
        preferred: &["pycharm", "vscode"],
    },
    KindDef {
        kind: "php",
        label: "PHP",
        markers: &[Marker::Named("composer.json")],
        preferred: &["phpstorm", "vscode"],
    },
    KindDef {
        kind: "ruby",
        label: "Ruby",
        markers: &[Marker::Named("Gemfile")],
        preferred: &["rubymine", "vscode"],
    },
    KindDef {
        kind: "go",
        label: "Go",
        markers: &[Marker::Named("go.mod")],
        preferred: &["goland", "vscode"],
    },
    KindDef {
        kind: "rust",
        label: "Rust",
        markers: &[Marker::Named("Cargo.toml")],
        preferred: &["rustrover", "zed", "vscode"],
    },
    KindDef {
        kind: "java",
        label: "Java",
        markers: &[Marker::Named("pom.xml"), Marker::Named("build.gradle")],
        preferred: &["idea"],
    },
    // C/C++ before .NET: a Visual C++ solution also carries a .sln, and marker
    // files can't reliably separate C from C++, so one "cpp" kind covers both.
    KindDef {
        kind: "cpp",
        label: "C / C++",
        markers: &[
            Marker::Named("CMakeLists.txt"),
            Marker::Named("meson.build"),
            Marker::Extension("vcxproj"),
        ],
        preferred: &["visualstudio", "clion", "vscode"],
    },
    KindDef {
        kind: "dotnet",
        label: "C# / .NET",
        markers: &[
            Marker::Named("global.json"),
            Marker::Extension("sln"),
            Marker::Extension("csproj"),
        ],
        preferred: &["visualstudio", "rider", "vscode"],
    },
];
const GENERALISTS: &[&str] = &[
    "vscode",
    "cursor",
    "antigravity",
    "windsurf",
    "vscodium",
    "zed",
    "sublime",
];

/// Polyglot IDEs — they cover several kinds at once (IDEA Ultimate ships the
/// full web stack next to the JVM and plugin languages), so a true hybrid
/// ranks them right after the generalists, still ahead of any single-language
/// specialist. (`JetBrains`' own compare matrix: `WebStorm` has no Rust at
/// all; IDEA is "a superset of most `IntelliJ` platform-based IDEs".)
const POLYGLOTS: &[&str] = &["idea"];

/// Extension → project kind for the byte census — only extensions that
/// unambiguously signal one registry kind (Linguist resolves ambiguous ones
/// with content heuristics; PADE just leaves them out of the weighing).
const CENSUS_EXTENSIONS: &[(&str, &str)] = &[
    ("ts", "web"),
    ("tsx", "web"),
    ("js", "web"),
    ("jsx", "web"),
    ("mjs", "web"),
    ("svelte", "web"),
    ("vue", "web"),
    ("html", "web"),
    ("css", "web"),
    ("scss", "web"),
    ("rs", "rust"),
    ("py", "python"),
    ("go", "go"),
    ("php", "php"),
    ("rb", "ruby"),
    ("java", "java"),
    ("kt", "java"),
    ("cs", "dotnet"),
    ("c", "cpp"),
    ("cc", "cpp"),
    ("cpp", "cpp"),
    ("h", "cpp"),
    ("hpp", "cpp"),
];

/// Census walk bounds — enough to weigh a real repo, bounded so the suggestion
/// never stalls on a huge working tree.
const CENSUS_MAX_DEPTH: usize = 5;
const CENSUS_MAX_FILES: usize = 4000;

/// A byte census of a project's languages must ignore *generated* files, or a
/// single vendored bundle — a 4&nbsp;MB minified `common.js` — drowns out the real
/// source and misreads the language mix (a Tauri hybrid reads as pure web). This
/// is the half of Linguist's method the marker/byte census alone skips: Linguist
/// excludes generated content before weighing. PADE judges a file by what it
/// *contains*, never by its name — a minified blob is recognised by lines that
/// run implausibly long for hand-written code, whatever a tool called the file.
const CENSUS_CONTENT_CHECK_MIN_BYTES: u64 = 64 * 1024;
const MINIFIED_SAMPLE_BYTES: usize = 8 * 1024;
const MINIFIED_AVG_LINE_BYTES: usize = 400;

/// Whether `path` looks generated/minified and should sit out the census. Cheap:
/// only files past [`CENSUS_CONTENT_CHECK_MIN_BYTES`] are read (smaller files
/// barely move the census), and only their head is sampled for the near-absence
/// of newlines that marks minified output. The decision is purely by content —
/// no filename or extension patterns.
fn is_generated(path: &std::path::Path, size: u64) -> bool {
    if size < CENSUS_CONTENT_CHECK_MIN_BYTES {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; MINIFIED_SAMPLE_BYTES];
    let Ok(read) = file.read(&mut head) else {
        return false;
    };
    // Minified output runs almost without line breaks — its first "line" spans
    // the whole file. A first line longer than any hand-written one (or a sample
    // with no break at all) marks it generated. `position` stops at the first
    // newline, so there's no scanning the whole buffer.
    match head[..read].iter().position(|&b| b == b'\n') {
        Some(first_break) => first_break > MINIFIED_AVG_LINE_BYTES,
        None => read > MINIFIED_AVG_LINE_BYTES,
    }
}

/// Byte share (percent) one kind needs, of all census-counted bytes across the
/// detected kinds, to count as dominant. A "hybrid" in markers only — a web
/// app with one Rust helper — still reads as its dominant kind; below the bar
/// the repo is a genuine hybrid. (No published cutoff exists; this is PADE's
/// tuning, chosen so its own repo — a real Tauri hybrid — stays hybrid.)
const DOMINANT_BYTE_PERCENT: u128 = 85;

/// The census kind for a file extension, or `None` for extensions that don't
/// unambiguously map to one registry kind (they sit out the weighing).
fn census_kind(extension: &str) -> Option<&'static str> {
    CENSUS_EXTENSIONS
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(extension))
        .map(|(_, kind)| *kind)
}

/// Fold one file's on-disk bytes into the running per-kind totals — the byte
/// weighting is Linguist's. Files with no census kind, and generated/minified
/// blobs, sit out.
fn weigh_file(path: &std::path::Path, totals: &mut BTreeMap<&'static str, u64>) {
    let Some(kind) = path
        .extension()
        .and_then(|value| value.to_str())
        .and_then(census_kind)
    else {
        return;
    };
    let bytes = std::fs::metadata(path).map_or(0, |meta| meta.len());
    if is_generated(path, bytes) {
        return;
    }
    *totals.entry(kind).or_insert(0) += bytes;
}

/// The repo's tracked files as absolute paths, or `None` when `cwd` isn't a git
/// repo (or git isn't installed). `git ls-files` lists exactly the tracked files
/// — never an untracked scratch file or gitignored build output — so the census
/// weighs the project's real source the way Linguist does: from what the repo
/// actually tracks, not from whatever happens to sit in the working tree. This
/// is what keeps a stray vendored bundle a contributor never committed from
/// rewriting the language mix.
fn git_tracked_files(cwd: &std::path::Path) -> Option<Vec<std::path::PathBuf>> {
    let output = crate::util::command("git")
        .arg("-C")
        .arg(cwd)
        .args(["ls-files", "-z"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    Some(
        listing
            .split('\0')
            .filter(|entry| !entry.is_empty())
            .map(|relative| cwd.join(relative))
            .collect(),
    )
}

/// Bounded filesystem-walk fallback for a folder that isn't a git repo: sum
/// census bytes under `dir`, skipping the hidden and dependency/build dirs the
/// marker probe skips (untracked build output physically lives here, so the
/// exclusions matter). A repo goes through [`git_tracked_files`] instead.
fn census_walk(
    dir: &std::path::Path,
    depth: usize,
    files_left: &mut usize,
    totals: &mut BTreeMap<&'static str, u64>,
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
        weigh_file(&path, totals);
    }
}

/// Byte totals per census kind for `cwd`. The file list is git's tracked set
/// when `cwd` is a repo — so untracked junk and ignored build output never sway
/// the mix — and a bounded filesystem walk otherwise. Bounded to
/// [`CENSUS_MAX_FILES`] either way so a huge tree can't stall a suggestion.
fn census(cwd: &std::path::Path) -> BTreeMap<&'static str, u64> {
    let mut totals = BTreeMap::new();
    if let Some(files) = git_tracked_files(cwd) {
        files
            .iter()
            .take(CENSUS_MAX_FILES)
            .for_each(|path| weigh_file(path, &mut totals));
    } else {
        let mut files_left = CENSUS_MAX_FILES;
        census_walk(cwd, 0, &mut files_left, &mut totals);
    }
    totals
}

/// The detected kind holding a clear byte majority of the census, or `None`
/// when the mix is genuinely hybrid (or nothing countable was found).
fn dominant_kind(cwd: &std::path::Path, kinds: &[&'static str]) -> Option<&'static str> {
    let totals = census(cwd);

    let counted: u64 = kinds
        .iter()
        .filter_map(|kind| totals.get(kind))
        .copied()
        .sum();
    if counted == 0 {
        return None;
    }

    kinds
        .iter()
        .find(|kind| {
            let bytes = totals.get(*kind).copied().unwrap_or(0);
            u128::from(bytes) * 100 >= u128::from(counted) * DOMINANT_BYTE_PERCENT
        })
        .copied()
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
/// are probed in the root and one level down (see [`probe_roots`]), so a
/// project's kinds are the union of what it contains — a web frontend with a
/// Rust core detects as both, and the multi-kind ranking in [`ide_suggest`]
/// then leads with editors that speak every side rather than one side's
/// specialist.
fn detect_kinds(cwd: &std::path::Path) -> Vec<&'static str> {
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
fn primary_kind(cwd: &std::path::Path) -> Option<&'static str> {
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

/// Editor ids to offer per editor-rules row, installed-only. Each recognised
/// project kind gets its [`KIND_REGISTRY`] preference list plus the generalists (so an
/// unrelated IDE — `WebStorm` for an Android project — is never offered); the
/// [`FALLBACK_KEY`] "any other folder" row gets only the generalists and any
/// user-added editors (a folder with no recognised kind wants a general editor,
/// not a language-specific IDE). The frontend maps these ids to its detected
/// editors.
#[tauri::command]
pub fn ide_kind_options() -> BTreeMap<String, Vec<String>> {
    let installed = ide_detect();
    let is_present = |id: &str| installed.iter().any(|editor| editor.id == id);
    let added_ids = installed
        .iter()
        .filter(|editor| editor.id.starts_with("added-"))
        .map(|editor| editor.id.clone());

    // General editors + the user's own added editors — suitable for any folder.
    let general = dedup_in_order(
        GENERALISTS
            .iter()
            .copied()
            .map(str::to_string)
            .chain(added_ids)
            .filter(|id| is_present(id))
            .collect(),
    );

    KIND_REGISTRY
        .iter()
        .map(|def| {
            let options = dedup_in_order(
                def.preferred
                    .iter()
                    .copied()
                    .map(str::to_string)
                    .filter(|id| is_present(id))
                    .chain(general.iter().cloned())
                    .collect(),
            );
            (def.kind.to_string(), options)
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
            kind: def.kind.to_string(),
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
/// editor-rules engine takes precedence: a user rule for the project's primary
/// kind is offered first, then the configured fallback, then the built-in
/// auto-ranking (kind preferences + generalists). A project with several entry
/// points (a monorepo — more than one detected kind) is usually better served by a
/// generalist than any one language's specialised IDE, so generalists lead the
/// auto-ranking there. Deduped, installed-only.
#[tauri::command]
pub fn ide_suggest() -> Result<Vec<Ide>, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let kinds = detect_kinds(&cwd);
    let prefs = crate::workspace::load().prefs;

    // 1) Explicit rule for the primary kind, 2) fallback, 3) auto-ranking.
    let rule = primary_kind(&cwd).and_then(|k| prefs.ide_rules.get(k).cloned());
    let configured = rule.into_iter().chain(prefs.ide_fallback);

    // Specialised IDEs for every detected kind, and the generalists. A
    // single-kind project leads with that kind's specialists. A multi-kind
    // project is weighed by the byte census first: one kind holding a clear
    // majority still leads with its specialists (a hybrid in markers only);
    // a genuine hybrid leads with the generalists and the polyglot IDEs —
    // no single-language specialist fits a repo that is really two languages.
    let preferred_of = |kind: &str| {
        KIND_REGISTRY
            .iter()
            .find(|def| def.kind == kind)
            .map_or::<&[&str], _>(&[], |def| def.preferred)
    };
    let specialized: Vec<&str> = kinds
        .iter()
        .flat_map(|k| preferred_of(k).iter().copied())
        .collect();
    let is_monorepo = kinds.len() > 1;
    let auto: Vec<String> = if is_monorepo {
        match dominant_kind(&cwd, &kinds) {
            Some(dominant) => preferred_of(dominant)
                .iter()
                .chain(GENERALISTS)
                .chain(POLYGLOTS)
                .chain(specialized.iter())
                .copied()
                .collect(),
            None => GENERALISTS
                .iter()
                .chain(POLYGLOTS)
                .chain(specialized.iter())
                .copied()
                .collect(),
        }
    } else {
        specialized
            .iter()
            .chain(GENERALISTS.iter())
            .copied()
            .collect::<Vec<&str>>()
    }
    .into_iter()
    .map(str::to_string)
    .collect();

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
        dominant_kind, exe_basename, family, ide_kinds, is_generated, open_args, open_style,
        project_name, protocol_id, relative_path, OpenStyle,
    };

    #[test]
    fn census_flags_a_dominant_kind_and_stays_hybrid_otherwise() {
        let dir = std::env::temp_dir().join(format!("pade-census-{}", std::process::id()));
        let core = dir.join("src-tauri");
        std::fs::create_dir_all(&core).expect("test dirs");

        // 9:1 bytes web:rust — the web side clearly dominates.
        std::fs::write(dir.join("app.ts"), vec![b'a'; 9_000]).expect("web file");
        std::fs::write(core.join("main.rs"), vec![b'a'; 1_000]).expect("rust file");
        assert_eq!(dominant_kind(&dir, &["web", "rust"]), Some("web"));

        // Balance the bytes — now it is a genuine hybrid, nobody dominates.
        std::fs::write(core.join("core.rs"), vec![b'a'; 8_000]).expect("rust file");
        assert_eq!(dominant_kind(&dir, &["web", "rust"]), None);

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn a_minified_bundle_sits_out_the_census_so_a_hybrid_stays_hybrid() {
        let dir = std::env::temp_dir().join(format!("pade-generated-{}", std::process::id()));
        let core = dir.join("src-tauri");
        std::fs::create_dir_all(&core).expect("test dirs");

        // Real hand-written source with ordinary short lines: a small web file
        // next to a comparable Rust core — a genuine hybrid on real code alone.
        let web_src = "const value = 1;\n".repeat(200);
        std::fs::write(dir.join("app.ts"), &web_src).expect("web file");
        std::fs::write(core.join("main.rs"), "fn main() {}\n".repeat(260)).expect("rust file");

        // A vendored, minified bundle: multi-megabyte, effectively one line. Left
        // in the census it would make web dominate ~99%; excluded, the repo stays
        // the hybrid its real source describes.
        std::fs::write(dir.join("common.js"), vec![b'a'; 4_000_000]).expect("bundle");

        assert!(is_generated(&dir.join("common.js"), 4_000_000));
        assert!(!is_generated(&dir.join("app.ts"), web_src.len() as u64));
        assert_eq!(dominant_kind(&dir, &["web", "rust"]), None);

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn the_census_weighs_only_git_tracked_source_not_untracked_junk() {
        let dir = std::env::temp_dir().join(format!("pade-gittracked-{}", std::process::id()));
        let core = dir.join("src-tauri");
        std::fs::create_dir_all(&core).expect("test dirs");

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

        // A balanced hybrid on the tracked source alone.
        std::fs::write(dir.join("app.ts"), "const value = 1;\n".repeat(400)).expect("web file");
        std::fs::write(core.join("main.rs"), "fn main() {}\n".repeat(500)).expect("rust file");
        git(&["add", "app.ts", "src-tauri/main.rs"]).expect("git add");

        // A large *untracked* web blob — it would tip the repo web-dominant if the
        // census weighed the working tree instead of git's tracked set.
        std::fs::write(dir.join("bundle.js"), "x = 1;\n".repeat(120_000)).expect("untracked blob");

        assert_eq!(dominant_kind(&dir, &["web", "rust"]), None);

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
