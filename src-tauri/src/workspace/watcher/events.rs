//! Turns one raw notify event into a Change Feed entry.

use super::change::line_count;
use super::change::line_delta;
use super::change::read_preview_text;
use super::change::reclassify;
use super::change::summarize;
use super::change::surfaces;
use super::change::ChangeEvent;
use super::change::ChangeKind;
use super::ignore::is_always_ignored;
use super::ignore::is_gitignore_file;
use super::ignore::refresh_ignore_policy;
use super::languages::report_language_news;
use super::languages::LanguageNews;
use super::mcp::detect_mcp_change;
use super::registry::ProjectWatch;
use super::CHANGE_EVENT_SEQUENCE;
use super::SAVE_BURST_DEBOUNCE;
use crate::platform::preview::MAXIMUM_PREVIEW_BYTES;
use crate::util::now_milliseconds;
use notify::Event;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Instant;
use tauri::AppHandle;
use tauri::Emitter;

/// Announces a single Change Feed entry (a file created/modified/deleted in a
/// watched project). This name is the frontend contract (`src/lib/bridge.ts`).
pub(super) const FEED_CHANGE_EVENT: &str = "feed://change";

/// One project watch's notify callback. Every surfaced change is routed back to
/// the owning window alone (`emit_to`), never broadcast to sibling windows, and
/// tagged with the project root as the frontend spelled it.
pub(super) fn handle_event(app: &AppHandle, watch: &ProjectWatch, event: Event) {
    let Some(kind) = ChangeKind::from_event(event.kind) else {
        return;
    };
    let label = watch.label.as_str();

    for path in event.paths {
        if is_always_ignored(&path) {
            watch.ignored.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        // Both the directory test and the atomic-write check need metadata.
        // Resolve it once per path rather than issuing two filesystem queries for
        // every event in an editor's save burst.
        let metadata = std::fs::metadata(&path).ok();
        if metadata.as_ref().is_some_and(std::fs::Metadata::is_dir) {
            continue;
        }
        // A created/modified file that has already vanished was an atomic-write
        // scratch file - skip it rather than surface a preview-less phantom card.
        if !surfaces(kind, metadata.is_some()) {
            continue;
        }

        // The event kind describes the past; the disk describes now. An editor's
        // atomic save deletes the real path and renames a scratch file over it,
        // so a Remove event routinely loses the race with the recreation -
        // trusting it would card a live file as "Deleted" while its fresh
        // content produces a positive line delta. A file that exists is a
        // modification, whatever the event said.
        let kind = reclassify(kind, metadata.is_some());

        // A changed MCP config (`.mcp.json`) may have gained or lost a server;
        // if so, the affected agent's sessions restart to pick it up. Checked
        // before the ignore filter, since `.mcp.json` is often git-ignored.
        detect_mcp_change(app, watch, &path);

        // A changed .gitignore - edited, appearing for the first time, or
        // deleted - changes what counts as ignored, so rebuild the whole policy
        // (fresh rules in static mode, dropped memoized decisions in git mode)
        // and tell the feed to re-filter what it already shows. The .gitignore
        // itself is not ignored, so it still surfaces as its own change.
        if is_gitignore_file(&path) {
            refresh_ignore_policy(app, watch);
        }

        // Skip whatever the project itself would ignore: git's own ignore rules in
        // a repo, or the tech-inferred ignore directories when there is no git. The
        // baseline `ignored` pre-filter above already dropped the giant dirs, so a
        // repo never shells git for node_modules/target and each surviving path is
        // git-checked at most once (memoized).
        if watch.ignore_rules.excludes(&path) {
            watch.ignored.fetch_add(1, Ordering::Relaxed);
            continue;
        }

        // The language watch leaves this project's working tree to the feed
        // (see `planned_language_routes`), so a change the feed keeps is news
        // for its language bar too.
        report_language_news(
            app,
            label,
            &watch.root.to_string_lossy(),
            LanguageNews::WorkingTree,
        );

        // Debounce: editors emit bursts per save.
        {
            let Ok(mut seen) = watch.last_seen.lock() else {
                continue;
            };
            let now = Instant::now();
            let within_debounce = seen
                .get(&path)
                .is_some_and(|previous| now.duration_since(*previous) < SAVE_BURST_DEBOUNCE);
            if within_debounce {
                continue;
            }
            seen.insert(path.clone(), now);
        }

        // First-touch baseline for the git-free preview: the content this path
        // held the first time it changed this session. A creation baselines as
        // empty (its whole content is new this session); any other first sighting
        // snapshots the current on-disk text - the accepted slightly-late baseline
        // (the edit that fired this very event is already on disk). Later edits,
        // and a later deletion, diff against this. `or_insert_with` runs only on
        // the first sighting, so a large file is read at most once.
        record_first_touch_baseline(&watch.baselines, &path, || match kind {
            ChangeKind::Created => Some(String::new()),
            ChangeKind::Modified => read_preview_text(&watch.canonical_root, &path)
                .ok()
                .flatten(),
            ChangeKind::Deleted => read_preview_text(&watch.canonical_root, &path)
                .ok()
                .flatten()
                .or_else(|| crate::vcs::head_file_text(&watch.root, &path, MAXIMUM_PREVIEW_BYTES)),
        });

        let new_count = line_count(&path);
        let (added, removed) = {
            let Ok(mut counts) = watch.line_counts.lock() else {
                continue;
            };
            let old = counts.get(&path).copied();
            let delta = line_delta(old, new_count);
            if let Some(count) = new_count {
                counts.insert(path.clone(), count);
            } else {
                counts.remove(&path);
            }
            delta
        };

        let path_string = path.to_string_lossy().into_owned();
        let id = format!(
            "{}-{}",
            now_milliseconds(),
            CHANGE_EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let event = ChangeEvent {
            id,
            root: watch.requested_root.clone(),
            summary: summarize(kind, added, removed),
            path: path_string,
            kind,
            added,
            removed,
            timestamp: now_milliseconds(),
        };
        watch.surfaced.fetch_add(1, Ordering::Relaxed);
        let _ = app.emit_to(label, FEED_CHANGE_EVENT, event);
    }
}

/// Record `path`'s first-touch baseline unless it already has one. The
/// snapshot (a file read, or a `git show` for a deletion) runs outside the
/// lock, so a preview command reading the baselines never waits on that I/O;
/// the insert keeps any baseline recorded meanwhile, so the first one wins.
pub(super) fn record_first_touch_baseline(
    baselines: &Mutex<HashMap<PathBuf, Option<String>>>,
    path: &Path,
    snapshot: impl FnOnce() -> Option<String>,
) {
    let is_recorded = baselines
        .lock()
        .map_or(true, |recorded| recorded.contains_key(path));
    if is_recorded {
        return;
    }
    let baseline = snapshot();
    if let Ok(mut recorded) = baselines.lock() {
        recorded.entry(path.to_path_buf()).or_insert(baseline);
    }
}
