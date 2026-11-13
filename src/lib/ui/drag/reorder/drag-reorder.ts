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

import { createGesture, endGesture, movePointer } from "@/lib/ui/drag/reorder/reorder-gesture";
import { Axis } from "@/lib/ui/drag/reorder/reorder-types";
import type { BeginReorderOptions } from "@/lib/ui/drag/reorder/reorder-types";
import { Key } from "@/lib/ui/keyboard/keys";
import { prefersReducedMotion } from "@/lib/ui/motion";

const DEFAULT_THRESHOLD = 5;
const PRIMARY_BUTTON = 0;

// Only one drag may run at a time - a second press while one is live (including
// the ~300ms settle after release) is ignored so gestures never overlap.
let isDragInProgress = false;

export function beginReorder(options: BeginReorderOptions) {
  const { e, itemSelector, idAttribute, axis, threshold = DEFAULT_THRESHOLD, ignoreSelector } = options;
  // Primary button only - middle-click (close a tab) and right-click stay theirs.
  const isPressIgnored = isDragInProgress || e.button !== PRIMARY_BUTTON;
  if (isPressIgnored) {
    return;
  }

  if (!(e.target instanceof Element)) {
    return;
  }

  // A press that lands on an ignored control is that control's, not a handle.
  const isOnIgnoredControl = ignoreSelector && e.target.closest(ignoreSelector);
  if (isOnIgnoredControl) {
    return;
  }

  const elPressed = e.target.closest<HTMLElement>(itemSelector);
  const elParent = elPressed?.parentElement ?? null;
  const draggedId = elPressed?.getAttribute(idAttribute) ?? null;
  if (!elPressed || !elParent || draggedId === null) {
    return;
  }

  const gesture = createGesture({
    options,
    elParent,
    draggedId,
    isHorizontal: axis === Axis.HORIZONTAL,
    isReducedMotion: prefersReducedMotion(),
    threshold,
    release() {
      isDragInProgress = false;
    }
  });

  isDragInProgress = true;
  addEventListener("pointermove", onPointerMove);
  addEventListener("pointerup", onPointerUp);
  addEventListener("pointercancel", onPointerCancel);
  addEventListener("keydown", onKeyDown, true);

  function onPointerMove(event: PointerEvent) {
    const isTracking = movePointer({
      gesture,
      e: event
    });
    if (!isTracking) {
      finish(true);
    }
  }

  function onPointerUp(event: PointerEvent) {
    gesture.lastPointerX = event.clientX;
    gesture.lastPointerY = event.clientY;

    if (gesture.isDragging) {
      // A real drag must not also fire the click the browser synthesizes under the
      // pointer (which would select/rename the item we just dropped).
      suppressNextClick();
    }

    finish(false);
  }

  function onPointerCancel() {
    finish(true);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key !== Key.ESCAPE) {
      return;
    }

    event.preventDefault();

    if (gesture.isDragging) {
      suppressNextClick();
    }

    finish(true);
  }

  function finish(isCancelled: boolean) {
    removeEventListener("pointermove", onPointerMove);
    removeEventListener("pointerup", onPointerUp);
    removeEventListener("pointercancel", onPointerCancel);
    removeEventListener("keydown", onKeyDown, true);
    endGesture({
      gesture,
      isCancelled
    });
  }
}

// Swallow the very next click anywhere (capture phase, one shot) so a drop can't
// double as a click. A macrotask retires the guard if the browser fires no click
// at all (it usually doesn't after a long drag), so it never eats a later click.
function suppressNextClick() {
  function swallow(event: Event) {
    event.stopPropagation();

    if (event.cancelable) {
      event.preventDefault();
    }

    removeEventListener("click", swallow, true);
  }

  addEventListener("click", swallow, true);
  setTimeout(() => removeEventListener("click", swallow, true), 0);
}
