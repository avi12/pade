// Pure row bookkeeping for the project rows (the left rail) and the open-window
// rows (the top bar's window menu): which projects a filter keeps, the ids and
// view-transition names each row carries, and the window rows re-ordered the
// way a drag left them.

import type { WindowInfo } from "@/lib/types";
import { displayName, normalizePath } from "@/lib/util/paths";

// Anything a popover id or view-transition-name can't carry.
const NON_IDENTIFIER_CHARACTERS = /[^a-zA-Z0-9]/g;

function identifierFrom(text: string) {
  return text.replaceAll(NON_IDENTIFIER_CHARACTERS, "-");
}

/** Stable, valid popover id/anchor per row kebab (sanitised from the path). */
export function rowMenuId(project: string) {
  return `switcher-${identifierFrom(project)}`;
}

/** Stable, unique view-transition-name per open-window row, keyed by the
 *  window's stable session label, so the scoped view transition glides a
 *  reordered row to its new slot instead of cross-fading the whole list. */
export function windowRowTransitionName(windowLabel: string) {
  return `window-row-${identifierFrom(windowLabel)}`;
}

/** Whether a listed project is this window's own, whatever the path's spelling. */
export function isCurrentProject({ project, current }: {
  project: string;
  current: string;
}) {
  return normalizePath(project) === normalizePath(current);
}

/** Whether a project's shown name or its path contains the (trimmed) filter
 *  text, case-insensitively. An empty filter keeps every project. */
export function matchesProjectFilter({ project, query, labels }: {
  project: string;
  query: string;
  labels: Record<string, string>;
}) {
  const needle = query.toLowerCase();
  if (!needle) {
    return true;
  }

  return (
    displayName({
      path: project,
      labels
    }).toLowerCase().includes(needle) ||
    project.toLowerCase().includes(needle)
  );
}

/** The window rows in the label order a drag committed; a label with no row
 *  (its window closed mid-drag) is dropped. */
export function windowRowsInOrder({ windowRows, order }: {
  windowRows: readonly WindowInfo[];
  order: readonly string[];
}) {
  const byLabel = new Map(windowRows.map(row => [row.label, row]));
  return order.flatMap(windowLabel => {
    const row = byLabel.get(windowLabel);
    return row ? [row] : [];
  });
}
