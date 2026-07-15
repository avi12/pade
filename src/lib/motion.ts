// Motion helpers for things CSS alone can't express. An exit animation is the
// one case: the moment a list drops an item, its element is gone from the DOM,
// so there is nothing left for a CSS transition to run on — the leaving row has
// to be animated out by the framework before it is detached.
//
// theme.css disables CSS animations under `prefers-reduced-motion`, but that
// media query can't reach a JS-driven transition, so every duration here goes
// through motionDuration() and collapses to 0 when reduced motion is asked for.

import { cubicOut } from "svelte/easing";

function prefersReducedMotion(): boolean {
  return globalThis.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function motionDuration(milliseconds: number): number {
  return prefersReducedMotion() ? 0 : milliseconds;
}

/** Out-transition for a row that was removed from a list: it fades and slides
 *  aside while its height (and the flex gap it was holding open) collapses, so
 *  the rows below glide up to close the space instead of snapping shut. */
export function collapseRow(node: Element, { duration = 260 }: { duration?: number } = {}) {
  const { height } = node.getBoundingClientRect();
  // The list's own `gap` still reserves space for a row of zero height; a
  // matching negative end-margin retires it in step with the collapse.
  const gap = node.parentElement
    ? Number.parseFloat(getComputedStyle(node.parentElement).rowGap) || 0
    : 0;

  return {
    duration: motionDuration(duration),
    easing: cubicOut,
    css: (progress: number, remaining: number) => `
      overflow: hidden;
      block-size: ${progress * height}px;
      margin-block-end: ${-gap * remaining}px;
      opacity: ${progress};
      transform: translateX(${-12 * remaining}px) scale(${1 - (0.03 * remaining)});
    `
  };
}
