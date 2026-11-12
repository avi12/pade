// The state of one live reorder drag and the steps that act on it: measure, lift,
// track the pointer, then settle (spring home or into the open slot), pop out onto
// the outside zone, or commit. `beginReorder` owns the listeners and the one-drag-
// at-a-time latch; everything that reads or writes the gesture lives here, on the
// one `ReorderGesture` record.

import { committedOrderOnDrop, insertionIndex } from "@/lib/session/panes/reorder";
import { axisCenter, measureSiblings, reflowSiblings, slotTranslate } from "@/lib/ui/drag/reorder/reorder-geometry";
import type { Point, Sibling } from "@/lib/ui/drag/reorder/reorder-geometry";
import { hitElementAt, isOverOutsideZone, resolveDropTarget } from "@/lib/ui/drag/reorder/reorder-hit-test";
import {
  clearItemStyles,
  liftSiblings,
  raiseShield,
  reclipAncestors,
  SPRING_MILLISECONDS,
  springToTarget,
  unclipAncestors
} from "@/lib/ui/drag/reorder/reorder-lift";
import type { OverflowSave } from "@/lib/ui/drag/reorder/reorder-lift";
import type { BeginReorderOptions, DropTarget } from "@/lib/ui/drag/reorder/reorder-types";
import { tick } from "svelte";

const NOT_FOUND = -1;

export interface ReorderGesture {
  readonly options: BeginReorderOptions;
  readonly elParent: HTMLElement;
  readonly draggedId: string;
  readonly isHorizontal: boolean;
  readonly isReducedMotion: boolean;
  readonly threshold: number;
  readonly pointerStartX: number;
  readonly pointerStartY: number;
  readonly bodyUserSelect: string;
  readonly parentPointerEvents: string;
  /** Frees the one-drag-at-a-time latch. */
  readonly release: () => void;
  readonly overflowSaves: OverflowSave[];
  isDragging: boolean;
  siblings: Sibling[];
  centers: number[];
  originalIds: string[];
  iOriginalById: Map<string, number>;
  iFrom: number;
  iCurrent: number;
  iLastBefore: number;
  grabOffsetX: number;
  grabOffsetY: number;
  lastPointerX: number;
  lastPointerY: number;
  isOutside: boolean;
  dropTarget: DropTarget | null;
  /** A transparent full-window sheet under the lifted item: it shows the grabbing
   *  cursor over every surface and swallows stray hover/click. Disabled around each
   *  hit-test so `elementFromPoint` reaches the drop zone below it. */
  elShield: HTMLElement | null;
}

export function createGesture({ options, elParent, draggedId, isHorizontal, isReducedMotion, threshold, release }: {
  options: BeginReorderOptions;
  elParent: HTMLElement;
  draggedId: string;
  isHorizontal: boolean;
  isReducedMotion: boolean;
  threshold: number;
  release: () => void;
}): ReorderGesture {
  return {
    options,
    elParent,
    draggedId,
    isHorizontal,
    isReducedMotion,
    threshold,
    pointerStartX: options.e.clientX,
    pointerStartY: options.e.clientY,
    bodyUserSelect: document.body.style.userSelect,
    parentPointerEvents: elParent.style.pointerEvents,
    release,
    overflowSaves: [],
    isDragging: false,
    siblings: [],
    centers: [],
    originalIds: [],
    iOriginalById: new Map(),
    iFrom: NOT_FOUND,
    iCurrent: NOT_FOUND,
    iLastBefore: NOT_FOUND,
    grabOffsetX: 0,
    grabOffsetY: 0,
    lastPointerX: options.e.clientX,
    lastPointerY: options.e.clientY,
    isOutside: false,
    dropTarget: null,
    elShield: null
  };
}

function snapshot(gesture: ReorderGesture) {
  const { options, elParent, draggedId, isHorizontal } = gesture;
  gesture.siblings = measureSiblings({
    elParent,
    itemSelector: options.itemSelector,
    idAttribute: options.idAttribute
  });

  gesture.originalIds = gesture.siblings.map(sibling => sibling.id);
  gesture.iOriginalById = new Map(gesture.siblings.map((sibling, i) => [sibling.id, i]));
  gesture.centers = gesture.siblings.map(slot => axisCenter({
    slot,
    isHorizontal
  }));
  gesture.iFrom = gesture.iOriginalById.get(draggedId) ?? NOT_FOUND;
  gesture.iCurrent = gesture.iFrom;
}

function updateOutsideHints({ gesture, point }: {
  gesture: ReorderGesture;
  point: Point;
}) {
  const { outsideSelector, itemSelector } = gesture.options;
  if (!outsideSelector) {
    return;
  }

  const elHit = hitElementAt({
    point,
    elShield: gesture.elShield
  });
  gesture.isOutside = isOverOutsideZone({
    elHit,
    outsideSelector,
    itemSelector
  });
  gesture.dropTarget = resolveDropTarget({
    elHit,
    x: point.x,
    itemSelector
  });
}

function startDrag(gesture: ReorderGesture) {
  gesture.isDragging = true;
  snapshot(gesture);

  if (gesture.iFrom === NOT_FOUND) {
    return;
  }

  const draggedSlot = gesture.siblings[gesture.iFrom];
  gesture.grabOffsetX = gesture.pointerStartX - draggedSlot.left;
  gesture.grabOffsetY = gesture.pointerStartY - draggedSlot.top;

  document.body.style.userSelect = "none";
  gesture.elParent.style.pointerEvents = "none";
  gesture.elShield = raiseShield();

  liftSiblings({
    siblings: gesture.siblings,
    iFrom: gesture.iFrom,
    isReducedMotion: gesture.isReducedMotion
  });
  gesture.overflowSaves.push(
    ...unclipAncestors({
      elParent: gesture.elParent,
      isEscapeDrag: Boolean(gesture.options.outsideSelector)
    })
  );
}

function followPointer({ gesture, e }: {
  gesture: ReorderGesture;
  e: PointerEvent;
}) {
  const { draggedId, siblings, iFrom, isHorizontal } = gesture;
  const draggedSlot = siblings[iFrom];
  const projectedLeft = e.clientX - gesture.grabOffsetX;
  const projectedTop = e.clientY - gesture.grabOffsetY;
  draggedSlot.elItem.style.translate = `${projectedLeft - draggedSlot.left}px ${projectedTop - draggedSlot.top}px`;

  // Exclude the dragged item's *current* gap (not its origin) from the count, so
  // a swapped item stays put until the pointer crosses the neighbour's new centre.
  const iBefore = insertionIndex({
    centers: gesture.centers,
    fromIndex: gesture.iCurrent,
    draggedCenter: isHorizontal
      ? projectedLeft + draggedSlot.width / 2
      : projectedTop + draggedSlot.height / 2
  });
  if (iBefore !== gesture.iLastBefore) {
    gesture.iLastBefore = iBefore;
    gesture.iCurrent = iBefore;
    reflowSiblings({
      siblings,
      originalIds: gesture.originalIds,
      iOriginalById: gesture.iOriginalById,
      draggedId,
      iBefore
    });
  }

  updateOutsideHints({
    gesture,
    point: {
      x: e.clientX,
      y: e.clientY
    }
  });
  gesture.options.onHint?.({
    id: draggedId,
    isOutside: gesture.isOutside,
    dropTarget: gesture.dropTarget,
    pointerX: e.clientX,
    pointerY: e.clientY
  });
}

/** Advance the gesture to a pointer position; false when the pressed item
 *  vanished before it could lift (the caller then cancels). */
export function movePointer({ gesture, e }: {
  gesture: ReorderGesture;
  e: PointerEvent;
}) {
  gesture.lastPointerX = e.clientX;
  gesture.lastPointerY = e.clientY;

  if (!gesture.isDragging) {
    const travelled = Math.hypot(e.clientX - gesture.pointerStartX, e.clientY - gesture.pointerStartY);
    if (travelled < gesture.threshold) {
      return true;
    }

    startDrag(gesture);

    if (gesture.iFrom === NOT_FOUND) {
      return false;
    }
  }

  followPointer({
    gesture,
    e
  });
  return true;
}

/** Pop the item out onto the outside zone when it was released over it; false
 *  when the release point isn't a drop-out. Re-hit-tests at the release point
 *  (the lifted item is still pointer-transparent, so the zone beneath is
 *  reachable). */
function dropOutside(gesture: ReorderGesture) {
  const { options, lastPointerX, lastPointerY } = gesture;
  if (!options.onDropOutside) {
    return false;
  }

  const elHit = hitElementAt({
    point: {
      x: lastPointerX,
      y: lastPointerY
    },
    elShield: gesture.elShield
  });
  const isReleasedOutside = isOverOutsideZone({
    elHit,
    outsideSelector: options.outsideSelector,
    itemSelector: options.itemSelector
  });
  if (!isReleasedOutside) {
    return false;
  }

  const releaseTarget = resolveDropTarget({
    elHit,
    x: lastPointerX,
    itemSelector: options.itemSelector
  });
  clearItemStyles(gesture.siblings);
  reclipAncestors(gesture.overflowSaves);
  options.onHint?.(null);
  options.onDropOutside({
    id: gesture.draggedId,
    dropTarget: releaseTarget,
    pointerX: lastPointerX,
    pointerY: lastPointerY
  });
  gesture.release();
  return true;
}

// A normal drop always commits the current order - even a same-slot no-op, where
// `committedOrderOnDrop` returns the full list unchanged. Skipping the commit on a
// no-op left the caller's list out of sync with the DOM the engine had moved, and
// a later selection reconciled against that stale order and dropped the tab; the
// design engine commits `lastOrder ?? ids.slice()` for exactly this reason.
async function finalizeCommit({ gesture, isCancelled }: {
  gesture: ReorderGesture;
  isCancelled: boolean;
}) {
  const order = committedOrderOnDrop({
    ids: gesture.originalIds,
    fromIndex: gesture.iFrom,
    toIndex: gesture.iCurrent,
    cancelled: isCancelled
  });
  if (order) {
    gesture.options.onCommit(order);
    // Let Svelte apply the committed DOM before stripping the inline translates,
    // so the two coincide in one frame and the drop never flashes.
    await tick();
  }

  // Strip the inline translates BEFORE restoring the ancestors' overflow: with
  // clipping back to `auto` first, the still-translated item overflows its
  // scroll container for the awaited frame and flashes a scrollbar.
  clearItemStyles(gesture.siblings);
  reclipAncestors(gesture.overflowSaves);
  gesture.options.onHint?.(null);
  gesture.release();
}

// Spring the lifted item from its release point to the target slot (the open gap,
// or home on cancel), fading the shadow out; commit the reorder only once it has
// settled, so nothing jumps.
function settleIntoSlot({ gesture, isCancelled }: {
  gesture: ReorderGesture;
  isCancelled: boolean;
}) {
  const { siblings, iFrom } = gesture;
  const elDragged = siblings[iFrom].elItem;
  const isReturningHome = isCancelled || gesture.iLastBefore < 0;
  const target = slotTranslate({
    from: siblings[iFrom],
    to: siblings[isReturningHome ? iFrom : gesture.iCurrent]
  });
  if (gesture.isReducedMotion) {
    elDragged.style.translate = target;
    queueMicrotask(async () => {
      await finalizeCommit({
        gesture,
        isCancelled
      });
    });
    return;
  }

  springToTarget({
    elDragged,
    target
  });
  setTimeout(async () => {
    await finalizeCommit({
      gesture,
      isCancelled
    });
  }, SPRING_MILLISECONDS);
}

/** Drop the shield and restore the page, then either pop out (outside), spring home
 *  (cancel), or spring into the new slot and commit (normal drop). */
export function endGesture({ gesture, isCancelled }: {
  gesture: ReorderGesture;
  isCancelled: boolean;
}) {
  document.body.style.userSelect = gesture.bodyUserSelect;
  gesture.elShield?.remove();
  gesture.elShield = null;
  gesture.elParent.style.pointerEvents = gesture.parentPointerEvents;

  if (!gesture.isDragging) {
    gesture.release();
    return;
  }

  gesture.isDragging = false;

  // The pressed item vanished before it could lift - nothing to settle or commit.
  if (gesture.iFrom === NOT_FOUND) {
    gesture.options.onHint?.(null);
    gesture.release();
    return;
  }

  if (!isCancelled && dropOutside(gesture)) {
    return;
  }

  settleIntoSlot({
    gesture,
    isCancelled
  });
}
