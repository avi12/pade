import { colorAlpha, OPAQUE_ALPHA } from "@/lib/code/colors";
import type { Sibling } from "@/lib/ui/drag/reorder/reorder-geometry";

/** How the lifted item springs into its slot (or home) on release: a gentle
 *  overshoot - the design's drop settle, distinct from the emphasized `--ease` the
 *  siblings slide with. No token exists for this one-off curve, so it lives here. */
const SPRING_EASE = "cubic-bezier(0.34, 1.35, 0.5, 1)";
const SPRING_SECONDS = 0.3;
export const SPRING_MILLISECONDS = 300;
const SIBLING_SECONDS = 0.24;
/** The lifted item's elevation: a big soft shadow that fades to nothing as it
 *  settles. The colour comes from the theme token so it reads in light and dark
 *  (the mockup, dark-only, hard-codes `hsl(214 40% 4% / .55)`, which the dark
 *  `--shadow-color` matches). */
const LIFT_SHADOW = "0 18px 44px var(--shadow-color)";
const LIFT_SHADOW_FADED = "0 18px 44px transparent";
/** A lifted item floats over its siblings, so it has to be opaque. Rows that
 *  paint no surface of their own (the app menu's window and pinned-project rows
 *  are transparent until hovered) would otherwise let the sibling sliding
 *  underneath read straight through them - two rows' text superimposed. One tone
 *  above the `--surface-2` menu they sit in; a tab pill or runner card that
 *  already paints its own surface keeps it. */
const LIFT_BACKGROUND = "var(--surface-3)";
const LIFTED_Z_INDEX = "130";
const SHIELD_Z_INDEX = "120";
const LIFTED_RADIUS = "0.75rem";

const GRABBING_CURSOR = "grabbing";

/** Whether the item already draws a surface the lift can float on, so the engine
 *  only paints `LIFT_BACKGROUND` under the rows that draw none. */
function paintsOwnSurface(elItem: HTMLElement) {
  const { backgroundColor, backgroundImage } = getComputedStyle(elItem);
  return backgroundImage !== "none" || colorAlpha(backgroundColor) >= OPAQUE_ALPHA;
}

/** Lift the dragged sibling above the rest and ready the others to slide aside. */
export function liftSiblings({ siblings, iFrom, isReducedMotion }: {
  siblings: Sibling[];
  iFrom: number;
  isReducedMotion: boolean;
}) {
  for (const [i, { elItem }] of siblings.entries()) {
    const { style } = elItem;
    style.position = "relative";

    if (i !== iFrom) {
      style.transition = isReducedMotion ? "none" : `translate ${SIBLING_SECONDS}s var(--ease)`;
      continue;
    }

    style.transition = "none";
    style.zIndex = LIFTED_Z_INDEX;
    style.boxShadow = LIFT_SHADOW;
    style.pointerEvents = "none";
    style.cursor = GRABBING_CURSOR;
    style.borderRadius = LIFTED_RADIUS;

    if (!paintsOwnSurface(elItem)) {
      style.background = LIFT_BACKGROUND;
    }
  }
}

/** A transparent full-window sheet under the lifted item: it shows the grabbing
 *  cursor over every surface and swallows stray hover/click. */
export function raiseShield() {
  const elShield = document.createElement("div");
  elShield.dataset.dragShield = "";
  elShield.style.position = "fixed";
  elShield.style.inset = "0";
  elShield.style.zIndex = SHIELD_Z_INDEX;
  elShield.style.cursor = GRABBING_CURSOR;
  elShield.style.background = "transparent";
  document.body.appendChild(elShield);
  return elShield;
}

export interface OverflowSave {
  elAncestor: HTMLElement;
  overflow: string;
}

// Neutralize the scroll ancestors' clipping for the gesture, returning each
// ancestor's own value to restore on drop. An escape-drag (`outsideSelector` - a tab toward
// the panes, a pane toward the strip) forces them `visible` so the lifted item
// isn't cut off leaving its container. An in-place sort forces them `hidden`
// instead: the translated item still extends the container's scrollable area,
// and an `overflow: auto` ancestor (the runner dock's grid) would otherwise
// pop scrollbars for the whole drag and settle.
export function unclipAncestors({ elParent, isEscapeDrag }: {
  elParent: HTMLElement;
  isEscapeDrag: boolean;
}) {
  const saves: OverflowSave[] = [];
  const neutral = isEscapeDrag ? "visible" : "hidden";
  let elAncestor: HTMLElement | null = elParent;
  while (elAncestor && elAncestor !== document.body) {
    const { overflow } = getComputedStyle(elAncestor);
    const isScrolling = overflow !== "" && overflow !== "visible" && overflow !== "hidden";
    const isClippingEscape = isEscapeDrag && overflow === "hidden";
    if (isScrolling || isClippingEscape) {
      saves.push({
        elAncestor,
        overflow: elAncestor.style.overflow
      });
      elAncestor.style.overflow = isScrolling ? neutral : "visible";
    }

    elAncestor = elAncestor.parentElement;
  }

  return saves;
}

export function reclipAncestors(saves: OverflowSave[]) {
  for (const { elAncestor, overflow } of saves) {
    elAncestor.style.overflow = overflow;
  }

  saves.length = 0;
}

export function clearItemStyles(siblings: Sibling[]) {
  for (const { elItem } of siblings) {
    const { style } = elItem;
    // Clear `transition` first so removing `translate` is instant, never animated.
    style.transition = "";
    style.translate = "";
    style.zIndex = "";
    style.position = "";
    style.background = "";
    style.boxShadow = "";
    style.pointerEvents = "";
    style.cursor = "";
    style.borderRadius = "";
  }
}

/** Spring the lifted item toward `target`, fading its shadow out as it settles. */
export function springToTarget({ elDragged, target }: {
  elDragged: HTMLElement;
  target: string;
}) {
  elDragged.style.transition = `translate ${SPRING_SECONDS}s ${SPRING_EASE}, box-shadow ${SPRING_SECONDS}s`;
  elDragged.style.boxShadow = LIFT_SHADOW_FADED;
  requestAnimationFrame(() => {
    elDragged.style.translate = target;
  });
}
