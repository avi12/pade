// Motion helpers for things CSS alone can't express. An exit animation is the
// one case: the moment a list drops an item, its element is gone from the DOM,
// so there is nothing left for a CSS transition to run on - the leaving row has
// to be animated out by the framework before it is detached.
//
// theme.css disables CSS animations under `prefers-reduced-motion`, but that
// media query can't reach a JS-driven transition, so every duration here goes
// through motionDuration() and collapses to 0 when reduced motion is asked for.

import { cubicOut } from "svelte/easing";

const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

/** Bisection rounds: each halves the interval, so twenty pin t to about a millionth. */
const BISECTION_STEPS = 20;
const EMPHASIZED_X_CONTROL_ONE = 0.2;
const EMPHASIZED_X_CONTROL_TWO = 0;
const EMPHASIZED_Y_CONTROL_ONE = 0;
const EMPHASIZED_Y_CONTROL_TWO = 1;

const REVEAL_DURATION = 240;
const COLLAPSE_DURATION = 260;
const EXPAND_DURATION = 220;
const GROW_INLINE_DURATION = 240;
const FLIP_DURATION = 220;

/** How far a sliding row travels sideways, in pixels, and how much it shrinks. */
const ROW_SLIDE_DISTANCE = 12;
const ROW_SHRINK = 0.03;
const PANE_SHRINK = 0.02;

/** Whether the user asked for reduced motion - the one home for that query, read
 *  by every JS-driven animation (false where there is no `matchMedia`, e.g. a
 *  node test run). */
export function prefersReducedMotion() {
  return globalThis.matchMedia?.(REDUCED_MOTION_QUERY).matches ?? false;
}

// One coordinate of a cubic Bézier with anchors fixed at 0 and 1 (P0 = 0,
// P3 = 1), for progress `t` in [0, 1], shaped by the two control points.
function bezierAxis({ controlOne, controlTwo, curveProgress }: {
  controlOne: number;
  controlTwo: number;
  curveProgress: number;
}) {
  const inverse = 1 - curveProgress;
  return (3 * inverse * inverse * curveProgress * controlOne)
    + (3 * inverse * curveProgress * curveProgress * controlTwo)
    + (curveProgress * curveProgress * curveProgress);
}

/** The M3 "emphasized" easing curve - `cubic-bezier(0.2, 0, 0, 1)`, the same
 *  motion the CSS `--ease` token carries - as an easing function for JS-driven
 *  animations (e.g. `animate:flip`), so JS motion matches the rest of the app.
 *  Solves x(t) = progress by bisection, then returns y(t). */
export function emphasized(progress: number) {
  let low = 0;
  let high = 1;
  let adjustedProgress = progress;
  for (let step = 0; step < BISECTION_STEPS; step += 1) {
    adjustedProgress = (low + high) / 2;

    const isBelowTarget = bezierAxis({
      controlOne: EMPHASIZED_X_CONTROL_ONE,
      controlTwo: EMPHASIZED_X_CONTROL_TWO,
      curveProgress: adjustedProgress
    }) < progress;
    if (isBelowTarget) {
      low = adjustedProgress;
    } else {
      high = adjustedProgress;
    }
  }

  return bezierAxis({
    controlOne: EMPHASIZED_Y_CONTROL_ONE,
    controlTwo: EMPHASIZED_Y_CONTROL_TWO,
    curveProgress: adjustedProgress
  });
}

/** A JS-driven transition's duration, collapsed to 0 under reduced motion. */
export function motionDuration(milliseconds: number) {
  return prefersReducedMotion() ? 0 : milliseconds;
}

/** In/out transition for a block revealed inside a card (the feed's inline
 *  diff): its height - and the end-margin it brings - glides open and closed
 *  while it fades, so toggling the card is a smooth glide both ways instead of
 *  an appear-then-snap. */
export function revealBlock(node: Element, { duration = REVEAL_DURATION }: { duration?: number } = {}) {
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
function slideRow({ node, duration }: {
  node: Element;
  duration: number;
}) {
  const { height } = node.getBoundingClientRect();
  // The list's own `gap` still reserves space for a row of zero height; a
  // matching negative end-margin retires it in step with the collapse.
  const gap = node.parentElement
    ? parseFloat(getComputedStyle(node.parentElement).rowGap) || 0
    : 0;
  // An element's own leading margin (an error line's 6px offset) collapses in
  // step too - left at full size it would pop the exact instant the row's
  // height reaches zero.
  const marginBlockStart = parseFloat(getComputedStyle(node).marginBlockStart) || 0;

  return {
    duration: motionDuration(duration),
    easing: cubicOut,
    // `clip`, not `hidden`: a hidden overflow makes the row a scroll
    // container, so a sticky header inside it would stick to the row instead
    // of the list, sitting low until the transition ends and then jumping.
    css: (progress: number, remaining: number) => `
      overflow: clip;
      block-size: ${progress * height}px;
      margin-block-start: ${progress * marginBlockStart}px;
      margin-block-end: ${-gap * remaining}px;
      opacity: ${progress};
      transform: translateX(${-ROW_SLIDE_DISTANCE * remaining}px) scale(${1 - (ROW_SHRINK * remaining)});
    `
  };
}

/** Out-transition for a row that was removed from a list: it fades and slides
 *  aside while its height (and the flex gap it was holding open) collapses, so
 *  the rows below glide up to close the space instead of snapping shut. */
export function collapseRow(node: Element, { duration = COLLAPSE_DURATION }: { duration?: number } = {}) {
  return slideRow({
    node,
    duration
  });
}

/** In-transition mirroring {@link collapseRow} for a row added to a list: it
 *  fades and slides into place as its height (and the flex gap it takes) opens
 *  up, so the rows below glide apart to make room instead of snapping open. */
export function expandRow(node: Element, { duration = EXPAND_DURATION }: { duration?: number } = {}) {
  return slideRow({
    node,
    duration
  });
}

/** Both ways at once for a row that can leave and come straight back (a list
 *  a search narrows): one reversible transition, so a row filtered out and
 *  back in mid-flight glides back from where it is. Separate in- and
 *  out-transitions restart from nothing instead, toward a height measured
 *  mid-collapse, and snap at the end. */
export function toggleRow(node: Element, { duration = COLLAPSE_DURATION }: { duration?: number } = {}) {
  return slideRow({
    node,
    duration
  });
}

/** Out-transition for a split PANE being closed: the flex-row pane collapses its
 *  inline-size - and the column-gap it was holding - to nothing while it fades
 *  and dips slightly in scale, so the surviving panes glide across to fill the
 *  space instead of snapping shut. The inline-axis mirror of {@link collapseRow}
 *  (which does the block axis), for `out:collapsePane` on a pane slot. */
export function collapsePane(node: Element, { duration = COLLAPSE_DURATION }: { duration?: number } = {}) {
  return slideInline({
    node,
    duration
  });
}

/** In/out transition for an inline item that comes and goes in a flex row (a
 *  usage pill's extra-usage segment, its reset chip): it opens and closes its
 *  width - and the gap it holds - while it fades, so its neighbours glide
 *  aside instead of jumping. */
export function growInline(node: Element, { duration = GROW_INLINE_DURATION }: { duration?: number } = {}) {
  return slideInline({
    node,
    duration
  });
}

/** Inline-axis collapse geometry shared by {@link collapsePane} and
 *  {@link growInline}: Svelte drives `progress` 1→0 on the way out and 0→1 on
 *  the way in, so one builder serves both. */
function slideInline({ node, duration }: {
  node: Element;
  duration: number;
}) {
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
      scale: ${1 - (PANE_SHRINK * remaining)};
    `
  };
}

/** `animate:flip` duration for a reordering list row, silenced under reduced
 *  motion so survivors jump straight to their new slots. */
export function flipDuration(milliseconds = FLIP_DURATION) {
  return motionDuration(milliseconds);
}
