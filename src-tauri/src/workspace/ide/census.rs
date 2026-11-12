//! The source census: counts a project's tracked and untracked source by language, skipping generated and vendored files.

use super::declarations::IGNORED_PROBE_DIRECTORIES;
use super::editor_registry::ProjectKind;
use super::project_registry::language_named;
use super::project_registry::language_of_extension;
use super::project_registry::LanguageDefinition;
use crate::vcs::git_in;
use std::collections::BTreeMap;
use std::io::Read;
use std::io::Write;
use std::process::Stdio;

/// Census walk bounds - enough to weigh a real repo, bounded so the suggestion
/// never stalls on a huge working tree.
pub(super) const CENSUS_MAXIMUM_DEPTH: usize = 5;

pub(super) const CENSUS_MAXIMUM_FILES: usize = 4000;

/// The first 8 KiB is enough to identify binary, minified, and generator-marked
/// content without turning editor suggestion into a full repository scan.
pub(super) const SOURCE_SAMPLE_BYTES: usize = 8 * 1024;

pub(super) const MINIFIED_MINIMUM_SAMPLE_BYTES: usize = 1024;

pub(super) const MINIFIED_AVERAGE_LINE_BYTES: usize = 400;

pub(super) const LINGUIST_GENERATED: &str = "linguist-generated";

pub(super) const LINGUIST_VENDORED: &str = "linguist-vendored";

pub(super) const LINGUIST_DOCUMENTATION: &str = "linguist-documentation";

pub(super) const LINGUIST_DETECTABLE: &str = "linguist-detectable";

pub(super) const LINGUIST_LANGUAGE: &str = "linguist-language";

/// The values `git check-attr` prints for a set (`attr`) and unset (`-attr`)
/// attribute; anything else is a string value or "unspecified".
pub(super) const GIT_ATTRIBUTE_SET: &str = "set";

pub(super) const GIT_ATTRIBUTE_UNSET: &str = "unset";

pub(super) const LINGUIST_ATTRIBUTES: &[&str] = &[
    LINGUIST_GENERATED,
    LINGUIST_VENDORED,
    LINGUIST_DOCUMENTATION,
    LINGUIST_DETECTABLE,
    LINGUIST_LANGUAGE,
];

/// The Git-authored exclusions and classification that apply to one counted
/// file. `None` means the corresponding attribute was unspecified.
#[derive(Default)]
pub(super) struct LinguistAttributes {
    language: Option<&'static LanguageDefinition>,
    pub(super) generated: Option<bool>,
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
            GIT_ATTRIBUTE_SET => Some(true),
            GIT_ATTRIBUTE_UNSET => Some(false),
            _ => None,
        };
        match attribute {
            LINGUIST_GENERATED => self.generated = boolean,
            LINGUIST_VENDORED => self.vendored = boolean,
            LINGUIST_DOCUMENTATION => self.documentation = boolean,
            LINGUIST_DETECTABLE => self.detectable = boolean,
            LINGUIST_LANGUAGE => self.language = language_named(value),
            _ => {}
        }
    }
}

/// Intrinsic source properties used before language classification. Binary data
/// and generated output never influence editor coverage, whatever their path.
/// Carries the file size read off the already-open handle, so classification
/// never re-stats the path it just opened.
pub(super) struct SourceContent {
    binary: bool,
    pub(super) generated: bool,
    bytes: u64,
}

/// One countable source file, retaining its location so project-root ownership
/// can distinguish core source from unrelated tools and documentation.
#[derive(Clone, Debug)]
pub(super) struct SourceFileEvidence {
    pub(super) path: std::path::PathBuf,
    pub(super) kind: ProjectKind,
    /// The Linguist language within the kind, for the language bar.
    pub(super) language: &'static str,
    pub(super) bytes: u64,
}

/// Both aggregate language weights and the file-level evidence behind them.
/// Ranking consumes the per-kind totals, the language bar the per-language
/// ones; project-shape analysis consumes the locations.
#[derive(Default)]
pub(super) struct SourceProfile {
    pub(super) totals: BTreeMap<ProjectKind, u64>,
    pub(super) languages: BTreeMap<&'static str, u64>,
    pub(super) files: Vec<SourceFileEvidence>,
}

impl SourceProfile {
    pub(super) fn record(&mut self, file: SourceFileEvidence) {
        *self.totals.entry(file.kind).or_insert(0) += file.bytes;
        *self.languages.entry(file.language).or_insert(0) += file.bytes;
        self.files.push(file);
    }
}

/// Read the small content sample used to exclude non-source files. A source
/// file is classified only when its bytes were readable; guessing around a read
/// failure would make the result depend on a transient filesystem error.
pub(super) fn source_content(path: &std::path::Path) -> Option<SourceContent> {
    let mut file = std::fs::File::open(path).ok()?;
    let bytes = file.metadata().map_or(0, |metadata| metadata.len());
    let mut sample = [0u8; SOURCE_SAMPLE_BYTES];
    let read = file.read(&mut sample).ok()?;
    let sample = &sample[..read];
    let binary = sample.contains(&0);
    if binary {
        return Some(SourceContent {
            binary,
            generated: false,
            bytes,
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
    let minified = sample.len() >= MINIFIED_MINIMUM_SAMPLE_BYTES
        && average_line_bytes > MINIFIED_AVERAGE_LINE_BYTES;
    Some(SourceContent {
        binary,
        generated: declares_generated || minified,
        bytes,
    })
}

/// Classify one source file. Git attributes are the project author's explicit
/// statement, so they override intrinsic generated detection and extension
/// classification.
pub(super) fn source_file_evidence(
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
    let language = attributes.language.or_else(|| {
        path.extension()
            .and_then(|value| value.to_str())
            .and_then(language_of_extension)
    })?;
    Some(SourceFileEvidence {
        path: path.to_path_buf(),
        kind: language.kind,
        language: language.name,
        bytes: content.bytes,
    })
}

/// The Git repository root and the paths below PADE's active project directory
/// that the census counts: every tracked file, then every untracked file Git
/// does not ignore - what an agent has written but nobody has staged yet.
/// `--full-name` keeps those paths rooted at the repository, so the census can
/// join them safely after Git scopes the listing to the active folder.
pub(super) struct GitRepository {
    root: std::path::PathBuf,
    counted_paths: Vec<String>,
}

/// One `git ls-files -z --full-name` listing under `cwd`, or `None` when Git
/// cannot produce it.
pub(super) fn git_file_listing(cwd: &std::path::Path, arguments: &[&str]) -> Option<Vec<String>> {
    let output = git_in(cwd)
        .args(["ls-files", "--full-name", "-z"])
        .args(arguments)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// The untracked files Git does not ignore. Dependency and build folders are
/// excluded on top of the project's own ignore rules, so a repository whose
/// `.gitignore` forgot `node_modules` never lists (or counts) its contents.
/// Git cannot combine `--others` with `--recurse-submodules`, so untracked
/// files inside a submodule wait until they are tracked.
pub(super) fn untracked_file_listing(cwd: &std::path::Path) -> Vec<String> {
    let excluded_directories: Vec<String> = IGNORED_PROBE_DIRECTORIES
        .iter()
        .map(|directory| format!("--exclude={directory}/"))
        .collect();
    let mut arguments = vec!["--others", "--exclude-standard"];
    arguments.extend(excluded_directories.iter().map(String::as_str));
    git_file_listing(cwd, &arguments).unwrap_or_default()
}

/// `--recurse-submodules` lists each submodule's tracked files too - a
/// companion app kept as a submodule is part of the project's code, not one
/// opaque entry. Tracked paths come first, so the census bound trims untracked
/// files before tracked ones.
pub(super) fn git_repository(cwd: &std::path::Path) -> Option<GitRepository> {
    let root_output = git_in(cwd)
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
    let mut counted_paths = git_file_listing(cwd, &["--cached", "--recurse-submodules"])?;
    counted_paths.extend(untracked_file_listing(cwd));
    Some(GitRepository {
        root,
        counted_paths,
    })
}

/// Resolve the project's `.gitattributes` through Git itself, including nested
/// files and Git's path-matching semantics. The fallback map is empty if Git
/// cannot evaluate the attributes; the Git-listed census remains usable.
pub(super) fn git_linguist_attributes(
    repository_root: &std::path::Path,
    paths: &[String],
) -> BTreeMap<String, LinguistAttributes> {
    let Ok(mut child) = git_in(repository_root)
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
    // Git answers each path as it reads it, so the paths go in on their own
    // thread while this one drains the answers: writing them all first fills
    // the output pipe on a repository of any size, and then Git waits for us to
    // read while we wait for it to read - forever.
    let (wrote_all_paths, output) = std::thread::scope(|scope| {
        let writer = scope.spawn(move || {
            paths.iter().all(|path| {
                input
                    .write_all(path.as_bytes())
                    .and_then(|()| input.write_all(&[0]))
                    .is_ok()
            })
        });
        let output = child.wait_with_output();
        (writer.join().unwrap_or(false), output)
    });
    let Ok(output) = output else {
        return BTreeMap::new();
    };
    if !wrote_all_paths || !output.status.success() {
        return BTreeMap::new();
    }

    let listing = String::from_utf8_lossy(&output.stdout);
    let values: Vec<&str> = listing.split('\0').collect();
    let mut attributes = BTreeMap::new();
    for record in values.as_chunks::<3>().0 {
        attributes
            .entry(record[0].to_string())
            .or_insert_with(LinguistAttributes::default)
            .set(record[1], record[2]);
    }
    attributes
}

/// Whether a directory entry is a directory, answered from the entry's own
/// dirent data - no per-path `stat`, which a walk over thousands of entries
/// pays dearly for. Only a symlink (whose dirent can't tell what it points at)
/// falls back to following the path, preserving follow-the-link semantics.
pub(super) fn is_directory_entry(entry: &std::fs::DirEntry) -> bool {
    match entry.file_type() {
        Ok(file_type) if file_type.is_symlink() => entry.path().is_dir(),
        Ok(file_type) => file_type.is_dir(),
        Err(_) => entry.path().is_dir(),
    }
}

/// Bounded filesystem-walk fallback for a folder that isn't a git repo: sum
/// census bytes under `dir`, skipping the hidden and dependency/build dirs the
/// marker probe skips (untracked build output physically lives here, so the
/// exclusions matter). A Git repository goes through [`git_repository`] instead.
pub(super) fn census_walk(
    directory: &std::path::Path,
    depth: usize,
    files_left: &mut usize,
    profile: &mut SourceProfile,
) {
    if depth > CENSUS_MAXIMUM_DEPTH || *files_left == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
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
        if is_directory_entry(&entry) {
            if !IGNORED_PROBE_DIRECTORIES.contains(&name) {
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

/// Source evidence for `cwd`. The file list is git's tracked set plus the
/// untracked files it does not ignore when `cwd` is a repo - so an agent's new
/// file counts the moment it is written, while ignored build output never sways
/// the mix - and a bounded filesystem walk otherwise. Bounded to
/// [`CENSUS_MAXIMUM_FILES`] either way so a huge tree can't stall a suggestion.
pub(super) fn source_profile(cwd: &std::path::Path) -> SourceProfile {
    let mut profile = SourceProfile::default();
    if let Some(GitRepository {
        root,
        counted_paths: mut paths,
    }) = git_repository(cwd)
    {
        paths.truncate(CENSUS_MAXIMUM_FILES);
        let attributes = git_linguist_attributes(&root, &paths);
        for path in &paths {
            let absolute_path = root.join(path);
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
        let mut files_left = CENSUS_MAXIMUM_FILES;
        census_walk(cwd, 0, &mut files_left, &mut profile);
    }
    profile
}

/// Byte totals retained as a small compatibility wrapper for census-focused
/// tests; runtime suggestion uses [`source_profile`] once for both views.
#[cfg(test)]
pub(super) fn census(cwd: &std::path::Path) -> BTreeMap<ProjectKind, u64> {
    source_profile(cwd).totals
}
