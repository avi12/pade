// A pointer-drag reorder engine, shared by the session-tab strip and the
// terminal pane row (DRY). One press-and-drag gesture: lift the pressed item,
// track the pointer, shift the other siblings aside to open the slot the item
// will land in, and commit the reordered id array on drop. Pointer events only
// (no HTML5 drag API), so it works identically for mouse, pen, and touch — the
// call sites give their drag handles `touch-action: none` so a touch-drag never
// scrolls instead.
//
// It is framework-light: it manipulates the DOM imperatively (the only way to
// make an element follow the pointer) and reports every state change through
// `onHint`, so a Svelte caller can mirror the hint into `$state` and paint its
// own indicators (the tab → split overlay). Reduced motion is respected — the
// lift/gap transitions are skipped so nothing animates.
//
// The DOM-free order/index math (which slot the pointer lands in, the reordered
// id array) lives in `@/lib/reorder` so it's unit-testable and shared (DRY).

import { insertionIndex, reorderedIds } from "@/lib/reorder";

/** The direction items are laid out in, and the one the reorder tracks. */
export const Axis = {
  Horizontal: "horizontal",
  Vertical: "vertical"
} as const;
export type Axis = (typeof Axis)[keyof typeof Axis];

/** Live state of an in-progress drag, mirrored into the UI so it can paint
 *  insertion hints and out-of-container drop zones. `null` when no drag runs. */
export interface DragHint {
  /** The id of the item being dragged. */
  id: string;
  /** Insertion index among the visible siblings, or `null` while the pointer is
   *  over the registered outside drop zone (then it's a drop-out, not a sort). */
  index: number | null;
  /** True while the pointer is over the `outsideSelector` drop zone. */
  outside: boolean;
  /** Pointer position in client coordinates, so the caller can hit-test its own
   *  drop zones (which pane, which side) without the engine knowing about them. */
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
  /** Called on drop with the visible siblings' ids in their new order. */
  onCommit: (orderedIds: string[]) => void;
  /** Called on every state change (start, index change, enter/leave outside,
   *  end) so the UI can render live indicators. `null` marks the drag over. */
  onHint?: (hint: DragHint | null) => void;
  /** A drop zone outside the item's own container (the terminal panes area). While
   *  the pointer is over it the reorder is suspended and the drag reads as a
   *  drop-out. */
  outsideSelector?: string;
  /** Called instead of `onCommit` when the drop lands on the outside zone. */
  onDropOutside?: (drop: {
    id: string;
    pointerX: number;
    pointerY: number;
  }) => void;
}

const DEFAULT_THRESHOLD = 5;
/** A hook class the call sites may target for extra styling if they wish; the
 *  engine sets the load-bearing lift styles inline so it works without any CSS. */
const DRAGGING_CLASS = "is-reordering";

interface Sibling {
  id: string;
  element: HTMLElement;
  /** Start edge (left/top) along the axis, measured before anything moved. */
  start: number;
  /** Size along the axis. */
  size: number;
  /** Center along the axis — what the dragged item's center is compared against. */
  center: number;
}

function prefersReducedMotion(): boolean {
  return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

/** The container's flex gap, measured between its first two siblings. The whole
 *  shift model rests on the invariant that CSS `gap` is uniform — every pair of
 *  neighbors sits the same distance apart, so removing the dragged item closes,
 *  and re-inserting it opens, exactly `size + gap` everywhere. If items ever
 *  gained individual margins, this measurement (and the model) would break. */
function measureUniformFlexGap(siblings: Sibling[]): number {
  if (siblings.length < 2) {
    return 0;
  }

  const [first, second] = siblings;
  return Math.max(0, second.start - (first.start + first.size));
}

// Only one drag may run at a time — a second press while one is live is ignored.
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
  const parent = pressed?.parentElement ?? null;
  const pressedId = pressed?.getAttribute(idAttribute) ?? null;
  if (!pressed || !parent || pressedId === null) {
    return;
  }

  // Closure-safe non-null bindings: TS drops the guard's narrowing inside the
  // nested handlers below, so re-bind through explicitly typed consts.
  const item: HTMLElement = pressed;
  const container: HTMLElement = parent;
  const draggedId: string = pressedId;

  const isHorizontal = axis === Axis.Horizontal;
  const reduce = prefersReducedMotion();
  const outsideElement = outsideSelector ? document.querySelector(outsideSelector) : null;

  // Snapshot the visible siblings and their geometry once, before anything moves,
  // so the insertion index is always computed against a stable layout rather than
  // the one the gaps are actively shifting. Hidden siblings (display:none → zero
  // size, e.g. an off-screen terminal pane) aren't part of the reorderable set.
  const siblings: Sibling[] = Array.from(container.children)
    .filter((child): child is HTMLElement => child instanceof HTMLElement && child.matches(itemSelector))
    .map(element => {
      const rect = element.getBoundingClientRect();
      const size = isHorizontal ? rect.width : rect.height;
      const start = isHorizontal ? rect.left : rect.top;
      return {
        id: element.getAttribute(idAttribute) ?? "",
        element,
        start,
        size,
        center: start + size / 2
      };
    })
    .filter(sibling => sibling.size > 0 && sibling.id !== "");

  const fromIndex = siblings.findIndex(sibling => sibling.id === draggedId);
  if (fromIndex === -1) {
    return;
  }

  const dragged = siblings[fromIndex];
  // Every displaced sibling moves by exactly the dragged item's size plus one
  // uniform gap, whatever its own width (true even for variable-width pills).
  const draggedExtent = dragged.size + measureUniformFlexGap(siblings);
  // Centers snapshot in visible order — a stable input to `insertionIndex`, so the
  // landing slot is always compared against the pre-drag layout, not the shifting
  // one the gaps are actively opening.
  const centers = siblings.map(sibling => sibling.center);

  const pointerStartX = e.clientX;
  const pointerStartY = e.clientY;
  let dragging = false;
  let currentIndex = fromIndex;
  let outside = false;
  const overflowSaves: {
    element: HTMLElement;
    value: string;
  }[] = [];
  const bodyStyle = document.body.style;
  const bodyUserSelect = bodyStyle.userSelect;
  const bodyCursor = bodyStyle.cursor;

  dragInProgress = true;
  addEventListener("pointermove", onPointerMove);
  addEventListener("pointerup", onPointerUp);
  addEventListener("pointercancel", cleanup);
  addEventListener("keydown", onKeyDown, true);

  // ── geometry ───────────────────────────────────────────────────────────────

  function shiftFor(index: number, targetIndex: number): number {
    if (index > fromIndex && index <= targetIndex) {
      return -draggedExtent;
    }

    if (index < fromIndex && index >= targetIndex) {
      return draggedExtent;
    }

    return 0;
  }

  function applyGaps(targetIndex: number): void {
    siblings.forEach((sibling, i) => {
      if (i === fromIndex) {
        return;
      }

      const shift = shiftFor(i, targetIndex);
      sibling.element.style.transition = reduce ? "" : "translate 170ms var(--ease)";
      sibling.element.style.translate = shift ? offset(shift) : "";
    });
  }

  function clearGaps(): void {
    siblings.forEach((sibling, i) => {
      if (i !== fromIndex) {
        sibling.element.style.translate = "";
      }
    });
  }

  function offset(distance: number): string {
    return isHorizontal ? `${distance}px 0` : `0 ${distance}px`;
  }

  function isOverOutside(point: PointerEvent): boolean {
    if (!outsideElement) {
      return false;
    }

    const rect = outsideElement.getBoundingClientRect();
    return point.clientX >= rect.left
      && point.clientX <= rect.right
      && point.clientY >= rect.top
      && point.clientY <= rect.bottom;
  }

  // ── lift / restore ───────────────────────────────────────────────────────────

  function startDrag(): void {
    dragging = true;
    item.classList.add(DRAGGING_CLASS);
    // Raised above siblings and drop zones; taken out of hit-testing so the outside
    // zone underneath it stays reachable; kept in flow (position:relative) so the
    // siblings' gaps still measure against its original slot.
    item.style.position = "relative";
    item.style.zIndex = "9999";
    item.style.pointerEvents = "none";
    item.style.boxShadow = "0 12px 32px var(--shadow-color)";

    // Transition the lift only — never `translate`, which must track the pointer
    // 1:1. The scale and translate are independent properties, so this is safe.
    if (!reduce) {
      item.style.transition = "scale 120ms var(--ease), box-shadow 160ms var(--ease)";
      item.style.scale = "1.03";
    }

    bodyStyle.userSelect = "none";
    bodyStyle.cursor = "grabbing";
    unclipAncestors();
  }

  // The strip clips its overflow so pills never wrap; a lifted pill leaving the
  // strip (dragged toward the panes) would be cut off. Lift the clip on every
  // ancestor that has one for the duration of the drag, and put each back exactly
  // as it was on drop.
  function unclipAncestors(): void {
    let node: HTMLElement | null = container;
    while (node && node !== document.body) {
      const style = getComputedStyle(node);
      const clips = [style.overflow, style.overflowX, style.overflowY].some(
        value => value !== "" && value !== "visible"
      );
      if (clips) {
        overflowSaves.push({
          element: node,
          value: node.style.overflow
        });
        node.style.overflow = "visible";
      }

      node = node.parentElement;
    }
  }

  function restore(): void {
    overflowSaves.forEach(({ element, value }) => {
      element.style.overflow = value;
    });
    overflowSaves.length = 0;

    item.classList.remove(DRAGGING_CLASS);
    item.style.position = "";
    item.style.zIndex = "";
    item.style.pointerEvents = "";
    item.style.boxShadow = "";
    item.style.transition = "";
    item.style.scale = "";
    item.style.translate = "";
    siblings.forEach(sibling => {
      sibling.element.style.translate = "";
      sibling.element.style.transition = "";
    });

    bodyStyle.userSelect = bodyUserSelect;
    bodyStyle.cursor = bodyCursor;
  }

  // ── events ─────────────────────────────────────────────────────────────────

  function onPointerMove(move: PointerEvent): void {
    const deltaX = move.clientX - pointerStartX;
    const deltaY = move.clientY - pointerStartY;
    if (!dragging) {
      if (Math.hypot(deltaX, deltaY) < threshold) {
        return;
      }

      startDrag();
    }

    // Follow the pointer freely: the reorder tracks the main axis, but the cross
    // axis lets a pill dip down toward the panes to become a split.
    item.style.translate = `${deltaX}px ${deltaY}px`;

    outside = isOverOutside(move);

    if (outside) {
      clearGaps();
      publish(move, null);
      return;
    }

    const draggedCenter = dragged.center + (isHorizontal ? deltaX : deltaY);
    const nextIndex = insertionIndex({
      centers,
      fromIndex,
      draggedCenter
    });
    if (nextIndex !== currentIndex) {
      currentIndex = nextIndex;
      applyGaps(nextIndex);
    }

    publish(move, nextIndex);
  }

  function onPointerUp(up: PointerEvent): void {
    if (!dragging) {
      cleanup();
      return;
    }

    // A real drag must not also fire the click the browser synthesizes on the
    // element under the pointer (which would select the tab we just dropped).
    suppressNextClick();

    if (outside && onDropOutside) {
      onDropOutside({
        id: draggedId,
        pointerX: up.clientX,
        pointerY: up.clientY
      });
      cleanup();
      return;
    }

    if (currentIndex !== fromIndex) {
      const orderedIds = reorderedIds({
        ids: siblings.map(sibling => sibling.id),
        fromIndex,
        toIndex: currentIndex
      });
      onCommit(orderedIds);
    }

    cleanup();
  }

  function onKeyDown(key: KeyboardEvent): void {
    if (key.key !== "Escape") {
      return;
    }

    key.preventDefault();

    if (dragging) {
      suppressNextClick();
    }

    cleanup();
  }

  function publish(point: PointerEvent, index: number | null): void {
    onHint?.({
      id: draggedId,
      index,
      outside,
      pointerX: point.clientX,
      pointerY: point.clientY
    });
  }

  function cleanup(): void {
    removeEventListener("pointermove", onPointerMove);
    removeEventListener("pointerup", onPointerUp);
    removeEventListener("pointercancel", cleanup);
    removeEventListener("keydown", onKeyDown, true);
    restore();
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
