import { Axis, beginReorder } from "@/lib/ui/drag-reorder";
import type { DragHint, DropOutside } from "@/lib/ui/drag-reorder";

/** The attribute every strip pill carries its session id on - the drag's handle. */
const SESSION_TAB_ATTRIBUTE = "data-session-tab";
// Pixels a pill must travel before a press becomes a drag.
const DRAG_THRESHOLD = 5;

/** Press-and-drag a pill by its body to reorder the strip (past a 5px
 *  threshold), or drag it down over the terminal panes to open it as a split.
 *  The close / AI buttons and the rename field carry `data-noreorder`, so a
 *  press on them is theirs; a plain press-and-release still selects. */
export function beginTabDrag({ e, onreorder, ondraghint, onsplit }: {
  e: PointerEvent;
  onreorder?: (orderedIds: string[]) => void;
  ondraghint?: (hint: DragHint | null) => void;
  onsplit?: (drop: Pick<DropOutside, "id" | "pointerX" | "pointerY">) => void;
}) {
  beginReorder({
    e,
    itemSelector: `[${SESSION_TAB_ATTRIBUTE}]`,
    idAttribute: SESSION_TAB_ATTRIBUTE,
    axis: Axis.Horizontal,
    threshold: DRAG_THRESHOLD,
    ignoreSelector: "[data-noreorder]",
    onCommit: ids => onreorder?.(ids),
    onHint: hint => ondraghint?.(hint),
    outsideSelector: "[data-panes]",
    onDropOutside: drop => onsplit?.(drop)
  });
}
