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

/** In/out transition for a block revealed inside a card (the feed's inline
 *  diff): its height — and the end-margin it brings — glides open and closed
 *  while it fades, so toggling the card is a smooth glide both ways instead of
 *  an appear-then-snap. */
export function revealBlock(node: Element, { duration = 240 }: { duration?: number } = {}) {
  const { height } = node.getBoundingClientRect();
  const marginBlockEnd = Number.parseFloat(getComputedStyle(node).marginBlockEnd) || 0;

  return {
    duration: motionDuration(duration),
    easing: cubicOut,
    css: (progress: number) => `
      overflow: hidden;
      block-size: ${progress * height}px;
      margin-block-end: ${progress * marginBlockEnd}px;
      opacity: ${progress};
    `
  };
}

/** Slide+collapse geometry shared by the in- and out-transitions of a flex-list
 *  row: its block-size and the flex `gap` the row holds glide together with a
 *  fade and a small sideways slide. Svelte drives `progress` 0→1 on the way in
 *  and 1→0 on the way out, so one builder serves both directions. */
function slideRow(node: Element, duration: number) {
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

/** Out-transition for a row that was removed from a list: it fades and slides
 *  aside while its height (and the flex gap it was holding open) collapses, so
 *  the rows below glide up to close the space instead of snapping shut. */
export function collapseRow(node: Element, { duration = 260 }: { duration?: number } = {}) {
  return slideRow(node, duration);
}

/** In-transition mirroring {@link collapseRow} for a row added to a list: it
 *  fades and slides into place as its height (and the flex gap it takes) opens
 *  up, so the rows below glide apart to make room instead of snapping open. */
export function expandRow(node: Element, { duration = 220 }: { duration?: number } = {}) {
  return slideRow(node, duration);
}

/** `animate:flip` duration for a reordering list row, silenced under reduced
 *  motion so survivors jump straight to their new slots. */
export function flipDuration(milliseconds = 220): number {
  return motionDuration(milliseconds);
}
