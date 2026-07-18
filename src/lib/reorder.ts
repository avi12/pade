// Pure order/index arithmetic behind the drag-to-reorder + drop-to-split feature.
// DOM-free, so it runs under vitest's node env and is the one authoritative home
// (DRY) for the math shared by the pointer-drag engine (`drag-reorder.ts`) and the
// session-tab → split drop (`App.svelte`). The engine measures geometry and drives
// the DOM imperatively; this module owns only the array/index math on drop.

/** Which half of a target pane a dragged tab lands on. Its own closed set — the
 *  panel-side `Side` in `App.svelte` names a different concern, so no literal is
 *  shared. Lives here (not `types.ts`, which is IPC payloads) because it's a
 *  parameter of `paneInsertIndex` and a UI-only concern. */
export const DropSide = {
  left: "left",
  right: "right"
} as const;
export type DropSide = (typeof DropSide)[keyof typeof DropSide];

/** The id array after moving the item at `fromIndex` to `toIndex`, where `toIndex`
 *  is counted among the *remaining* items (the dragged item is lifted out first).
 *  Returns the order unchanged when `fromIndex === toIndex`. */
export function reorderedIds({ ids, fromIndex, toIndex }: {
  ids: string[];
  fromIndex: number;
  toIndex: number;
}): string[] {
  const draggedId = ids[fromIndex];
  const rest = ids.filter((_, index) => index !== fromIndex);
  return [...rest.slice(0, toIndex), draggedId, ...rest.slice(toIndex)];
}

/** Where the dragged item would land: the count of *other* items whose center
 *  sits before the dragged item's projected `draggedCenter` along the axis.
 *  `centers` holds the siblings' centers in visible order; the slot at `fromIndex`
 *  — the dragged item's slot — is excluded from the count. Feeding it the dragged
 *  item's *current* gap (not its origin) each move gives the drag its stickiness:
 *  a swapped item holds until the pointer crosses the neighbour's new center. */
export function insertionIndex({ centers, fromIndex, draggedCenter }: {
  centers: number[];
  fromIndex: number;
  draggedCenter: number;
}): number {
  let index = 0;
  centers.forEach((center, position) => {
    if (position !== fromIndex && center < draggedCenter) {
      index += 1;
    }
  });
  return index;
}

/** Which half of a target pane a pointer sits over: the left half lands the drop
 *  before the pane, the right half after it. The single geometry decision behind
 *  both the live drop-half highlight and the actual split insertion (DRY), kept
 *  pure so it's unit-tested without a DOM — the caller measures the pane's rect. */
export function paneDropSide({ pointerX, left, width }: {
  pointerX: number;
  left: number;
  width: number;
}): DropSide {
  return pointerX < left + width / 2 ? DropSide.left : DropSide.right;
}

/** Index within the base list (`paneIds` with `draggedId` removed) at which to
 *  insert a tab dropped onto `targetId`'s `side`: the right half lands after the
 *  target, the left half before it. If the target isn't in the base list the tab
 *  appends. `draggedId` is filtered out first, so re-dropping an already-shown
 *  pane repositions it rather than duplicating. */
export function paneInsertIndex({ paneIds, draggedId, targetId, side }: {
  paneIds: string[];
  draggedId: string;
  targetId: string;
  side: DropSide;
}): number {
  const base = paneIds.filter(id => id !== draggedId);
  const targetIndex = base.indexOf(targetId);
  if (targetIndex === -1) {
    return base.length;
  }

  return side === DropSide.right ? targetIndex + 1 : targetIndex;
}
