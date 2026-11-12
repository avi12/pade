//! Feed commands: watch status, diffs, previews and ignore checks.

use super::change::authorize_path;
use super::change::read_authorized_file;
use super::change::read_preview_text;
use super::ignore::is_always_ignored;
use super::registry::owning_project;
use super::registry::window_watches;
use super::registry::ProjectWatch;
use super::registry::WatcherState;
use crate::platform::preview::image_mime_type;
use crate::platform::preview::video_preview;
use crate::platform::preview::FeedImage;
use crate::platform::preview::FeedVideo;
use crate::platform::preview::MAXIMUM_IMAGE_BYTES;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::State;
use tauri::WebviewWindow;

/// What one held project's Change Feed watch is doing right now - the panel states it
/// verbatim when the feed is empty, so "nothing has changed yet" is never
/// mistaken for "nothing is being watched".
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WatchStatus {
    /// The root the watch is armed on; `None` when this window doesn't watch it.
    root: Option<String>,
    /// Epoch ms the watch was armed - the feed holds nothing from before it.
    armed_at: Option<u64>,
    /// Changes surfaced to this window since arming.
    surfaced: u64,
    /// Changes the ignore rules dropped since arming. A climbing count under an
    /// empty feed says the watch is alive and the rules are eating everything.
    ignored: u64,
}

/// The live watch state of `root` in this window (see [`WatchStatus`]); the
/// default, empty status when the window doesn't watch that root. A pure
/// in-memory registry read, so it stays synchronous.
#[tauri::command]
pub fn watch_status(
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    root: String,
) -> WatchStatus {
    let Some(watch) =
        window_watches(&state, window.label()).and_then(|watches| watches.find(&root))
    else {
        return WatchStatus::default();
    };
    WatchStatus {
        root: Some(watch.root.to_string_lossy().into_owned()),
        armed_at: Some(watch.armed_at),
        surfaced: watch.surfaced.load(Ordering::Relaxed),
        ignored: watch.ignored.load(Ordering::Relaxed),
    }
}

/// The window's project watch that owns `path` (see [`WindowWatches::owner_of`]).
pub(super) fn owning_watch(
    state: &WatcherState,
    label: &str,
    path: &Path,
) -> Option<Arc<ProjectWatch>> {
    window_watches(state, label)?.owner_of(path)
}

/// The two texts a Change Feed card diffs: the session's first-touch baseline and
/// the file's current content. The frontend renders the unified diff from these.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedDiff {
    /// First-touch baseline content (empty for a file created this session).
    before: String,
    /// Current on-disk content (empty when the file is now deleted).
    after: String,
}

/// The session-baseline diff inputs for `path`: its first-touch baseline
/// (`before`) and current on-disk content (`after`). `None` when no baseline was
/// captured this session, or when the baseline or the current file is binary /
/// over the size cap - the card then shows "No preview available". Git-independent
/// by construction, so an untracked or ignored file previews like any other, and a
/// file deleted after being touched shows its baseline as a full removal.
#[tauri::command]
pub async fn feed_diff(
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    path: String,
) -> Result<Option<FeedDiff>, String> {
    let file = Path::new(&path);
    let Some(watch) = owning_watch(&state, window.label(), file) else {
        return Ok(None);
    };
    let before = {
        let baselines = watch.baselines.lock().map_err(|error| error.to_string())?;
        let Some(Some(text)) = baselines.get(file) else {
            return Ok(None);
        };
        text.clone()
    };

    let after = match read_preview_text(&watch.canonical_root, file) {
        Ok(Some(text)) => text,
        Ok(None) => String::new(),
        Err(()) => return Ok(None),
    };

    Ok(Some(FeedDiff { before, after }))
}

/// The current text of a watched `path`, for the Change Feed's markdown/HTML
/// Preview toggle: the same UTF-8, size-capped read [`feed_diff`] uses for its
/// `after`, but handed over on its own so the Preview renders the file as it is
/// now (the diff is baseline→current; the preview is just current). `None` when
/// the path was not surfaced by this window's watch this session (so the command
/// can't be turned into an arbitrary file reader), is gone, or is binary / over
/// [`MAXIMUM_PREVIEW_BYTES`] - the card then keeps its diff and offers no preview.
#[tauri::command]
pub async fn feed_text(
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    path: String,
) -> Result<Option<String>, String> {
    let file = Path::new(&path);
    let Some(watch) = owning_watch(&state, window.label(), file) else {
        return Ok(None);
    };
    if !was_surfaced(&watch, file)? {
        return Ok(None);
    }
    Ok(read_preview_text(&watch.canonical_root, file)
        .ok()
        .flatten())
}

/// Whether this window's watch surfaced `path` this session (it holds a
/// baseline entry for it). The gate every preview command passes first, so none
/// of them can be turned into a reader of an arbitrary file off disk.
pub(super) fn was_surfaced(watch: &ProjectWatch, path: &Path) -> Result<bool, String> {
    let baselines = watch.baselines.lock().map_err(|error| error.to_string())?;
    Ok(baselines.contains_key(path))
}

/// The inline image preview for `path`: its bytes as a base64 `data:` URL tagged
/// with the extension's MIME type. `None` when `path` is not a previewable image,
/// was not surfaced by this window's watch this session, is missing / not a
/// regular file, or is larger than [`MAXIMUM_IMAGE_BYTES`] - the card then falls back
/// to its text summary. Reads the CURRENT bytes (what the image is now); a
/// before/after for a modified image is a deliberate non-goal here. Gated on a
/// captured baseline like `feed_diff`, so the command can't base64 an arbitrary
/// file off disk.
#[tauri::command]
pub async fn feed_image(
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    path: String,
) -> Result<Option<FeedImage>, String> {
    let file = Path::new(&path);
    let Some(watch) = owning_watch(&state, window.label(), file) else {
        return Ok(None);
    };
    if !was_surfaced(&watch, file)? {
        return Ok(None);
    }

    if image_mime_type(file).is_none() {
        return Ok(None);
    }
    let Ok(Some(bytes)) = read_authorized_file(&watch.canonical_root, file, MAXIMUM_IMAGE_BYTES)
    else {
        return Ok(None);
    };

    Ok(FeedImage::of(file, &bytes))
}

/// The video preview for `path`: its opening seconds transcoded by ffmpeg into a
/// small MP4 (see [`crate::platform::media`]). `None` when `path` is not a video, was not
/// surfaced by this window's watch this session, is gone, or ffmpeg couldn't
/// read it. Gated and root-confined exactly like [`feed_image`].
#[tauri::command]
pub async fn feed_video(
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    path: String,
) -> Result<Option<FeedVideo>, String> {
    let file = Path::new(&path);
    let Some(watch) = owning_watch(&state, window.label(), file) else {
        return Ok(None);
    };
    if !crate::platform::media::is_video(file) || !was_surfaced(&watch, file)? {
        return Ok(None);
    }
    let Ok(Some(source)) = authorize_path(&watch.canonical_root, file) else {
        return Ok(None);
    };

    video_preview(source).await
}

/// The subset of `paths` the current ignore policy excludes - how the frontend
/// re-filters the Change Feed it already rendered after `feed://ignore-changed`
/// (events for now-ignored paths are dropped; never-recorded ones can't be
/// resurrected, they simply surface again on their next real change). Each
/// path is judged by the rules of the held project that owns it.
#[tauri::command]
pub async fn feed_ignored(
    window: WebviewWindow,
    state: State<'_, WatcherState>,
    paths: Vec<String>,
) -> Result<Vec<String>, String> {
    let projects = window_watches(&state, window.label())
        .map(|watches| watches.snapshot())
        .unwrap_or_default();
    Ok(paths
        .into_iter()
        .filter(|candidate| {
            let path = Path::new(candidate);
            is_always_ignored(path)
                || owning_project(&projects, path)
                    .is_some_and(|project| project.ignore_rules.excludes(path))
        })
        .collect())
}
