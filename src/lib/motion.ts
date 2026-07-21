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

// One coordinate of a cubic Bézier with anchors fixed at 0 and 1 (P0 = 0,
// P3 = 1), for progress `t` in [0, 1], shaped by the two control points.
function bezierAxis({ controlOne, controlTwo, t }: {
  controlOne: number;
  controlTwo: number;
  t: number;
}): number {
  const inverse = 1 - t;
  return (3 * inverse * inverse * t * controlOne) + (3 * inverse * t * t * controlTwo) + (t * t * t);
}

/** The M3 "emphasized" easing curve — `cubic-bezier(0.2, 0, 0, 1)`, the same
 *  motion the CSS `--ease` token carries — as an easing function for JS-driven
 *  animations (e.g. `animate:flip`), so JS motion matches the rest of the app.
 *  Solves x(t) = progress by bisection, then returns y(t). */
export function emphasized(progress: number): number {
  let low = 0;
  let high = 1;
  let t = progress;
  for (let step = 0; step < 20; step += 1) {
    t = (low + high) / 2;

    if (bezierAxis({
      controlOne: 0.2,
      controlTwo: 0,
      t
    }) < progress) {
      low = t;
    } else {
      high = t;
    }
  }

  return bezierAxis({
    controlOne: 0,
    controlTwo: 1,
    t
  });
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
  const marginBlockEnd = parseFloat(getComputedStyle(node).marginBlockEnd) || 0;

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
    ? parseFloat(getComputedStyle(node.parentElement).rowGap) || 0
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

/** Out-transition for a split PANE being closed: the flex-row pane collapses its
 *  inline-size — and the column-gap it was holding — to nothing while it fades
 *  and dips slightly in scale, so the surviving panes glide across to fill the
 *  space instead of snapping shut. The inline-axis mirror of {@link collapseRow}
 *  (which does the block axis), for `out:collapsePane` on a pane slot. */
export function collapsePane(node: Element, { duration = 260 }: { duration?: number } = {}) {
  const { width } = node.getBoundingClientRect();
  const gap = node.parentElement
    ? parseFloat(getComputedStyle(node.parentElement).columnGap) || 0
    : 0;

  return {
    duration: motionDuration(duration),
    easing: cubicOut,
    css: (progress: number, remaining: number) => `
      overflow: hidden;
      flex: none;
      inline-size: ${progress * width}px;
      margin-inline-end: ${-gap * remaining}px;
      opacity: ${progress};
      scale: ${1 - (0.02 * remaining)};
    `
  };
}

/** `animate:flip` duration for a reordering list row, silenced under reduced
 *  motion so survivors jump straight to their new slots. */
export function flipDuration(milliseconds = 220): number {
  return motionDuration(milliseconds);
}
