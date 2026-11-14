//! Kind commands: the per-kind editor options and the kind of each listed project.

use super::declarations::declarations_in;
use super::declarations::has_extension;
use super::declarations::kinds_from_declarations;
use super::declarations::probe_roots;
use super::declarations::ProjectDeclaration;
use super::editor_registry::EditorCoverage;
use super::editor_registry::ProjectKind;
use super::editor_registry::REGISTRY;
use super::editors::installed_ides;
use super::editors::ADDED_EDITOR_ID_PREFIX;
use super::editors::TYPESCRIPT_ICON_KEY;
use super::project_registry::Marker;
use super::project_registry::KIND_REGISTRY;
use super::suggest::dedup_in_order;
use super::suggest::ranked_editor_ids;
use super::suggest::FALLBACK_KEY;
use serde::Serialize;
use std::collections::BTreeMap;

/// Editor ids to offer per editor-rules row, installed-only. A kind's list is
/// derived from the same capability table and coverage scorer as suggestions,
/// so an unrelated IDE is never offered. Unknown folders get only universal
/// editors and user-added launchers.
// `async` + `spawn_blocking`: rides the same PATH-probing detection as
// [`ide_detect`], so it must stay off the UI thread too.
#[tauri::command]
pub(crate) async fn ide_kind_options() -> BTreeMap<String, Vec<String>> {
    tauri::async_runtime::spawn_blocking(kind_options)
        .await
        .unwrap_or_default()
}

/// The per-kind editor options behind [`ide_kind_options`].
pub(super) fn kind_options() -> BTreeMap<String, Vec<String>> {
    let installed = installed_ides();
    let is_present = |id: &str| installed.iter().any(|editor| editor.id == id);
    let added_ids = installed
        .iter()
        .filter(|editor| editor.id.starts_with(ADDED_EDITOR_ID_PREFIX))
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
        .map(|definition| {
            let source_bytes = BTreeMap::from([(definition.kind, 1)]);
            let options = dedup_in_order(
                ranked_editor_ids(&source_bytes, &[definition.kind])
                    .into_iter()
                    .filter(|id| {
                        REGISTRY
                            .iter()
                            .find(|editor| editor.id == id)
                            .is_some_and(|editor| editor.coverage.supports(definition.kind))
                    })
                    .filter(|id| is_present(id))
                    .chain(added_ids.clone())
                    .collect(),
            );
            (definition.kind.as_str().to_string(), options)
        })
        .chain(std::iter::once((FALLBACK_KEY.to_string(), general)))
        .collect()
}

/// One project kind as the editor-rules UI renders it - its id, display label,
/// and the marker signals shown under the label.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KindInfo {
    pub(super) kind: String,
    pub(super) label: String,
    pub(super) signals: Vec<String>,
}

/// Every recognised project kind, in the [`KIND_REGISTRY`]'s priority/render
/// order. Drives the editor-rules rows in the picker's settings.
#[tauri::command]
pub(crate) fn ide_kinds() -> Vec<KindInfo> {
    KIND_REGISTRY
        .iter()
        .map(|definition| KindInfo {
            kind: definition.kind.as_str().to_string(),
            label: definition.label.to_string(),
            signals: definition.markers.iter().map(Marker::display).collect(),
        })
        .collect()
}

/// Primary detected kind for each of `paths` - the kind most of its project
/// roots declare ([`primary_declared_kind`]) - for the switcher's per-project language
/// logo. Paths with no recognised markers are omitted, so the caller falls back
/// to a folder glyph. Marker probing only (root + one level down), so it stays
/// cheap enough to run for a menuful of recent projects at once.
// `async` + `spawn_blocking`: one child walk per project for a whole menuful of
// recents is real filesystem work - off the UI thread.
#[tauri::command]
pub(crate) async fn ide_project_kinds(paths: Vec<String>) -> BTreeMap<String, String> {
    tauri::async_runtime::spawn_blocking(move || project_kinds(paths))
        .await
        .unwrap_or_default()
}

/// The kind most of a workspace's project roots declare - six C# projects
/// beside two small Rust tools read as .NET, not as whichever kind the registry
/// happens to list first. A platform declared anywhere claims the kinds it owns,
/// so an Android app's Gradle files - which read as Java in every module - count
/// for Android rather than outvoting it. A tie goes to the registry's priority
/// order.
pub(super) fn primary_declared_kind(declarations: &[ProjectDeclaration]) -> Option<ProjectKind> {
    let declared = kinds_from_declarations(declarations);
    let claimed_by = |kind: ProjectKind| {
        declared
            .iter()
            .copied()
            .find(|platform| *platform != kind && platform.owns_source_kind(kind))
            .unwrap_or(kind)
    };
    let declaring_roots = |kind: &ProjectKind| {
        declarations
            .iter()
            .filter(|declaration| {
                declaration
                    .kinds
                    .iter()
                    .any(|declared_kind| claimed_by(*declared_kind) == *kind)
            })
            .count()
    };
    // `max_by_key` keeps the LAST of equal maxima, so walking the registry order
    // backwards leaves a tie with the kind listed first.
    declared
        .iter()
        .copied()
        .filter(|kind| claimed_by(*kind) == *kind)
        .rev()
        .max_by_key(declaring_roots)
}

/// The per-path primary-kind probe behind [`ide_project_kinds`].
pub(super) fn project_kinds(paths: Vec<String>) -> BTreeMap<String, String> {
    paths
        .into_iter()
        .filter_map(|path| {
            // One child walk per project, shared by the marker probe and the
            // TypeScript narrowing - this runs for a whole menuful of recents.
            let roots = probe_roots(std::path::Path::new(&path));
            let kind = primary_declared_kind(&declarations_in(&roots))?;
            // A web project that uses TypeScript shows the TS badge, not JavaScript.
            let icon_key = if kind == ProjectKind::Web && uses_typescript(&roots) {
                TYPESCRIPT_ICON_KEY.to_string()
            } else {
                kind.as_str().to_string()
            };
            Some((path, icon_key))
        })
        .collect()
}

/// Whether a web project uses TypeScript - a `tsconfig.json`, or a `.ts`/`.tsx`
/// source in one of the already-walked probe roots or its `src` dir. Narrows
/// the per-project switcher icon from the JavaScript badge to the TypeScript one.
pub(super) fn uses_typescript(roots: &[std::path::PathBuf]) -> bool {
    roots.iter().any(|root| {
        root.join("tsconfig.json").exists()
            || has_extension(root, "ts")
            || has_extension(root, "tsx")
            || has_extension(&root.join("src"), "ts")
            || has_extension(&root.join("src"), "tsx")
    })
}
