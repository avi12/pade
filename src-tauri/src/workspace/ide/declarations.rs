//! Project declarations: which kinds a project proves it is, from its manifests and where its source lives.

use super::census::is_directory_entry;
use super::census::SourceFileEvidence;
use super::census::SourceProfile;
use super::editor_registry::ProjectKind;
use super::project_registry::KIND_REGISTRY;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// Whether any direct child of `dir` has the given extension (case-insensitive).
pub(super) fn has_extension(directory: &std::path::Path, extension: &str) -> bool {
    std::fs::read_dir(directory).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        })
    })
}

/// Directories that never signal a project kind of their own: dependency and
/// build output, plus anything hidden. Skipped when probing markers a level
/// down, so `node_modules` or `vendor` can't smuggle in a false kind.
pub(super) const IGNORED_PROBE_DIRECTORIES: &[&str] =
    &["node_modules", "target", "vendor", "dist", "build", "out"];

/// The project root plus its direct child directories - everywhere a kind
/// marker may live. A hybrid app or a small monorepo keeps each side's manifest
/// in its own folder (`src-tauri/Cargo.toml`, `backend/go.mod`), and a root-only
/// probe would miss that whole side of the project.
pub(super) fn probe_roots(cwd: &std::path::Path) -> Vec<std::path::PathBuf> {
    let children = std::fs::read_dir(cwd)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(is_directory_entry)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    !name.starts_with('.') && !IGNORED_PROBE_DIRECTORIES.contains(&name)
                })
        });
    std::iter::once(cwd.to_path_buf()).chain(children).collect()
}

/// One independently declared project unit within the active workspace. Keeping
/// its root lets source files belong to the nearest unit instead of one nested
/// package contaminating its parent profile.
#[derive(Clone, Debug)]
pub(super) struct ProjectDeclaration {
    pub(super) root: std::path::PathBuf,
    pub(super) kinds: Vec<ProjectKind>,
}

/// Every marker-bearing project root among already-walked probe roots - the
/// seam for callers that reuse one [`probe_roots`] listing across several
/// probes instead of re-walking the workspace's children per question.
pub(super) fn declarations_in(roots: &[std::path::PathBuf]) -> Vec<ProjectDeclaration> {
    roots
        .iter()
        .filter_map(|root| {
            let kinds: Vec<ProjectKind> = KIND_REGISTRY
                .iter()
                .filter(|definition| {
                    definition
                        .markers
                        .iter()
                        .any(|marker| marker.is_present(root))
                })
                .map(|definition| definition.kind)
                .collect();
            (!kinds.is_empty()).then_some(ProjectDeclaration {
                root: root.clone(),
                kinds,
            })
        })
        .collect()
}

/// Every marker-bearing project root in the workspace root or a direct child.
/// This depth captures small monorepos and hybrid applications while leaving a
/// nested support utility owned by its enclosing project unless explicitly
/// opened as the active workspace.
pub(super) fn project_declarations(cwd: &std::path::Path) -> Vec<ProjectDeclaration> {
    declarations_in(&probe_roots(cwd))
}

/// Sniff the project kinds present in the current directory from the
/// [`KIND_REGISTRY`] marker files, in the registry's priority order. Markers
/// are probed in the root and one level down (see [`probe_roots`]); they provide
/// a second, source-free signal when a new project has no countable files yet.
pub(super) fn kinds_from_declarations(declarations: &[ProjectDeclaration]) -> Vec<ProjectKind> {
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
pub(super) struct BranchEvidence {
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

pub(super) fn source_branch(
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

pub(super) fn declaration_owns_file(
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
pub(super) fn required_project_kinds(
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
