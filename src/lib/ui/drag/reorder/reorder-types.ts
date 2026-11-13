import type { DropSide } from "@/lib/session/panes/reorder";

/** The direction items are laid out in, and the one the reorder tracks. */
export const Axis = {
  HORIZONTAL: "horizontal",
  VERTICAL: "vertical"
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
