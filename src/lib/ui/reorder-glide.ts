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

/** `key` when its row sits higher in the new order than in the painted one -
 *  the row a scroll should follow - and nothing otherwise. */
export function risenKey({ key, from, to }: {
  key: string | undefined;
  from: readonly string[];
  to: readonly string[];
}) {
  if (key === undefined) {
    return undefined;
  }

  const iFrom = from.indexOf(key);
  const iTo = to.indexOf(key);
  const hasRisen = iFrom !== -1 && iTo !== -1 && iTo < iFrom;
  return hasRisen ? key : undefined;
}

/** One list's glide: attach `row(key)` to each keyed row, and call
 *  `beforeReorder` from an `$effect.pre` that reads the list's order. `duration`
 *  is read when a glide starts, so a reduced-motion preference turns it off (0). */
export function createReorderGlide({ duration }: { duration: () => number }) {
  const rows = new Map<string, HTMLElement>();
  let paintedBoxes: Map<HTMLElement, DOMRect> | null = null;
  let followedKey: string | null = null;
  const runningGlides = new Set<Animation>();

  // A glide still running offsets its row, so it is dropped before the rows
  // are measured or scrolled to: each row then reads at its real new place,
  // while `paintedBoxes` already holds where the running glide had it.
  function cancelRunningGlides() {
    for (const animation of runningGlides) {
      animation.cancel();
    }
    runningGlides.clear();
  }

  // The followed row is scrolled into view before the new boxes are measured,
  // so it glides from where it was painted straight to where the scroll put
  // it, and the scroll travels with it instead of leaving it out of sight.
  function scrollFollowedRowIntoView() {
    const elFollowed = followedKey === null ? undefined : rows.get(followedKey);
    followedKey = null;
    elFollowed?.scrollIntoView({ block: "nearest" });
  }

  function glide() {
    const boxes = paintedBoxes;
    paintedBoxes = null;
    cancelRunningGlides();
    scrollFollowedRowIntoView();
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
      runningGlides.add(
        row.animate([{ translate: `${offset.x}px ${offset.y}px` }, { translate: "0 0" }], {
          duration: milliseconds,
          easing
        })
      );
    }
  }

  return {
    row: (key: string): Attachment<HTMLElement> => element => {
      rows.set(key, element);
      return () => {
        if (rows.get(key) === element) {
          rows.delete(key);
        }
      };
    },
    /** Record where every row is painted, once per frame, and glide them from
     *  there once the DOM has its new order. A `follow` row is kept in view. */
    beforeReorder({ follow }: { follow?: string } = {}) {
      followedKey = follow ?? followedKey;

      if (paintedBoxes !== null) {
        return;
      }

      paintedBoxes = new Map([...rows.values()].map(row => [row, row.getBoundingClientRect()]));
      requestAnimationFrame(glide);
    }
  };
}
