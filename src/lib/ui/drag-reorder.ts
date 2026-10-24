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

import { colorAlpha, OPAQUE_ALPHA } from "@/lib/code/colors";
import { committedOrderOnDrop, DropSide, insertionIndex, paneDropSide } from "@/lib/session/panes/reorder";
import { Key } from "@/lib/ui/keys";
import { prefersReducedMotion } from "@/lib/ui/motion";
import { tick } from "svelte";

/** The direction items are laid out in, and the one the reorder tracks. */
export const Axis = {
  Horizontal: "horizontal",
  Vertical: "vertical"
} as const;
export type Axis = (typeof Axis)[keyof typeof Axis];

/** A droppable-with-halves target inside the outside zone - a shown terminal pane.
 *  Which half the pointer is over decides the split side. */
export interface DropTarget {
  /** The target pane's id (its `data-pane-id`). */
  id: string;
  /** Which half of the pane the pointer sits over (reused by the split insert). */
  side: DropSide;
}

/** Live state of an in-progress drag, mirrored into the UI so it can paint the
 *  "drop → split" overlay, the target pane's drop half, and the tab-strip pop cue.
 *  Published `null` through `onHint` when the gesture ends. */
export interface DragHint {
  /** The id of the item being dragged. */
  id: string;
  /** True while the pointer is over the `outsideSelector` zone and not over a
   *  reorderable item - a drop-out (split / pop-to-tab), not an in-strip sort. */
  isOutside: boolean;
  /** The pane under the pointer and the half it's over, or `null` when the pointer
   *  isn't over a pane. The engine owns this decision (via `reorder.paneDropSide`),
   *  so a caller can paint the drop half straight from it without re-hit-testing. */
  dropTarget: DropTarget | null;
  /** Raw pointer position - retained so a caller may still hit-test its own zones,
   *  but `dropTarget` above is the resolved answer and should be preferred. */
  pointerX: number;
  pointerY: number;
}

/** Payload for a drop that lands on the outside zone (a tab onto the panes → split,
 *  a pane header onto the tab strip → pop to tab). */
export interface DropOutside {
  /** The dragged item's id. */
  id: string;
  /** The pane + half released over, or `null` when the release point has no pane
   *  (e.g. a pane popped onto the tab strip, which has no halves). */
  dropTarget: DropTarget | null;
  pointerX: number;
  pointerY: number;
}

export interface BeginReorderOptions {
  /** The `pointerdown` that opens the gesture (from the drag handle). */
  e: PointerEvent;
  /** CSS selector for a reorderable item - matched with `closest` from the press
   *  target, and used to enumerate the siblings within the item's parent. */
  itemSelector: string;
  /** Attribute on each item element holding its stable id (e.g. `data-pane-id`). */
  idAttribute: string;
  /** The axis the items flow along; the insertion index is computed along it. */
  axis: Axis;
  /** Pixels the pointer must travel before the press becomes a drag (a plain
   *  click stays a click). Defaults to 5. */
  threshold?: number;
  /** A press starting on an element matching this selector is left alone (the
   *  close / AI buttons on a pill are controls, not drag handles). */
  ignoreSelector?: string;
  /** Called with the visible siblings' ids in their new order, ~300ms after a
   *  normal drop settles (never for a drop-out or a cancel). */
  onCommit: (orderedIds: string[]) => void;
  /** Called on every state change (start, index change, enter/leave outside, drop
   *  target change, end) so the UI can render live indicators. `null` marks the
   *  drag over. */
  onHint?: (hint: DragHint | null) => void;
  /** A drop zone outside the item's own container (the panes for a tab, the tab
   *  strip for a pane). While the pointer is over it the drag reads as a drop-out. */
  outsideSelector?: string;
  /** Called instead of `onCommit` when the drop lands on the outside zone. */
  onDropOutside?: (drop: DropOutside) => void;
}

const DEFAULT_THRESHOLD = 5;

/** The panes are the one droppable-with-halves target: a tab dragged over the
 *  panes resolves which pane + half it lands on. Its one authoritative home. */
const PANE_DROP_TARGET = {
  selector: "[data-pane-id]",
  idAttribute: "data-pane-id"
} as const;

/** How the lifted item springs into its slot (or home) on release: a gentle
 *  overshoot - the design's drop settle, distinct from the emphasized `--ease` the
 *  siblings slide with. No token exists for this one-off curve, so it lives here. */
const SPRING_EASE = "cubic-bezier(0.34, 1.35, 0.5, 1)";
const SPRING_SECONDS = 0.3;
const SPRING_MILLISECONDS = 300;
const SIBLING_SECONDS = 0.24;
/** The lifted item's elevation: a big soft shadow that fades to nothing as it
 *  settles. The colour comes from the theme token so it reads in light and dark
 *  (the mockup, dark-only, hard-codes `hsl(214 40% 4% / .55)`, which the dark
 *  `--shadow-color` matches). */
const LIFT_SHADOW = "0 18px 44px var(--shadow-color)";
const LIFT_SHADOW_FADED = "0 18px 44px transparent";
/** A lifted item floats over its siblings, so it has to be opaque. Rows that
 *  paint no surface of their own (the app menu's window and pinned-project rows
 *  are transparent until hovered) would otherwise let the sibling sliding
 *  underneath read straight through them - two rows' text superimposed. One tone
 *  above the `--surface-2` menu they sit in; a tab pill or runner card that
 *  already paints its own surface keeps it. */
const LIFT_BACKGROUND = "var(--surface-3)";
const LIFTED_Z_INDEX = "130";
const SHIELD_Z_INDEX = "120";
const LIFTED_RADIUS = "0.75rem";

const GRABBING_CURSOR = "grabbing";
const PRIMARY_BUTTON = 0;
const NOT_FOUND = -1;

interface Sibling {
  id: string;
  elItem: HTMLElement;
  /** Left edge in client coordinates, measured before anything moved. */
  left: number;
  /** Top edge in client coordinates, measured before anything moved. */
  top: number;
  width: number;
  height: number;
}

/** Where the pointer is, in client coordinates. */
interface Point {
  x: number;
  y: number;
}

/** Whether the item already draws a surface the lift can float on, so the engine
 *  only paints `LIFT_BACKGROUND` under the rows that draw none. */
function paintsOwnSurface(elItem: HTMLElement) {
  const { backgroundColor, backgroundImage } = getComputedStyle(elItem);
  return backgroundImage !== "none" || colorAlpha(backgroundColor) >= OPAQUE_ALPHA;
}

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
  const overflowSaves: {
    elAncestor: HTMLElement;
    overflow: string;
  }[] = [];

  isDragInProgress = true;
  addEventListener("pointermove", onPointerMove);
  addEventListener("pointerup", onPointerUp);
  addEventListener("pointercancel", onPointerCancel);
  addEventListener("keydown", onKeyDown, true);

  // ── geometry ───────────────────────────────────────────────────────────────

  function axisCenter(slot: Sibling) {
    return isHorizontal ? slot.left + slot.width / 2 : slot.top + slot.height / 2;
  }

  function slotTranslate({ from, to }: {
    from: Sibling;
    to: Sibling;
  }) {
    return `${to.left - from.left}px ${to.top - from.top}px`;
  }

  // Snapshot the visible siblings and their geometry once, the moment the drag
  // begins, so the insertion index is always computed against a stable pre-drag
  // layout rather than the one the gaps are actively shifting. Hidden siblings
  // (display:none → zero size, e.g. an off-screen pane) aren't reorderable.
  function snapshot() {
    siblings = [];
    for (const elChild of elParent.children) {
      if (!(elChild instanceof HTMLElement) || !elChild.matches(itemSelector)) {
        continue;
      }

      const rectangle = elChild.getBoundingClientRect();
      const id = elChild.getAttribute(idAttribute);
      const isReorderable = rectangle.width > 0 && rectangle.height > 0 && id !== null && id !== "";
      if (!isReorderable) {
        continue;
      }

      siblings.push({
        id,
        elItem: elChild,
        left: rectangle.left,
        top: rectangle.top,
        width: rectangle.width,
        height: rectangle.height
      });
    }

    originalIds = siblings.map(sibling => sibling.id);
    iOriginalById = new Map(siblings.map((sibling, i) => [sibling.id, i]));
    centers = siblings.map(axisCenter);
    iFrom = iOriginalById.get(draggedId) ?? NOT_FOUND;
    iCurrent = iFrom;
  }

  // Slide every non-dragged sibling from its original slot to the slot at its new
  // position when the dragged item is inserted at `iBefore` among the others. Each
  // is mapped to a real slot's geometry, so variable-width pills close and open
  // exactly - no uniform-gap assumption. The siblings' transition is set once on
  // lift, so this only writes `translate`.
  function reflow(iBefore: number) {
    const others = originalIds.filter(id => id !== draggedId);
    const order = [...others.slice(0, iBefore), draggedId, ...others.slice(iBefore)];
    for (const [i, id] of order.entries()) {
      const iOriginal = iOriginalById.get(id);
      if (id === draggedId || iOriginal === undefined) {
        continue;
      }

      const sibling = siblings[iOriginal];
      sibling.elItem.style.translate = slotTranslate({
        from: sibling,
        to: siblings[i]
      });
    }
  }

  // The element the pointer is truly over, resolved through the drag shield (and
  // through the lifted item, which already carries `pointerEvents:none`), so the
  // surface beneath both stays reachable.
  function hitElementAt({ x, y }: Point) {
    if (elShield) {
      elShield.style.pointerEvents = "none";
    }

    const elHit = document.elementFromPoint(x, y);
    if (elShield) {
      elShield.style.pointerEvents = "";
    }

    return elHit;
  }

  /** Over the outside drop zone and not over a reorderable item - a drop-out. */
  function isOverOutsideZone(elHit: Element | null) {
    if (!outsideSelector || !elHit) {
      return false;
    }

    return elHit.closest(outsideSelector) !== null && elHit.closest(itemSelector) === null;
  }

  // The pane + half under the pointer, or null - the drop-half decision, made once
  // here (via `reorder.paneDropSide`) for both the live hint and the release.
  function resolveDropTarget({ elHit, x }: {
    elHit: Element | null;
    x: number;
  }) {
    const elTarget = elHit?.closest(PANE_DROP_TARGET.selector);
    if (!elHit || !elTarget || elHit.closest(itemSelector)) {
      return null;
    }

    const id = elTarget.getAttribute(PANE_DROP_TARGET.idAttribute);
    if (id === null || id === "") {
      return null;
    }

    const rectangle = elTarget.getBoundingClientRect();
    return {
      id,
      side: paneDropSide({
        pointerX: x,
        left: rectangle.left,
        width: rectangle.width
      })
    };
  }

  function updateOutsideHints(point: Point) {
    if (!outsideSelector) {
      return;
    }

    const elHit = hitElementAt(point);
    isOutside = isOverOutsideZone(elHit);
    dropTarget = resolveDropTarget({
      elHit,
      x: point.x
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
    raiseShield();

    for (const [i, { elItem }] of siblings.entries()) {
      const { style } = elItem;
      style.position = "relative";

      if (i !== iFrom) {
        style.transition = isReducedMotion ? "none" : `translate ${SIBLING_SECONDS}s var(--ease)`;
        continue;
      }

      style.transition = "none";
      style.zIndex = LIFTED_Z_INDEX;
      style.boxShadow = LIFT_SHADOW;
      style.pointerEvents = "none";
      style.cursor = GRABBING_CURSOR;
      style.borderRadius = LIFTED_RADIUS;

      if (!paintsOwnSurface(elItem)) {
        style.background = LIFT_BACKGROUND;
      }
    }

    unclipAncestors();
  }

  function raiseShield() {
    const elNewShield = document.createElement("div");
    elNewShield.dataset.dragShield = "";
    elNewShield.style.position = "fixed";
    elNewShield.style.inset = "0";
    elNewShield.style.zIndex = SHIELD_Z_INDEX;
    elNewShield.style.cursor = GRABBING_CURSOR;
    elNewShield.style.background = "transparent";
    document.body.appendChild(elNewShield);
    elShield = elNewShield;
  }

  function removeShield() {
    elShield?.remove();
    elShield = null;
  }

  // Neutralize the scroll ancestors' clipping for the gesture, restoring each
  // exactly as it was on drop. An escape-drag (`outsideSelector` - a tab toward
  // the panes, a pane toward the strip) forces them `visible` so the lifted item
  // isn't cut off leaving its container. An in-place sort forces them `hidden`
  // instead: the translated item still extends the container's scrollable area,
  // and an `overflow: auto` ancestor (the runner dock's grid) would otherwise
  // pop scrollbars for the whole drag and settle.
  function unclipAncestors() {
    const isEscapeDrag = Boolean(outsideSelector);
    const neutral = isEscapeDrag ? "visible" : "hidden";
    let elAncestor: HTMLElement | null = elParent;
    while (elAncestor && elAncestor !== document.body) {
      const { overflow } = getComputedStyle(elAncestor);
      const isScrolling = overflow !== "" && overflow !== "visible" && overflow !== "hidden";
      const isClippingEscape = isEscapeDrag && overflow === "hidden";
      if (isScrolling || isClippingEscape) {
        overflowSaves.push({
          elAncestor,
          overflow: elAncestor.style.overflow
        });
        elAncestor.style.overflow = isScrolling ? neutral : "visible";
      }

      elAncestor = elAncestor.parentElement;
    }
  }

  function reclipAncestors() {
    for (const { elAncestor, overflow } of overflowSaves) {
      elAncestor.style.overflow = overflow;
    }

    overflowSaves.length = 0;
  }

  function clearItemStyles() {
    for (const { elItem } of siblings) {
      const { style } = elItem;
      // Clear `transition` first so removing `translate` is instant, never animated.
      style.transition = "";
      style.translate = "";
      style.zIndex = "";
      style.position = "";
      style.background = "";
      style.boxShadow = "";
      style.pointerEvents = "";
      style.cursor = "";
      style.borderRadius = "";
    }
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
      reflow(iBefore);
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
      x: lastPointerX,
      y: lastPointerY
    });
    if (!isOverOutsideZone(elHit)) {
      return false;
    }

    const releaseTarget = resolveDropTarget({
      elHit,
      x: lastPointerX
    });
    clearItemStyles();
    reclipAncestors();
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

    elDragged.style.transition = `translate ${SPRING_SECONDS}s ${SPRING_EASE}, box-shadow ${SPRING_SECONDS}s`;
    elDragged.style.boxShadow = LIFT_SHADOW_FADED;
    requestAnimationFrame(() => {
      elDragged.style.translate = target;
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
    clearItemStyles();
    reclipAncestors();
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
