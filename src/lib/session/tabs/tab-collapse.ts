import { cubicOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";

import { motionDuration } from "@/lib/ui/motion";

const COLLAPSE_DURATION = 240;
// How far a collapsing pill tucks under its neighbour as it closes.
const COLLAPSE_TUCK = 6;

/** The out-transition of a pill being closed: its width and opacity shrink to
 *  nothing as it tucks under its neighbour, with its height pinned so the label
 *  reflow can't grow the row. `width` / `height` are the pill's size as the
 *  outro begins. */
export function collapseTab({ width, height }: { width: number; height: number }) {
  return {
    duration: motionDuration(COLLAPSE_DURATION),
    easing: cubicOut,
    css: progress =>
      `overflow: hidden; block-size: ${height}px; inline-size: ${width * progress}px;` +
      `opacity: ${progress}; margin-inline-start: ${(progress - 1) * COLLAPSE_TUCK}px;`
  } satisfies TransitionConfig;
}
