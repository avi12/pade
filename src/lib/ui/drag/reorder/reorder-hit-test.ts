import { paneDropSide } from "@/lib/session/panes/reorder";
import type { Point } from "@/lib/ui/drag/reorder/reorder-geometry";

/** The panes are the one droppable-with-halves target: a tab dragged over the
 *  panes resolves which pane + half it lands on. Its one authoritative home. */
const PANE_DROP_TARGET = {
  selector: "[data-pane-id]",
  idAttribute: "data-pane-id"
} as const;

// The element the pointer is truly over, resolved through the drag shield (and
// through the lifted item, which already carries `pointerEvents:none`), so the
// surface beneath both stays reachable.
export function hitElementAt({ point, elShield }: { point: Point; elShield: HTMLElement | null }) {
  if (elShield) {
    elShield.style.pointerEvents = "none";
  }

  const elHit = document.elementFromPoint(point.x, point.y);
  if (elShield) {
    elShield.style.pointerEvents = "";
  }

  return elHit;
}

/** Over the outside drop zone and not over a reorderable item - a drop-out. */
export function isOverOutsideZone({
  elHit,
  outsideSelector,
  itemSelector
}: {
  elHit: Element | null;
  outsideSelector: string | undefined;
  itemSelector: string;
}) {
  if (!outsideSelector || !elHit) {
    return false;
  }

  return elHit.closest(outsideSelector) !== null && elHit.closest(itemSelector) === null;
}

// The pane + half under the pointer, or null - the drop-half decision, made once
// here (via `reorder.paneDropSide`) for both the live hint and the release.
export function resolveDropTarget({
  elHit,
  x,
  itemSelector
}: {
  elHit: Element | null;
  x: number;
  itemSelector: string;
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
