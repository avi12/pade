// Files dragged in from Explorer or an IDE land in the composer as their
// paths, exactly like a Ctrl+V'd image: the agent attaches an image path and
// reads any other path as a file reference. Tauri's native drag events carry
// the absolute paths the web drag API withholds, and every pane hears every
// drop in the window, so each one claims only a drop on its own element.
// A background session's pane keeps its full-size box under `visibility:
// hidden`, so the box alone would let every session in the window claim it.

import { EventType, onMessage } from "@/lib/ipc/messaging";
import type { DragPosition } from "@/lib/types";
import { isDragOverElement } from "@/lib/ui/drag/drag-position";

export function listenForFileDrops({ element, onHoverChange, onDrop }: {
  element: () => HTMLElement;
  /** Whether a file is hovering over the element. */
  onHoverChange: (isOver: boolean) => void;
  /** Files were dropped onto the element. */
  onDrop: (paths: string[]) => void;
}) {
  function isOverShownElement(position: DragPosition) {
    const elTarget = element();
    return elTarget.checkVisibility({
      visibilityProperty: true
    }) && isDragOverElement({
      element: elTarget,
      position
    });
  }

  return Promise.all([
    onMessage(EventType.dragOver, ({ data: { position } }) => {
      onHoverChange(isOverShownElement(position));
    }),
    onMessage(EventType.dragLeave, () => {
      onHoverChange(false);
    }),
    onMessage(EventType.dragDrop, ({ data: { paths, position } }) => {
      onHoverChange(false);
      const isLandingHere = paths.length > 0 && isOverShownElement(position);
      if (!isLandingHere) {
        return;
      }

      onDrop(paths);
    })
  ]);
}
