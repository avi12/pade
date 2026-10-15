// The DOM side of the split panes: hit-testing a pointer against the shown
// panes and springing a newly added pane in. The layout math itself lives in
// `@/lib/session/pane-layout` (DOM-free, unit-tested).

import { paneAtPoint } from "@/lib/session/pane-layout";
import { tick } from "svelte";

// The spring a pane joining a split enters with.
const PANE_ENTER_ANIMATION = "pane-enter 340ms var(--spring)";

/** The shown pane under a point, and which half of it - used both for the live
 *  highlight and for the actual drop, so the two never disagree (DRY). */
export function paneDropAt({ elPanes, paneIds, x, y }: {
  elPanes: HTMLElement | undefined;
  paneIds: readonly string[];
  x: number;
  y: number;
}) {
  const slots = [...elPanes?.querySelectorAll<HTMLElement>("[data-pane-id]") ?? []];
  const panes = slots
    .filter(slot => paneIds.includes(slot.dataset.paneId ?? ""))
    .map(slot => ({
      id: slot.dataset.paneId ?? "",
      rectangle: slot.getBoundingClientRect()
    }));
  return paneAtPoint({
    panes,
    x,
    y
  });
}

/** Spring just the added pane in from its trailing edge (pane-enter). Fired on
 *  the one slot that joined a split - never on a plain tab open / close /
 *  switch, which stay instant - so it can't run on every show the way a CSS rule
 *  on `.shown` would (that wiped the terminal in on any tab change). Clears the
 *  inline animation once it settles so a later show/hide of the same slot can't
 *  replay it. Mirrors the design's imperative `animatePaneIn`. */
export async function animatePaneIn({ elPanes, id }: {
  elPanes: HTMLElement | undefined;
  id: string;
}) {
  await tick();
  const elSlot = elPanes?.querySelector<HTMLElement>(`[data-pane-id="${id}"]`);
  if (!elSlot) {
    return;
  }

  elSlot.style.animation = "none";
  // Force a synchronous reflow between clearing and re-setting the animation so
  // re-adding the same pane restarts the run; getBoundingClientRect flushes
  // layout the same as reading offsetWidth, but as a call it isn't a bare
  // "unused expression" (nor an unused local, which a discard const would be).
  elSlot.getBoundingClientRect();
  elSlot.style.animation = PANE_ENTER_ANIMATION;
  elSlot.addEventListener("animationend", () => (elSlot.style.animation = ""), { once: true });
}
