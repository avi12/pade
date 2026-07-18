// Pure, DOM-free navigation across the ordered split panes. The panes render
// left→right in `paneIds`; these resolve which pane a keyboard step or a
// by-position pick lands on, so the shortcut registrar stays free of index
// math and every hop is unit-tested. All three navigators resolve through the
// one bounds-checked `paneIdAt` lookup (DRY): prev/next compute a wrapped index
// and hand it to `paneIdAt`, exactly as the by-position pick does.

/** The panes shown in a split, left→right, plus which one is active. A plain
 *  object keeps navigation testable without any DOM or Svelte state. */
export interface PaneSelection {
  paneIds: readonly string[];
  activeId: string;
}

/** The id of the pane at a 0-based position, or null when the position falls
 *  outside the split — the shared, bounds-checked lookup that the by-position
 *  pick and both wrapping steps resolve through. */
export function paneIdAt({ paneIds, index }: {
  paneIds: readonly string[];
  index: number;
}): string | null {
  return paneIds[index] ?? null;
}

/** The id of the pane `step` positions from the active one, wrapping around the
 *  ends (step -1 = previous, +1 = next). Null when the split is empty or the
 *  active id isn't among the shown panes. One wrapped-index calculation serves
 *  both directions. */
function steppedPaneId({ paneIds, activeId, step }: PaneSelection & { step: number }): string | null {
  const count = paneIds.length;
  if (count === 0) {
    return null;
  }

  const current = paneIds.indexOf(activeId);
  if (current === -1) {
    return null;
  }

  const wrapped = (current + step + count) % count;
  return paneIdAt({
    paneIds,
    index: wrapped
  });
}

/** The pane before the active one, wrapping from the first pane to the last. */
export function previousPaneId(selection: PaneSelection): string | null {
  return steppedPaneId({
    ...selection,
    step: -1
  });
}

/** The pane after the active one, wrapping from the last pane to the first. */
export function nextPaneId(selection: PaneSelection): string | null {
  return steppedPaneId({
    ...selection,
    step: 1
  });
}
