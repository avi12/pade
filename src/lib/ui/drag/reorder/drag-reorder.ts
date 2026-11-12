// The one pointer-drag engine (Tesler's Law: all the drag complexity lives here),
// shared by the session-tab strip, the terminal pane row, and the pinned-project
// list (DRY). One press-and-drag gesture: lift the pressed item, track the
// pointer, slide the siblings aside to open the slot it will land in, and - on a
// normal drop - spring the lifted item from where it was released into that slot
// before committing the new order. Pointer events only (no HTML5 drag API), so it
// works the same for mouse, pen, and touch; the call sites give their handles
// `touch-action: none` so a touch-drag never scrolls instead.
//
// The FEEL is the design mockup's engine, reproduced faithfully:
//   • Spring-settle from the release point - the lifted item animates
//     (`translate .3s cubic-bezier(.34,1.35,.5,1)` with the shadow fading out) from
//     wherever it was let go into its target slot, and the reordered array is only
//     committed ~300ms later, once it has settled. The engine owns the whole drop
//     animation, so the tab/pane lists must NOT also run `animate:flip` (double
//     animation) - the parent removes it.
//   • A transparent drag shield (`data-drag-shield`) under the lifted item carries
//     the grabbing cursor over every surface (xterm canvases included) and freezes
//     stray interaction; it is dropped from hit-testing around each `elementFromPoint`
//     probe so the real drop zone underneath stays reachable.
//   • Unclip / reclip - a tab dragged OUT of the horizontally-scrolling strip would
//     be cut off, so on lift every clipping ancestor's overflow is forced `visible`
//     and restored on drop.
//   • Sticky insertion - the landing slot excludes the dragged item's *current*
//     gap, so a swapped item holds until the pointer crosses the neighbour's new
//     centre (no flicker at the boundary). Reduced motion skips the transitions.
//
// The DOM-free order/geometry math (`insertionIndex`, `reorderedIds`, `paneDropSide`)
// lives in `@/lib/session/panes/reorder` so it's unit-tested and shared - the which-half decision
// has one home there. This module measures geometry, drives the DOM imperatively,
// and reports every state change through `onHint` so a Svelte caller can mirror it
// into `$state` and paint its own affordances. The deferred commit awaits Svelte's
// `tick` so the reordered re-render and the inline-style teardown coincide in one
// frame (no flash).

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
import { Axis } from "@/lib/ui/drag/reorder/reorder-types";
import type { BeginReorderOptions, DropTarget } from "@/lib/ui/drag/reorder/reorder-types";
import { Key } from "@/lib/ui/keyboard/keys";
import { prefersReducedMotion } from "@/lib/ui/motion";
import { tick } from "svelte";

const DEFAULT_THRESHOLD = 5;
const PRIMARY_BUTTON = 0;
const NOT_FOUND = -1;

// Only one drag may run at a time - a second press while one is live (including
// the ~300ms settle after release) is ignored so gestures never overlap.
let isDragInProgress = false;

export function beginReorder(options: BeginReorderOptions) {
  const {
    e,
    itemSelector,
    idAttribute,
    axis,
    threshold = DEFAULT_THRESHOLD,
    ignoreSelector,
    onCommit,
    onHint,
    outsideSelector,
    onDropOutside
  } = options;
  // Primary button only - middle-click (close a tab) and right-click stay theirs.
  if (isDragInProgress || e.button !== PRIMARY_BUTTON || !(e.target instanceof Element)) {
    return;
  }

  // A press that lands on an ignored control is that control's, not a handle.
  if (ignoreSelector && e.target.closest(ignoreSelector)) {
    return;
  }

  const elPressed = e.target.closest<HTMLElement>(itemSelector);
  const elPressedParent = elPressed?.parentElement ?? null;
  const pressedId = elPressed?.getAttribute(idAttribute) ?? null;
  if (!elPressed || !elPressedParent || pressedId === null) {
    return;
  }

  // Closure-safe non-null bindings: TS drops the guard's narrowing inside the
  // nested handlers below, so re-bind through explicitly typed consts.
  const elParent: HTMLElement = elPressedParent;
  const draggedId: string = pressedId;
  const isHorizontal = axis === Axis.Horizontal;
  const isReducedMotion = prefersReducedMotion();

  const pointerStartX = e.clientX;
  const pointerStartY = e.clientY;
  const bodyStyle = document.body.style;
  const bodyUserSelect = bodyStyle.userSelect;
  const parentPointerEvents = elParent.style.pointerEvents;

  // Mutable gesture state - all snapshotted lazily once the drag actually begins.
  let isDragging = false;
  let siblings: Sibling[] = [];
  let centers: number[] = [];
  let originalIds: string[] = [];
  let iOriginalById = new Map<string, number>();
  let iFrom = NOT_FOUND;
  let iCurrent = NOT_FOUND;
  let iLastBefore = NOT_FOUND;
  let grabOffsetX = 0;
  let grabOffsetY = 0;
  let lastPointerX = pointerStartX;
  let lastPointerY = pointerStartY;
  let isOutside = false;
  let dropTarget: DropTarget | null = null;
  // A transparent full-window sheet under the lifted item: it shows the grabbing
  // cursor over every surface and swallows stray hover/click. Disabled around each
  // hit-test so `elementFromPoint` reaches the drop zone below it.
  let elShield: HTMLElement | null = null;
  const overflowSaves: OverflowSave[] = [];

  isDragInProgress = true;
  addEventListener("pointermove", onPointerMove);
  addEventListener("pointerup", onPointerUp);
  addEventListener("pointercancel", onPointerCancel);
  addEventListener("keydown", onKeyDown, true);

  // ── geometry ───────────────────────────────────────────────────────────────

  function snapshot() {
    siblings = measureSiblings({
      elParent,
      itemSelector,
      idAttribute
    });

    originalIds = siblings.map(sibling => sibling.id);
    iOriginalById = new Map(siblings.map((sibling, i) => [sibling.id, i]));
    centers = siblings.map(slot => axisCenter({
      slot,
      isHorizontal
    }));
    iFrom = iOriginalById.get(draggedId) ?? NOT_FOUND;
    iCurrent = iFrom;
  }

  function updateOutsideHints(point: Point) {
    if (!outsideSelector) {
      return;
    }

    const elHit = hitElementAt({
      point,
      elShield
    });
    isOutside = isOverOutsideZone({
      elHit,
      outsideSelector,
      itemSelector
    });
    dropTarget = resolveDropTarget({
      elHit,
      x: point.x,
      itemSelector
    });
  }

  // ── lift / restore ───────────────────────────────────────────────────────────

  function startDrag() {
    isDragging = true;
    snapshot();

    if (iFrom === NOT_FOUND) {
      return;
    }

    const draggedSlot = siblings[iFrom];
    grabOffsetX = pointerStartX - draggedSlot.left;
    grabOffsetY = pointerStartY - draggedSlot.top;

    bodyStyle.userSelect = "none";
    elParent.style.pointerEvents = "none";
    elShield = raiseShield();

    liftSiblings({
      siblings,
      iFrom,
      isReducedMotion
    });
    overflowSaves.push(
      ...unclipAncestors({
        elParent,
        isEscapeDrag: Boolean(outsideSelector)
      })
    );
  }

  function removeShield() {
    elShield?.remove();
    elShield = null;
  }

  // ── events ─────────────────────────────────────────────────────────────────

  function onPointerMove(e: PointerEvent) {
    lastPointerX = e.clientX;
    lastPointerY = e.clientY;

    if (!isDragging) {
      const travelled = Math.hypot(e.clientX - pointerStartX, e.clientY - pointerStartY);
      if (travelled < threshold) {
        return;
      }

      startDrag();

      if (iFrom === NOT_FOUND) {
        finish(true);
        return;
      }
    }

    const draggedSlot = siblings[iFrom];
    const projectedLeft = e.clientX - grabOffsetX;
    const projectedTop = e.clientY - grabOffsetY;
    draggedSlot.elItem.style.translate = `${projectedLeft - draggedSlot.left}px ${projectedTop - draggedSlot.top}px`;

    // Exclude the dragged item's *current* gap (not its origin) from the count, so
    // a swapped item stays put until the pointer crosses the neighbour's new centre.
    const iBefore = insertionIndex({
      centers,
      fromIndex: iCurrent,
      draggedCenter: isHorizontal
        ? projectedLeft + draggedSlot.width / 2
        : projectedTop + draggedSlot.height / 2
    });
    if (iBefore !== iLastBefore) {
      iLastBefore = iBefore;
      iCurrent = iBefore;
      reflowSiblings({
        siblings,
        originalIds,
        iOriginalById,
        draggedId,
        iBefore
      });
    }

    updateOutsideHints({
      x: e.clientX,
      y: e.clientY
    });
    onHint?.({
      id: draggedId,
      isOutside,
      dropTarget,
      pointerX: e.clientX,
      pointerY: e.clientY
    });
  }

  function onPointerUp(e: PointerEvent) {
    lastPointerX = e.clientX;
    lastPointerY = e.clientY;

    if (isDragging) {
      // A real drag must not also fire the click the browser synthesizes under the
      // pointer (which would select/rename the item we just dropped).
      suppressNextClick();
    }

    finish(false);
  }

  function onPointerCancel() {
    finish(true);
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key !== Key.Escape) {
      return;
    }

    e.preventDefault();

    if (isDragging) {
      suppressNextClick();
    }

    finish(true);
  }

  /** Pop the item out onto the outside zone when it was released over it; false
   *  when the release point isn't a drop-out. Re-hit-tests at the release point
   *  (the lifted item is still pointer-transparent, so the zone beneath is
   *  reachable). */
  function dropOutside() {
    if (!onDropOutside) {
      return false;
    }

    const elHit = hitElementAt({
      point: {
        x: lastPointerX,
        y: lastPointerY
      },
      elShield
    });
    const isReleasedOutside = isOverOutsideZone({
      elHit,
      outsideSelector,
      itemSelector
    });
    if (!isReleasedOutside) {
      return false;
    }

    const releaseTarget = resolveDropTarget({
      elHit,
      x: lastPointerX,
      itemSelector
    });
    clearItemStyles(siblings);
    reclipAncestors(overflowSaves);
    onHint?.(null);
    onDropOutside({
      id: draggedId,
      dropTarget: releaseTarget,
      pointerX: lastPointerX,
      pointerY: lastPointerY
    });
    isDragInProgress = false;
    return true;
  }

  // Tear down input, drop the shield, then either pop out (outside), spring home
  // (cancel), or spring into the new slot and commit (normal drop).
  function finish(isCancelled: boolean) {
    removeEventListener("pointermove", onPointerMove);
    removeEventListener("pointerup", onPointerUp);
    removeEventListener("pointercancel", onPointerCancel);
    removeEventListener("keydown", onKeyDown, true);
    bodyStyle.userSelect = bodyUserSelect;
    removeShield();
    elParent.style.pointerEvents = parentPointerEvents;

    if (!isDragging) {
      isDragInProgress = false;
      return;
    }

    isDragging = false;

    // The pressed item vanished before it could lift - nothing to settle or commit.
    if (iFrom === NOT_FOUND) {
      onHint?.(null);
      isDragInProgress = false;
      return;
    }

    if (!isCancelled && dropOutside()) {
      return;
    }

    settleIntoSlot(isCancelled);
  }

  // Spring the lifted item from its release point to the target slot (the open gap,
  // or home on cancel), fading the shadow out; commit the reorder only once it has
  // settled, so nothing jumps.
  function settleIntoSlot(isCancelled: boolean) {
    const elDragged = siblings[iFrom].elItem;
    const isReturningHome = isCancelled || iLastBefore < 0;
    const target = slotTranslate({
      from: siblings[iFrom],
      to: siblings[isReturningHome ? iFrom : iCurrent]
    });
    if (isReducedMotion) {
      elDragged.style.translate = target;
      queueMicrotask(async () => {
        await finalizeCommit(isCancelled);
      });
      return;
    }

    springToTarget({
      elDragged,
      target
    });
    setTimeout(async () => {
      await finalizeCommit(isCancelled);
    }, SPRING_MILLISECONDS);
  }

  // A normal drop always commits the current order - even a same-slot no-op, where
  // `committedOrderOnDrop` returns the full list unchanged. Skipping the commit on a
  // no-op left the caller's list out of sync with the DOM the engine had moved, and
  // a later selection reconciled against that stale order and dropped the tab; the
  // design engine commits `lastOrder ?? ids.slice()` for exactly this reason.
  async function finalizeCommit(isCancelled: boolean) {
    const order = committedOrderOnDrop({
      ids: originalIds,
      fromIndex: iFrom,
      toIndex: iCurrent,
      cancelled: isCancelled
    });
    if (order) {
      onCommit(order);
      // Let Svelte apply the committed DOM before stripping the inline translates,
      // so the two coincide in one frame and the drop never flashes.
      await tick();
    }

    // Strip the inline translates BEFORE restoring the ancestors' overflow: with
    // clipping back to `auto` first, the still-translated item overflows its
    // scroll container for the awaited frame and flashes a scrollbar.
    clearItemStyles(siblings);
    reclipAncestors(overflowSaves);
    onHint?.(null);
    isDragInProgress = false;
  }
}

// Swallow the very next click anywhere (capture phase, one shot) so a drop can't
// double as a click. A macrotask retires the guard if the browser fires no click
// at all (it usually doesn't after a long drag), so it never eats a later click.
function suppressNextClick() {
  function swallow(e: Event) {
    e.stopPropagation();

    if (e.cancelable) {
      e.preventDefault();
    }

    removeEventListener("click", swallow, true);
  }

  addEventListener("click", swallow, true);
  setTimeout(() => removeEventListener("click", swallow, true), 0);
}
