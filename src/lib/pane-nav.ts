// Pure, DOM-free navigation across the ordered split panes. The panes render
// left→right in `paneIds`; these resolve which pane a keyboard step or a
// by-position pick lands on, so the shortcut registrar stays free of index
// math and every hop is unit-tested. All three navigators resolve through the
// one bounds-checked `paneIdAt` lookup (DRY): prev/next compute a wrapped index
// and hand it to `paneIdAt`, exactly as the by-position pick does.

const STEP_PREVIOUS = -1;
const STEP_NEXT = 1;

/** The panes shown in a split, left→right, plus which one is active. A plain
 *  object keeps navigation testable without any DOM or Svelte state. */
interface PaneSelection {
  paneIds: readonly string[];
  activeId: string;
}

/** The id of the pane at a 0-based position, or null when the position falls
 *  outside the split - the shared, bounds-checked lookup that the by-position
 *  pick and both wrapping steps resolve through. */
export function paneIdAt({ paneIds, iPane }: {
  paneIds: readonly string[];
  iPane: number;
}) {
  return paneIds[iPane] ?? null;
}

/** The id of the pane `step` positions from the active one, wrapping around the
 *  ends. Null when the split is empty or the active id isn't among the shown
 *  panes. One wrapped-index calculation serves both directions. */
function steppedPaneId({ paneIds, activeId, step }: PaneSelection & { step: number }) {
  const count = paneIds.length;
  const iActive = paneIds.indexOf(activeId);
  if (count === 0 || iActive === -1) {
    return null;
  }

  return paneIdAt({
    paneIds,
    iPane: (iActive + step + count) % count
  });
}

/** The pane before the active one, wrapping from the first pane to the last. */
export function previousPaneId(selection: PaneSelection) {
  return steppedPaneId({
    ...selection,
    step: STEP_PREVIOUS
  });
}

/** The pane after the active one, wrapping from the last pane to the first. */
export function nextPaneId(selection: PaneSelection) {
  return steppedPaneId({
    ...selection,
    step: STEP_NEXT
  });
}
