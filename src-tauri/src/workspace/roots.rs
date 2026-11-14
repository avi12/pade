//! Project roots: adding and removing them, scanning for projects, path probing and creating a project.

use super::recents::workspace_open;
use super::store::is_project;
use super::store::update_settings;
use super::store::ProjectEntry;
use super::store::Settings;
use super::store::GIT_DIRECTORY;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

/// The result of trying to add a root folder. A directory that already exists (or
/// one just created on request) is added and carries the refreshed `Settings`; the
/// two "didn't add" outcomes tell the picker to prompt or show an error instead of
/// persisting a broken root.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub(crate) enum AddRootOutcome {
    /// The path is (now) a directory and was added. Boxed so this data-carrying
    /// variant doesn't bloat the empty ones (serializes identically to `Settings`).
    Added { settings: Box<Settings> },
    /// The path doesn't exist and creation wasn't requested.
    Missing,
    /// The path exists but names a file, not a directory.
    NotADirectory,
}

/// Rebuild a path from its components - collapses doubled or trailing separators
/// and forward slashes so one folder is spelled exactly one way and dedups (e.g.
/// `C:\\a\\b`, `C:/a/b` and `C:\a\b\` all fold to `C:\a\b`), while keeping a bare
/// drive root like `C:\` intact.
pub(crate) fn canonical_path(path: &str) -> String {
    Path::new(path)
        .components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned()
}

/// Join a user-provided folder name beneath `root` without allowing the name to
/// become a path, drive, device, alternate data stream, or parent traversal.
pub(crate) fn validated_child_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    let has_separator = name.contains(['/', '\\']);
    let has_control = name.chars().any(char::is_control);
    if name.is_empty() || name == "." || name == ".." || has_separator || has_control {
        return Err("folder name must be one plain path segment".into());
    }

    #[cfg(windows)]
    {
        /// Device names Windows reserves whatever the extension.
        const RESERVED_DEVICE_NAMES: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
        /// Port names Windows reserves when followed by one digit from 1 to 9.
        const RESERVED_PORT_PREFIXES: [&str; 2] = ["COM", "LPT"];

        let stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let is_reserved_port = RESERVED_PORT_PREFIXES.iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(
                |number| matches!(number.as_bytes(), [digit] if (b'1'..=b'9').contains(digit)),
            )
        });
        let is_reserved = RESERVED_DEVICE_NAMES.contains(&stem.as_str()) || is_reserved_port;
        if name.contains(':') || name.ends_with(['.', ' ']) || is_reserved {
            return Err("folder name is reserved by Windows".into());
        }
    }

    Ok(root.join(name))
}

/// Canonicalize a path list and drop duplicates, keeping first-seen order - so a
/// folder recorded twice under different spellings collapses to one entry.
pub(super) fn canonical_dedup(paths: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    paths
        .iter()
        .map(|path| canonical_path(path))
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

/// Persist `path` as a root (deduped) and hand back the refreshed settings. The
/// path is canonicalized first (see [`canonical_path`]) so a root is stored one
/// way and dedups.
pub(super) fn push_root(path: String) -> Result<Settings, String> {
    let path = canonical_path(&path);
    update_settings(|settings| {
        if !settings.roots.contains(&path) {
            settings.roots.push(path);
        }
        Ok(())
    })
}

/// Add a root folder. An existing directory is added as-is; a missing path is only
/// created (and then added) when `create` is set, otherwise it reports `Missing`;
/// a path that exists but is a file reports `NotADirectory`.
#[tauri::command]
pub(crate) async fn workspace_add_root(path: String, create: bool) -> Result<AddRootOutcome, String> {
    let target = Path::new(&path);
    if target.is_dir() {
        return Ok(AddRootOutcome::Added {
            settings: Box::new(push_root(path)?),
        });
    }
    if target.exists() {
        return Ok(AddRootOutcome::NotADirectory);
    }
    if !create {
        return Ok(AddRootOutcome::Missing);
    }
    std::fs::create_dir_all(target).map_err(|error| error.to_string())?;
    Ok(AddRootOutcome::Added {
        settings: Box::new(push_root(path)?),
    })
}

#[tauri::command]
pub(crate) fn workspace_remove_root(path: String) -> Result<Settings, String> {
    update_settings(|settings| {
        settings.roots.retain(|root| root != &path);
        Ok(())
    })
}

/// Shown for a project folder whose name is not valid UTF-8.
pub(super) const UNREADABLE_NAME: &str = "?";

/// Immediate sub-directories of `root` that look like projects.
#[tauri::command]
pub(crate) async fn workspace_scan(root: String) -> Result<Vec<ProjectEntry>, String> {
    let directory = std::fs::read_dir(&root).map_err(|error| error.to_string())?;
    let mut entries: Vec<ProjectEntry> = directory
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && is_project(path))
        .map(|path| ProjectEntry {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(UNREADABLE_NAME)
                .to_string(),
            is_git: path.join(GIT_DIRECTORY).exists(),
            path: path.to_string_lossy().into_owned(),
        })
        .collect();
    entries.sort_by_cached_key(|entry| entry.name.to_lowercase());
    Ok(entries)
}

/// A live look at a path the user is typing into the add-root field: what the
/// path itself is on disk (so the field can say "exists" vs "will be created", or
/// reject a file), plus the child directories that would complete it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PathProbe {
    /// The typed path is an existing directory.
    is_dir: bool,
    /// The typed path exists but names a file, not a directory.
    is_file: bool,
    /// The typed path's parent is an existing directory - so even though the path
    /// itself doesn't exist yet, it names a real place PADE can create it in. This
    /// is what tells a not-yet-created folder ("C:\repositories\new-app") apart
    /// from a stray, un-locatable string - an existence check in place of a regex.
    parent_exists: bool,
    /// Absolute paths of child directories that complete the text, name-sorted
    /// and capped. Empty when nothing matches or the parent can't be read.
    suggestions: Vec<String>,
    /// The typed path completed as far as every matching folder agrees (Ctrl+Space
    /// in the path field): the one match plus a separator to drill into it, or
    /// the leaf prefix all matches share. Read off every match, not the capped
    /// `suggestions`, so a long listing never completes past where it forks.
    /// Empty when nothing matches.
    completion: String,
}

/// How many directory completions to offer at once - enough to be useful, few
/// enough to stay a glance rather than a scroll.
pub(super) const SUGGESTION_LIMIT: usize = 8;

/// Split a partially-typed path into the directory to list and the (possibly
/// empty) leaf typed so far. A trailing separator means "list everything inside
/// this directory"; otherwise the last segment is the prefix to match. The
/// separator is kept on a bare drive/root head so `C:\` stays absolute. `None`
/// when there's no separator yet (e.g. a lone `C:` - nothing to complete).
pub(super) fn split_for_completion(input: &str) -> Option<(PathBuf, String)> {
    let cut = input.rfind(['\\', '/'])?;
    let (head, tail) = input.split_at(cut);
    let prefix = tail[1..].to_string();
    let parent = if head.is_empty() || head.ends_with(':') {
        format!("{head}{}", std::path::MAIN_SEPARATOR)
    } else {
        head.to_string()
    };
    Some((PathBuf::from(parent), prefix))
}

/// Child directories of `parent` whose name starts with `prefix` (case-insensitive),
/// keyed by the lowercased leaf. Collecting into a `BTreeMap` yields the name sort
/// for free - no mutable scratch vector to sort in place.
pub(super) fn matching_directories(parent: &Path, prefix: &str) -> BTreeMap<String, PathBuf> {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return BTreeMap::new();
    };
    let needle = prefix.to_lowercase();
    // The name test runs first so only matching entries pay for the stat.
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let lowercase_leaf = entry
                .file_name()
                .to_str()
                .unwrap_or_default()
                .to_lowercase();
            lowercase_leaf
                .starts_with(&needle)
                .then(|| (lowercase_leaf, entry.path()))
        })
        .filter(|(_, path)| path.is_dir())
        .collect()
}

/// The matching directories as absolute paths, capped at [`SUGGESTION_LIMIT`].
pub(super) fn directory_completions(matches: &BTreeMap<String, PathBuf>) -> Vec<String> {
    matches
        .values()
        .take(SUGGESTION_LIMIT)
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

/// The longest leading run two leaf names share, compared case-insensitively
/// (Windows names are) and spelled as `first` spells it.
pub(super) fn shared_leaf_prefix(first: &str, last: &str) -> String {
    first
        .chars()
        .zip(last.chars())
        .take_while(|(first_char, last_char)| {
            first_char.to_lowercase().eq(last_char.to_lowercase())
        })
        .map(|(first_char, _)| first_char)
        .collect()
}

/// How far every match agrees, as a path under `parent`: the one match plus a
/// separator, so completion carries on into it; or the prefix all matches
/// share. The map is sorted by lowercased leaf, so what its first and last
/// entries share, every entry between them shares too.
pub(super) fn next_segment_completion(
    parent: &Path,
    matches: &BTreeMap<String, PathBuf>,
) -> String {
    let (Some(first), Some(last)) = (matches.values().next(), matches.values().next_back()) else {
        return String::new();
    };
    if matches.len() == 1 {
        return format!("{}{}", first.to_string_lossy(), std::path::MAIN_SEPARATOR);
    }

    let leaf = |path: &PathBuf| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    parent
        .join(shared_leaf_prefix(&leaf(first), &leaf(last)))
        .to_string_lossy()
        .into_owned()
}

/// Probe the path being typed into a folder field: what it is on disk, plus the
/// child-directory completions that drive the field's autocomplete and its
/// Ctrl+Space completion. Pure query (no persistence); a bad or unreadable path
/// just yields no completions.
#[tauri::command]
pub(crate) async fn workspace_probe_path(path: String) -> PathProbe {
    let trimmed = path.trim();
    let target = Path::new(trimmed);
    let (suggestions, completion) = split_for_completion(trimmed)
        .map(|(parent, prefix)| {
            let matches = matching_directories(&parent, &prefix);
            (
                directory_completions(&matches),
                next_segment_completion(&parent, &matches),
            )
        })
        .unwrap_or_default();
    PathProbe {
        is_dir: target.is_dir(),
        is_file: target.is_file(),
        parent_exists: target.parent().is_some_and(Path::is_dir),
        suggestions,
        completion,
    }
}

/// Create a new project directory under `root` and open it.
#[tauri::command]
pub(crate) async fn workspace_create(root: String, name: String) -> Result<String, String> {
    let path = validated_child_path(Path::new(&root), &name)?;
    std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    let path_string = path.to_string_lossy().into_owned();
    workspace_open(path_string.clone())?;
    Ok(path_string)
}
