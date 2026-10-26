// Rows of a keyed list glide to their new place when the order changes (FLIP).
// Svelte's own `animate:flip` measures the rows on every reconcile of the list,
// so a list that re-renders twice in one tick - a session starting updates the
// sessions, and then the chat registry derived from them - measures the rows
// where they already moved to and plays nothing. This measures once per frame:
// the first reorder of a frame records where the rows were painted, and the
// glide runs from there right before the next paint.

import type { Attachment } from "svelte/attachments";

interface Offset {
  x: number;
  y: number;
}

/** How far a row's new box sits from where it was painted - the distance its
 *  glide starts at. */
export function glideOffset({ from, to }: {
  from: Pick<DOMRect, "left" | "top">;
  to: Pick<DOMRect, "left" | "top">;
}): Offset {
  return {
    x: from.left - to.left,
    y: from.top - to.top
  };
}

function hasMoved(offset: Offset) {
  return offset.x !== 0 || offset.y !== 0;
}

/** One list's glide: attach `row` to each keyed row, and call `beforeReorder`
 *  from an `$effect.pre` that reads the list's order. `duration` is read when a
 *  glide starts, so a reduced-motion preference turns it off (0). */
export function createReorderGlide({ duration }: { duration: () => number }) {
  const rows = new Set<HTMLElement>();
  let paintedBoxes: Map<HTMLElement, DOMRect> | null = null;

  function glide() {
    const boxes = paintedBoxes;
    paintedBoxes = null;
    const milliseconds = duration();
    if (boxes === null || milliseconds === 0) {
      return;
    }

    for (const [row, from] of boxes) {
      const offset = glideOffset({
        from,
        to: row.getBoundingClientRect()
      });
      if (!row.isConnected || !hasMoved(offset)) {
        continue;
      }

      // The palette's own curve, so the glide eases like every other move.
      const easing = getComputedStyle(row).getPropertyValue("--ease").trim() || "ease-out";
      row.animate([{ translate: `${offset.x}px ${offset.y}px` }, { translate: "0 0" }], {
        duration: milliseconds,
        easing
      });
    }
  }

  return {
    row: (element => {
      rows.add(element);
      return () => {
        rows.delete(element);
      };
    }) satisfies Attachment<HTMLElement>,
    /** Record where every row is painted, once per frame, and glide them from
     *  there once the DOM has its new order. */
    beforeReorder() {
      if (paintedBoxes !== null) {
        return;
      }

      paintedBoxes = new Map([...rows].map(row => [row, row.getBoundingClientRect()]));
      requestAnimationFrame(glide);
    }
  };
}
