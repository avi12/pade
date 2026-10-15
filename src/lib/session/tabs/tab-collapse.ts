import { motionDuration } from "@/lib/ui/motion";
import { cubicOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";

const COLLAPSE_DURATION = 240;
// How far a collapsing pill tucks under its neighbour as it closes.
const COLLAPSE_TUCK = 6;

/** The out-transition of a pill being closed: its width and opacity shrink to
 *  nothing as it tucks under its neighbour, with its height pinned so the label
 *  reflow can't grow the row. `width` / `height` are the pill's size as the
 *  outro begins. */
export function collapseTab({ width, height }: {
  width: number;
  height: number;
}) {
  return {
    duration: motionDuration(COLLAPSE_DURATION),
    easing: cubicOut,
    css: t =>
      `overflow: hidden; block-size: ${height}px; inline-size: ${width * t}px;` +
        `opacity: ${t}; margin-inline-start: ${(t - 1) * COLLAPSE_TUCK}px;`
  } satisfies TransitionConfig;
}
