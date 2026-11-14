//! The order windows are listed, reordered and cycled through, and focusing one by label or relative position.

use super::{
    bring_to_front, broadcast_windows_changed, WindowProjects, MAIN_WINDOW_LABEL,
    SPAWNED_LABEL_PREFIX,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewWindow};

/// Which neighbouring window to cycle to. Mirrors the frontend
/// `focusRelative("previous" | "next")` - one authoritative home for the two
/// names. `pub` because it appears in a `#[tauri::command]` signature.
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CycleDirection {
    Previous,
    Next,
}

/// Stable creation-order sort key: the startup main window first (0), then each
/// spawned `w-{n}` by its sequence number. An unrecognised label sorts last so a
/// stray window never breaks the cycle. Lexicographic sorting would misplace
/// `w-10` before `w-2`; the numeric parse keeps them in creation order.
pub(super) fn order_key(label: &str) -> u32 {
    if label == MAIN_WINDOW_LABEL {
        return 0;
    }
    label
        .strip_prefix(SPAWNED_LABEL_PREFIX)
        .and_then(|sequence| sequence.parse::<u32>().ok())
        .unwrap_or(u32::MAX)
}

/// The one sort key both the switcher list and the `Ctrl+Alt+[`/`]` cycle read, so
/// they never diverge. Windows the user has explicitly ordered (drag-reorder) come
/// first, in that order; any live window not yet in the explicit order follows, in
/// stable creation order. The label breaks remaining ties deterministically. A
/// label recorded in the explicit order but no longer live simply never gets a key
/// (only live windows are enumerated), so it drops out.
pub(super) fn window_sort_key(label: &str, order: &[String]) -> (usize, u32, String) {
    let explicit = order
        .iter()
        .position(|ordered| ordered == label)
        .unwrap_or(usize::MAX);
    (explicit, order_key(label), label.to_string())
}

/// Every live window's label, in switcher order.
fn sorted_live_labels(app: &AppHandle, state: &WindowProjects) -> Vec<String> {
    let order = state.order_snapshot();
    let mut labels: Vec<String> = app.webview_windows().into_keys().collect();
    labels.sort_by_cached_key(|label| window_sort_key(label, &order));
    labels
}

/// Record the user's explicit "Open windows" order (the drag-reordered labels) so
/// both `window_list` and `window_focus_relative` sort by it - one source of truth
/// for the switcher's order and the `Ctrl+Alt+[`/`]` cycle. Session-scoped: labels
/// are ephemeral per app run, so there's nothing meaningful to persist to disk.
#[tauri::command]
pub(crate) fn window_reorder(app: AppHandle, state: tauri::State<WindowProjects>, labels: Vec<String>) {
    if let Ok(mut order) = state.order.lock() {
        *order = labels;
    }
    broadcast_windows_changed(&app);
}

/// The index `direction` lands on from `current` among `count` windows, wrapping
/// around at both ends.
pub(super) fn cycled_index(current: usize, count: usize, direction: CycleDirection) -> usize {
    match direction {
        CycleDirection::Next => (current + 1) % count,
        CycleDirection::Previous => (current + count - 1) % count,
    }
}

/// Focus the previous/next open PADE window, wrapping around at the ends, in the
/// user's explicit switcher order (drag-reordered via `window_reorder`) and
/// creation order for any window not yet reordered - the same order the switcher's
/// "Open windows" list shows. Returns true when another window was focused (false
/// when this is the only window). The calling window is injected as `window`, so
/// the frontend passes only a direction; live windows are re-enumerated each press,
/// so a closed window simply drops out of the cycle.
#[tauri::command]
pub(crate) fn window_focus_relative(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<WindowProjects>,
    direction: CycleDirection,
) -> bool {
    let labels = sorted_live_labels(&app, &state);
    let Some(current) = labels.iter().position(|label| label == window.label()) else {
        return false;
    };
    let is_only_window = labels.len() < 2;
    if is_only_window {
        return false;
    }

    let target_label = &labels[cycled_index(current, labels.len(), direction)];
    app.get_webview_window(target_label)
        .is_some_and(|target| bring_to_front(&target))
}

/// One open PADE window and the project it has: its unique `label` (the id the
/// frontend focuses by), the project path, and whether it is the calling window.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WindowInfo {
    label: String,
    path: String,
    is_current: bool,
}

/// Every open PADE window that has a project, in the user's explicit switcher order
/// (drag-reordered via `window_reorder`, falling back to creation order for any not
/// yet reordered), flagging the caller as the current window. Windows with no
/// registered project (an empty or picker window) are omitted. Drives the
/// switcher's "Open windows" section, so its order matches the `Ctrl+Alt+[`/`]`
/// cycle order - both read the one explicit order.
#[tauri::command]
pub(crate) fn window_list(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<WindowProjects>,
) -> Vec<WindowInfo> {
    let projects = state
        .projects
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    sorted_live_labels(&app, &state)
        .into_iter()
        .filter_map(|label| {
            let path = projects.get(&label)?.clone();
            let is_current = label == window.label();
            Some(WindowInfo {
                label,
                path,
                is_current,
            })
        })
        .collect()
}

/// Focus a specific open PADE window by label - the switcher's "Open windows"
/// rows. Returns true when that window existed and took focus.
#[tauri::command]
pub(crate) fn window_focus_label(app: AppHandle, label: String) -> bool {
    app.get_webview_window(&label)
        .is_some_and(|target| bring_to_front(&target))
}
