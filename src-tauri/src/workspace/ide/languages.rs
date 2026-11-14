//! Per-project language shares and the cache that keeps their census cheap.

use super::census::source_profile;
use super::project_registry::language_named;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::BTreeMap;

/// A language below this share of a project's source (in per mille) is left
/// off its language bar, so a build script never paints a sliver.
pub(super) const MINIMUM_LANGUAGE_PER_MILLE: u64 = 30;

pub(super) const PER_MILLE: u64 = 1000;

/// One language's share of a project's source, for the switcher's GitHub-style
/// language bar. `perMille` is of the languages shown, so the bar's segments
/// always fill it.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LanguageShare {
    /// The Linguist name - the bar's label and colour key.
    pub(super) language: String,
    /// Its family, for a project row's icon.
    pub(super) kind: String,
    pub(super) per_mille: u64,
}

/// Each project's languages by share of its source, biggest first, for the
/// switcher rows' language bars. Paths with no countable source are omitted.
// `async` + `spawn_blocking`: this is the git-tracked source census, once per
// visible row - real filesystem work, off the UI thread.
#[tauri::command]
pub(crate) async fn ide_project_languages(paths: Vec<String>) -> BTreeMap<String, Vec<LanguageShare>> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .filter_map(|path| {
                let shares = cached_language_shares(&path);
                (!shares.is_empty()).then_some((path, shares))
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// What a project's count was made against. The index moves when the tracked
/// set does (a stage, commit or checkout); the working-tree generation moves
/// when a watch reports an edit or a new file - a change the index never sees.
/// A count is served only while both still match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CensusStamp {
    index_modified: std::time::SystemTime,
    working_tree_generation: u64,
}

pub(super) type CountedShares = (CensusStamp, Vec<LanguageShare>);

pub(super) static LANGUAGE_CACHE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, CountedShares>>,
> = std::sync::LazyLock::new(Default::default);

/// How many working-tree changes a watch has reported per project, keyed like
/// [`LANGUAGE_CACHE`]. In memory only: a restart starts with an empty cache.
pub(super) static WORKING_TREE_GENERATIONS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, u64>>,
> = std::sync::LazyLock::new(Default::default);

/// One spelling per project folder, whichever way a caller wrote it - the
/// switcher's row path and the Change Feed's watch root must land on the same
/// cache entry. Windows paths compare case-insensitively.
pub(crate) fn project_key(project: &std::path::Path) -> String {
    let normalized: std::path::PathBuf = project.components().collect();
    let key = normalized.to_string_lossy().into_owned();
    if cfg!(windows) {
        key.to_lowercase()
    } else {
        key
    }
}

/// Record that a watch saw a file in `project` change, so its next language
/// count is made afresh instead of served from [`LANGUAGE_CACHE`].
pub(crate) fn note_working_tree_change(project: &std::path::Path) {
    if let Ok(mut generations) = WORKING_TREE_GENERATIONS.lock() {
        *generations.entry(project_key(project)).or_insert(0) += 1;
    }
}

pub(super) fn working_tree_generation(key: &str) -> u64 {
    WORKING_TREE_GENERATIONS
        .lock()
        .ok()
        .and_then(|generations| generations.get(key).copied())
        .unwrap_or(0)
}

/// When anything a project tracks last changed - the newest of its own and its
/// submodules' git index files - or `None` outside a git checkout.
pub(super) fn index_modified(project: &std::path::Path) -> Option<std::time::SystemTime> {
    crate::vcs::tracked_set_indexes(project)
        .iter()
        .filter_map(|index| {
            std::fs::metadata(index)
                .and_then(|metadata| metadata.modified())
                .ok()
        })
        .max()
}

/// A project's language shares, counted again only when its git index moved or
/// a watch reported a working-tree change since the last count - so re-checking
/// every row each time the switcher opens costs one `stat` (plus two map
/// lookups) per unchanged project. The stamp is taken before counting, so an
/// edit landing mid-count leaves the entry already stale. A folder outside git
/// is always counted.
pub(super) fn cached_language_shares(path: &str) -> Vec<LanguageShare> {
    let project = std::path::Path::new(path);
    let key = project_key(project);
    let stamp = index_modified(project).map(|index_modified| CensusStamp {
        index_modified,
        working_tree_generation: working_tree_generation(&key),
    });
    let cached = stamp.and_then(|stamp| {
        let cache = LANGUAGE_CACHE.lock().ok()?;
        let (counted_against, shares) = cache.get(&key)?;
        (*counted_against == stamp).then(|| shares.clone())
    });
    if let Some(shares) = cached {
        return shares;
    }

    let shares = language_shares(&source_profile(project).languages);
    if let (Some(stamp), Ok(mut cache)) = (stamp, LANGUAGE_CACHE.lock()) {
        cache.insert(key, (stamp, shares.clone()));
    }
    shares
}

/// Turn per-language source bytes into the bar's shares: languages under
/// [`MINIMUM_LANGUAGE_PER_MILLE`] dropped, the rest re-shared among themselves
/// and ordered biggest first.
pub(super) fn language_shares(totals: &BTreeMap<&'static str, u64>) -> Vec<LanguageShare> {
    let total: u64 = totals.values().sum();
    if total == 0 {
        return Vec::new();
    }

    let mut kept: Vec<(&'static str, u64)> = totals
        .iter()
        .filter(|(_, bytes)| **bytes * PER_MILLE / total >= MINIMUM_LANGUAGE_PER_MILLE)
        .map(|(language, bytes)| (*language, *bytes))
        .collect();
    kept.sort_by_key(|(_, bytes)| Reverse(*bytes));
    let kept_total: u64 = kept.iter().map(|(_, bytes)| bytes).sum();
    kept.into_iter()
        .filter_map(|(name, bytes)| {
            let language = language_named(name)?;
            Some(LanguageShare {
                language: language.name.to_string(),
                kind: language.kind.as_str().to_string(),
                per_mille: bytes * PER_MILLE / kept_total,
            })
        })
        .collect()
}
