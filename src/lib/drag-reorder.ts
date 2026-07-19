// The one pointer-drag engine (Tesler's Law: all the drag complexity lives here),
// shared by the session-tab strip, the terminal pane row, and the pinned-project
// list (DRY). One press-and-drag gesture: lift the pressed item, track the
// pointer, slide the siblings aside to open the slot it will land in, and — on a
// normal drop — spring the lifted item from where it was released into that slot
// before committing the new order. Pointer events only (no HTML5 drag API), so it
// works the same for mouse, pen, and touch; the call sites give their handles
// `touch-action: none` so a touch-drag never scrolls instead.
//
// The FEEL is the design mockup's engine, reproduced faithfully:
//   • Spring-settle from the release point — the lifted item animates
//     (`translate .3s cubic-bezier(.34,1.35,.5,1)` with the shadow fading out) from
//     wherever it was let go into its target slot, and the reordered array is only
//     committed ~300ms later, once it has settled. The engine owns the whole drop
//     animation, so the tab/pane lists must NOT also run `animate:flip` (double
//     animation) — the parent removes it.
//   • A transparent drag shield (`data-drag-shield`) under the lifted item carries
//     the grabbing cursor over every surface (xterm canvases included) and freezes
//     stray interaction; it is dropped from hit-testing around each `elementFromPoint`
//     probe so the real drop zone underneath stays reachable.
//   • Unclip / reclip — a tab dragged OUT of the horizontally-scrolling strip would
//     be cut off, so on lift every clipping ancestor's overflow is forced `visible`
//     and restored on drop.
//   • Sticky insertion — the landing slot excludes the dragged item's *current*
//     gap, so a swapped item holds until the pointer crosses the neighbour's new
//     centre (no flicker at the boundary). Reduced motion skips the transitions.
//
// The DOM-free order/geometry math (`insertionIndex`, `reorderedIds`, `paneDropSide`)
// lives in `@/lib/reorder` so it's unit-tested and shared — the which-half decision
// has one home there. This module measures geometry, drives the DOM imperatively,
// and reports every state change through `onHint` so a Svelte caller can mirror it
// into `$state` and paint its own affordances. The deferred commit awaits Svelte's
// `tick` so the reordered re-render and the inline-style teardown coincide in one
// frame (no flash).

import { DropSide, insertionIndex, paneDropSide, reorderedIds } from "@/lib/reorder";
import { tick } from "svelte";

/** The direction items are laid out in, and the one the reorder tracks. */
export const Axis = {
  Horizontal: "horizontal",
  Vertical: "vertical"
} as const;
export type Axis = (typeof Axis)[keyof typeof Axis];

/** A droppable-with-halves target inside the outside zone — a shown terminal pane.
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
   *  reorderable item — a drop-out (split / pop-to-tab), not an in-strip sort. */
  outside: boolean;
  /** The pane under the pointer and the half it's over, or `null` when the pointer
   *  isn't over a pane. The engine owns this decision (via `reorder.paneDropSide`),
   *  so a caller can paint the drop half straight from it without re-hit-testing. */
  dropTarget: DropTarget | null;
  /** Raw pointer position — retained so a caller may still hit-test its own zones,
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
  /** CSS selector for a reorderable item — matched with `closest` from the press
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
 *  overshoot — the design's drop settle, distinct from the emphasized `--ease` the
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
const LIFTED_Z_INDEX = "130";
const SHIELD_Z_INDEX = "120";
const LIFTED_RADIUS = "0.75rem";

interface Sibling {
  id: string;
  element: HTMLElement;
  /** Left edge in client coordinates, measured before anything moved. */
  left: number;
  /** Top edge in client coordinates, measured before anything moved. */
  top: number;
  width: number;
  height: number;
}

function prefersReducedMotion(): boolean {
  return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

// Only one drag may run at a time — a second press while one is live (including
// the ~300ms settle after release) is ignored so gestures never overlap.
let dragInProgress = false;

export function beginReorder(options: BeginReorderOptions): void {
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
  // Primary button only — middle-click (close a tab) and right-click stay theirs.
  if (dragInProgress || e.button !== 0 || !(e.target instanceof Element)) {
    return;
  }

  // A press that lands on an ignored control is that control's, not a handle.
  if (ignoreSelector && e.target.closest(ignoreSelector)) {
    return;
  }

  const pressed = e.target.closest<HTMLElement>(itemSelector);
  const pressedParent = pressed?.parentElement ?? null;
  const pressedId = pressed?.getAttribute(idAttribute) ?? null;
  if (!pressed || !pressedParent || pressedId === null) {
    return;
  }

  // Closure-safe non-null bindings: TS drops the guard's narrowing inside the
  // nested handlers below, so re-bind through explicitly typed consts.
  const parent: HTMLElement = pressedParent;
  const draggedId: string = pressedId;
  const isHorizontal = axis === Axis.Horizontal;
  const reduce = prefersReducedMotion();

  const pointerStartX = e.clientX;
  const pointerStartY = e.clientY;
  const bodyStyle = document.body.style;
  const bodyUserSelect = bodyStyle.userSelect;
  const parentPointerEvents = parent.style.pointerEvents;

  // Mutable gesture state — all snapshotted lazily once the drag actually begins.
  let dragging = false;
  let siblings: Sibling[] = [];
  let centers: number[] = [];
  let originalIds: string[] = [];
  let originalIndexById = new Map<string, number>();
  let fromIndex = -1;
  let currentIndex = -1;
  let lastBefore = -1;
  let grabOffsetX = 0;
  let grabOffsetY = 0;
  let lastPointerX = pointerStartX;
  let lastPointerY = pointerStartY;
  let outside = false;
  let dropTarget: DropTarget | null = null;
  // A transparent full-window sheet under the lifted item: it shows the grabbing
  // cursor over every surface and swallows stray hover/click. Disabled around each
  // hit-test so `elementFromPoint` reaches the drop zone below it.
  let shield: HTMLElement | null = null;
  const overflowSaves: {
    element: HTMLElement;
    overflow: string;
  }[] = [];

  dragInProgress = true;
  addEventListener("pointermove", onPointerMove);
  addEventListener("pointerup", onPointerUp);
  addEventListener("pointercancel", onPointerCancel);
  addEventListener("keydown", onKeyDown, true);

  // ── geometry ───────────────────────────────────────────────────────────────

  function axisCenter(slot: Sibling): number {
    return isHorizontal ? slot.left + slot.width / 2 : slot.top + slot.height / 2;
  }

  function slotTranslate(from: Sibling, to: Sibling): string {
    return `${to.left - from.left}px ${to.top - from.top}px`;
  }

  // Snapshot the visible siblings and their geometry once, the moment the drag
  // begins, so the insertion index is always computed against a stable pre-drag
  // layout rather than the one the gaps are actively shifting. Hidden siblings
  // (display:none → zero size, e.g. an off-screen pane) aren't reorderable.
  function snapshot(): void {
    siblings = [];
    for (const child of parent.children) {
      if (!(child instanceof HTMLElement) || !child.matches(itemSelector)) {
        continue;
      }

      const rect = child.getBoundingClientRect();
      const id = child.getAttribute(idAttribute);
      if (rect.width <= 0 || rect.height <= 0 || id === null || id === "") {
        continue;
      }

      siblings.push({
        id,
        element: child,
        left: rect.left,
        top: rect.top,
        width: rect.width,
        height: rect.height
      });
    }

    originalIds = siblings.map(sibling => sibling.id);
    originalIndexById = new Map(siblings.map((sibling, index) => [sibling.id, index]));
    centers = siblings.map(axisCenter);
    fromIndex = originalIndexById.get(draggedId) ?? -1;
    currentIndex = fromIndex;
  }

  // Slide every non-dragged sibling from its original slot to the slot at its new
  // position when the dragged item is inserted at `before` among the others. Each
  // is mapped to a real slot's geometry, so variable-width pills close and open
  // exactly — no uniform-gap assumption. The siblings' transition is set once on
  // lift, so this only writes `translate`.
  function reflow(before: number): void {
    const others = originalIds.filter(id => id !== draggedId);
    const order = [...others.slice(0, before), draggedId, ...others.slice(before)];
    for (let newPosition = 0; newPosition < order.length; newPosition++) {
      const id = order[newPosition];
      if (id === draggedId) {
        continue;
      }

      const originalPosition = originalIndexById.get(id);
      if (originalPosition === undefined) {
        continue;
      }

      const sibling = siblings[originalPosition];
      sibling.element.style.translate = slotTranslate(sibling, siblings[newPosition]);
    }
  }

  // The element the pointer is truly over, resolved through the drag shield (and
  // through the lifted item, which already carries `pointerEvents:none`), so the
  // surface beneath both stays reachable.
  function hitElementAt(x: number, y: number): Element | null {
    if (shield) {
      shield.style.pointerEvents = "none";
    }

    const element = document.elementFromPoint(x, y);
    if (shield) {
      shield.style.pointerEvents = "";
    }

    return element;
  }

  // The pane + half under the pointer, or null — the drop-half decision, made once
  // here (via `reorder.paneDropSide`) for both the live hint and the release.
  function resolveDropTarget(hit: Element | null, x: number): DropTarget | null {
    if (!hit) {
      return null;
    }

    const targetElement = hit.closest(PANE_DROP_TARGET.selector);
    if (!targetElement || hit.closest(itemSelector)) {
      return null;
    }

    const id = targetElement.getAttribute(PANE_DROP_TARGET.idAttribute);
    if (id === null || id === "") {
      return null;
    }

    const rect = targetElement.getBoundingClientRect();
    return {
      id,
      side: paneDropSide({
        pointerX: x,
        left: rect.left,
        width: rect.width
      })
    };
  }

  function updateOutsideHints(x: number, y: number): void {
    if (!outsideSelector) {
      return;
    }

    const hit = hitElementAt(x, y);
    outside = hit !== null && hit.closest(outsideSelector) !== null && hit.closest(itemSelector) === null;
    dropTarget = resolveDropTarget(hit, x);
  }

  // ── lift / restore ───────────────────────────────────────────────────────────

  function startDrag(): void {
    dragging = true;
    snapshot();

    if (fromIndex === -1) {
      return;
    }

    const draggedSlot = siblings[fromIndex];
    grabOffsetX = pointerStartX - draggedSlot.left;
    grabOffsetY = pointerStartY - draggedSlot.top;

    bodyStyle.userSelect = "none";
    parent.style.pointerEvents = "none";
    raiseShield();

    for (let index = 0; index < siblings.length; index++) {
      const { style } = siblings[index].element;
      style.position = "relative";

      if (index === fromIndex) {
        style.transition = "none";
        style.zIndex = LIFTED_Z_INDEX;
        style.boxShadow = LIFT_SHADOW;
        style.pointerEvents = "none";
        style.cursor = "grabbing";
        style.borderRadius = LIFTED_RADIUS;
      } else {
        style.transition = reduce ? "none" : `translate ${SIBLING_SECONDS}s var(--ease)`;
      }
    }

    unclipAncestors();
  }

  function raiseShield(): void {
    const element = document.createElement("div");
    element.dataset.dragShield = "";
    element.style.position = "fixed";
    element.style.inset = "0";
    element.style.zIndex = SHIELD_Z_INDEX;
    element.style.cursor = "grabbing";
    element.style.background = "transparent";
    document.body.appendChild(element);
    shield = element;
  }

  function removeShield(): void {
    shield?.remove();
    shield = null;
  }

  // The strip clips its overflow so pills never wrap; a lifted pill dragged out of
  // it (toward the panes) would be cut off. Force every clipping ancestor visible
  // for the drag, and put each back exactly as it was on drop.
  function unclipAncestors(): void {
    if (!outsideSelector) {
      return;
    }

    let node: HTMLElement | null = parent;
    while (node && node !== document.body) {
      const { overflow } = getComputedStyle(node);
      if (overflow !== "" && overflow !== "visible") {
        overflowSaves.push({
          element: node,
          overflow: node.style.overflow
        });
        node.style.overflow = "visible";
      }

      node = node.parentElement;
    }
  }

  function reclipAncestors(): void {
    for (const { element, overflow } of overflowSaves) {
      element.style.overflow = overflow;
    }

    overflowSaves.length = 0;
  }

  function clearItemStyles(): void {
    for (const sibling of siblings) {
      const { style } = sibling.element;
      // Clear `transition` first so removing `translate` is instant, never animated.
      style.transition = "";
      style.translate = "";
      style.zIndex = "";
      style.position = "";
      style.boxShadow = "";
      style.pointerEvents = "";
      style.cursor = "";
      style.borderRadius = "";
    }
  }

  // ── events ─────────────────────────────────────────────────────────────────

  function onPointerMove(move: PointerEvent): void {
    lastPointerX = move.clientX;
    lastPointerY = move.clientY;

    if (!dragging) {
      const travelled = Math.hypot(move.clientX - pointerStartX, move.clientY - pointerStartY);
      if (travelled < threshold) {
        return;
      }

      startDrag();

      if (fromIndex === -1) {
        finish(true);
        return;
      }
    }

    const draggedSlot = siblings[fromIndex];
    const projectedLeft = move.clientX - grabOffsetX;
    const projectedTop = move.clientY - grabOffsetY;
    draggedSlot.element.style.translate = `${projectedLeft - draggedSlot.left}px ${projectedTop - draggedSlot.top}px`;

    // Exclude the dragged item's *current* gap (not its origin) from the count, so
    // a swapped item stays put until the pointer crosses the neighbour's new centre.
    const projectedCenter = isHorizontal
      ? projectedLeft + draggedSlot.width / 2
      : projectedTop + draggedSlot.height / 2;
    const before = insertionIndex({
      centers,
      fromIndex: currentIndex,
      draggedCenter: projectedCenter
    });
    if (before !== lastBefore) {
      lastBefore = before;
      currentIndex = before;
      reflow(before);
    }

    updateOutsideHints(move.clientX, move.clientY);
    onHint?.({
      id: draggedId,
      outside,
      dropTarget,
      pointerX: move.clientX,
      pointerY: move.clientY
    });
  }

  function onPointerUp(up: PointerEvent): void {
    lastPointerX = up.clientX;
    lastPointerY = up.clientY;

    if (dragging) {
      // A real drag must not also fire the click the browser synthesizes under the
      // pointer (which would select/rename the item we just dropped).
      suppressNextClick();
    }

    finish(false);
  }

  function onPointerCancel(): void {
    finish(true);
  }

  function onKeyDown(key: KeyboardEvent): void {
    if (key.key !== "Escape") {
      return;
    }

    key.preventDefault();

    if (dragging) {
      suppressNextClick();
    }

    finish(true);
  }

  // Tear down input, drop the shield, then either pop out (outside), spring home
  // (cancel), or spring into the new slot and commit (normal drop).
  function finish(cancel: boolean): void {
    removeEventListener("pointermove", onPointerMove);
    removeEventListener("pointerup", onPointerUp);
    removeEventListener("pointercancel", onPointerCancel);
    removeEventListener("keydown", onKeyDown, true);
    bodyStyle.userSelect = bodyUserSelect;
    removeShield();
    parent.style.pointerEvents = parentPointerEvents;

    if (!dragging) {
      dragInProgress = false;
      return;
    }

    dragging = false;

    // The pressed item vanished before it could lift — nothing to settle or commit.
    if (fromIndex === -1) {
      onHint?.(null);
      dragInProgress = false;
      return;
    }

    // Re-hit-test at the release point (the lifted item is still pointer-transparent,
    // so the zone beneath is reachable): a genuine drop-out pops the item out.
    if (!cancel && onDropOutside && outsideSelector) {
      const hit = hitElementAt(lastPointerX, lastPointerY);
      const overOutside = hit !== null && hit.closest(outsideSelector) !== null && hit.closest(itemSelector) === null;
      if (overOutside) {
        const releaseTarget = resolveDropTarget(hit, lastPointerX);
        clearItemStyles();
        reclipAncestors();
        onHint?.(null);
        onDropOutside({
          id: draggedId,
          dropTarget: releaseTarget,
          pointerX: lastPointerX,
          pointerY: lastPointerY
        });
        dragInProgress = false;
        return;
      }
    }

    settleIntoSlot(cancel);
  }

  // Spring the lifted item from its release point to the target slot (the open gap,
  // or home on cancel), fading the shadow out; commit the reorder only once it has
  // settled, so nothing jumps.
  function settleIntoSlot(cancel: boolean): void {
    const draggedElement = siblings[fromIndex].element;
    const settleIndex = cancel || lastBefore < 0 ? fromIndex : currentIndex;
    const target = slotTranslate(siblings[fromIndex], siblings[settleIndex]);
    const orderChanged = !cancel && currentIndex !== fromIndex;
    if (reduce) {
      draggedElement.style.translate = target;
      queueMicrotask(async () => {
        await finalizeCommit(orderChanged);
      });
      return;
    }

    draggedElement.style.transition = `translate ${SPRING_SECONDS}s ${SPRING_EASE}, box-shadow ${SPRING_SECONDS}s`;
    draggedElement.style.boxShadow = LIFT_SHADOW_FADED;
    requestAnimationFrame(() => {
      draggedElement.style.translate = target;
    });
    setTimeout(async () => {
      await finalizeCommit(orderChanged);
    }, SPRING_MILLISECONDS);
  }

  async function finalizeCommit(orderChanged: boolean): Promise<void> {
    reclipAncestors();

    if (orderChanged) {
      onCommit(
        reorderedIds({
          ids: originalIds,
          fromIndex,
          toIndex: currentIndex
        })
      );
      // Let Svelte apply the reordered DOM before stripping the inline translates,
      // so the two coincide in one frame and the drop never flashes.
      await tick();
    }

    clearItemStyles();
    onHint?.(null);
    dragInProgress = false;
  }
}

// Swallow the very next click anywhere (capture phase, one shot) so a drop can't
// double as a click. A macrotask retires the guard if the browser fires no click
// at all (it usually doesn't after a long drag), so it never eats a later click.
function suppressNextClick(): void {
  function swallow(click: Event): void {
    click.stopPropagation();

    if (click.cancelable) {
      click.preventDefault();
    }

    removeEventListener("click", swallow, true);
  }

  addEventListener("click", swallow, true);
  setTimeout(() => removeEventListener("click", swallow, true), 0);
}
