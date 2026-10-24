// Files dragged in from Explorer or an IDE land in the composer as their
// paths, exactly like a Ctrl+V'd image: the agent attaches an image path and
// reads any other path as a file reference. Tauri's native drag events carry
// the absolute paths the web drag API withholds, and every pane hears every
// drop in the window, so each one claims only a drop on its own element.

import { EventType, onMessage } from "@/lib/ipc/messaging";
import { isDragOverElement } from "@/lib/ui/drag/drag-position";

export function listenForFileDrops({ element, onHoverChange, onDrop }: {
  element: () => HTMLElement;
  /** Whether a file is hovering over the element. */
  onHoverChange: (isOver: boolean) => void;
  /** Files were dropped onto the element. */
  onDrop: (paths: string[]) => void;
}) {
  return Promise.all([
    onMessage(EventType.dragOver, ({ data: { position } }) => {
      onHoverChange(
        isDragOverElement({
          element: element(),
          position
        })
      );
    }),
    onMessage(EventType.dragLeave, () => {
      onHoverChange(false);
    }),
    onMessage(EventType.dragDrop, ({ data: { paths, position } }) => {
      onHoverChange(false);
      const isLandingHere = paths.length > 0 && isDragOverElement({
        element: element(),
        position
      });
      if (!isLandingHere) {
        return;
      }

      onDrop(paths);
    })
  ]);
}
