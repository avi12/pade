//! Which editors to suggest for a project: ranking, coverage and the per-project choice.

use super::census::source_profile;
use super::declarations::project_declarations;
use super::declarations::required_project_kinds;
use super::editor_registry::EditorCoverage;
use super::editor_registry::IdeDefinition;
use super::editor_registry::ProjectKind;
use super::editor_registry::REGISTRY;
use super::editors::added_editor_ides;
use super::editors::added_editors;
use super::editors::lookup;
use super::editors::Ide;
use super::editors::ADDED_EDITOR_ID_PREFIX;
use super::kinds::primary_declared_kind;
use crate::util::is_on_path;
use std::cmp::Reverse;
use std::collections::BTreeMap;

/// The map key for the "any other folder" fallback options (not a project kind).
/// Matches the `key` the frontend's fallback editor-select uses.
pub(super) const FALLBACK_KEY: &str = "fallback";

/// Order-preserving dedup: keep each id's first appearance only. No mutable
/// accumulator - an id survives just at the index of its first occurrence.
pub(super) fn dedup_in_order(ids: Vec<String>) -> Vec<String> {
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
pub(super) fn ranked_editor_ids(
    source_bytes: &BTreeMap<ProjectKind, u64>,
    required_kinds: &[ProjectKind],
) -> Vec<String> {
    let mut editors: Vec<&IdeDefinition> = REGISTRY.iter().collect();
    editors.sort_by_cached_key(|editor| editor_rank(editor.coverage, source_bytes, required_kinds));
    editors.iter().map(|editor| editor.id.to_string()).collect()
}

/// One editor's sort key for [`ranked_editor_ids`], best first under ascending
/// order. With no evidence at all a general-purpose editor leads; with no
/// required kinds, source coverage leads and narrower breadth breaks ties; with
/// required kinds, required coverage leads, then breadth, then source coverage.
pub(super) fn editor_rank(
    coverage: EditorCoverage,
    source_bytes: &BTreeMap<ProjectKind, u64>,
    required_kinds: &[ProjectKind],
) -> (Reverse<u64>, usize, Reverse<u64>) {
    let has_evidence = !source_bytes.is_empty() || !required_kinds.is_empty();
    if !has_evidence {
        let is_general = matches!(coverage, EditorCoverage::EveryKind);
        return (Reverse(u64::from(is_general)), 0, Reverse(0));
    }
    let covered_source: u64 = source_bytes
        .iter()
        .filter(|(kind, _)| coverage.supports(**kind))
        .map(|(_, bytes)| *bytes)
        .sum();
    if required_kinds.is_empty() {
        return (Reverse(covered_source), coverage.breadth(), Reverse(0));
    }
    let covered_required = required_kinds
        .iter()
        .filter(|kind| coverage.supports(**kind))
        .count();
    let covered_required = u64::try_from(covered_required).unwrap_or(u64::MAX);
    (
        Reverse(covered_required),
        coverage.breadth(),
        Reverse(covered_source),
    )
}

/// Whether a known editor covers the project's required capabilities. Declared
/// project kinds plus languages in their main source branch distinguish a real
/// hybrid from incidental tracked source such as documentation and support
/// tools. An unmarked folder requires every source kind the census found.
/// User-added editors lack a capability declaration, so their explicit rule is
/// retained rather than guessing what the user's installation supports.
pub(super) fn editor_covers_project(
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
pub(super) fn suggestible_editor_ids(
    source_bytes: &BTreeMap<ProjectKind, u64>,
    required_kinds: &[ProjectKind],
) -> Vec<String> {
    ranked_editor_ids(source_bytes, required_kinds)
        .into_iter()
        .filter(|id| editor_covers_project(id, source_bytes, required_kinds))
        .collect()
}

/// An id is worth suggesting only if it's actually launchable - a registry
/// launcher on PATH, or a user-added editor (its stored path is the launcher,
/// resolved against the caller's already-loaded `added` list).
pub(super) fn is_installed(id: &str, added: &[Ide]) -> bool {
    if id.starts_with(ADDED_EDITOR_ID_PREFIX) {
        return added.iter().any(|editor| editor.id == id);
    }
    lookup(id, added).is_some_and(|editor| is_on_path(&editor.command))
}

/// The user-configured editor ids that lead the suggestion ranking, in winning
/// order. An explicit per-project choice always wins - the user pointed at that
/// editor for this very project, so no coverage rule may displace it. The
/// primary-kind rule and the fallback follow, but only while they cover the
/// project's required kinds.
pub(super) fn preference_chain(
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
/// every required kind derived from project declarations and source ownership -
/// a user rule for the primary declared kind first, then the configured
/// fallback, then evidence-weighted editor coverage. No dominant-language
/// threshold or framework-specific detection is involved.
// `async` + `spawn_blocking`: the source census walks the whole project and the
// installed check probes PATH - far too much filesystem work for the UI thread.
#[tauri::command]
pub async fn ide_suggest(cwd: String) -> Result<Vec<Ide>, String> {
    tauri::async_runtime::spawn_blocking(move || suggest_for(&cwd))
        .await
        .map_err(|error| error.to_string())?
}

/// The ranking pass behind [`ide_suggest`].
pub(super) fn suggest_for(cwd: &str) -> Result<Vec<Ide>, String> {
    let canonical_project = crate::workspace::canonical_path(cwd);
    let cwd = std::path::Path::new(&cwd);
    if !cwd.is_dir() {
        return Err("project directory does not exist".to_string());
    }
    let declarations = project_declarations(cwd);
    let profile = source_profile(cwd);
    let required_kinds = required_project_kinds(&profile, &declarations);
    // One settings read for the whole pass; every per-id resolve below shares it.
    let prefs = crate::workspace::load().prefs;
    let added = added_editor_ides(prefs.added_editors.clone());

    // 1) Explicit per-project pick, 2) compatible primary-kind rule,
    // 3) compatible fallback, 4) coverage ranking.
    let choice = prefs.ide_project_choices.get(&canonical_project).cloned();
    let rule = primary_declared_kind(&declarations)
        .and_then(|kind| prefs.ide_rules.get(kind.as_str()).cloned());
    let configured = preference_chain(choice.clone(), rule, prefs.ide_fallback, |id| {
        editor_covers_project(id, &profile.totals, &required_kinds)
    });
    let auto = suggestible_editor_ids(&profile.totals, &required_kinds);

    let mut ordered: Vec<String> = Vec::new();
    for id in configured.into_iter().chain(auto) {
        let is_new_and_installed = !ordered.contains(&id) && is_installed(&id, &added);
        if is_new_and_installed {
            ordered.push(id);
        }
    }
    let mut ranked: Vec<Ide> = ordered.iter().filter_map(|id| lookup(id, &added)).collect();
    for ide in &mut ranked {
        ide.chosen = choice.as_deref() == Some(ide.id.as_str());
    }
    Ok(ranked)
}

/// Remember an explicit editor pick for the project at `cwd`. The pick becomes
/// the project's editor everywhere at once, because every surface resolves
/// through [`ide_suggest`], which puts it first. Only a known editor id (a
/// registry entry or a user-added editor) is persisted.
#[tauri::command]
pub fn ide_choose_editor(cwd: String, id: String) -> Result<crate::workspace::Settings, String> {
    let is_known_editor = REGISTRY.iter().any(|editor| editor.id == id)
        || added_editors().iter().any(|editor| editor.id == id);
    if !is_known_editor {
        return Err(format!("unknown editor id \u{201c}{id}\u{201d}"));
    }
    crate::workspace::set_project_editor(&cwd, &id)
}
