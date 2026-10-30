import { clearOverflowTooltip, showOverflowTooltip } from "@/lib/ui/tooltip/overflow-tooltip.svelte";

/** A tooltip text, or a thunk for one that must stay live (e.g. it names the
 *  currently ranked editor, which can change after the attachment mounts). */
type TooltipText = string | (() => string);

function resolve(text: TooltipText) {
  return typeof text === "function" ? text() : text;
}

// Unique anchor id per attached trigger, so the shared top-level bubble
// (OverflowTooltipLayer) can position itself back onto whichever one is hovered.
let nextAnchorId = 0;

const ANCHOR_NAME_PROPERTY = "anchor-name";
const NESTED_TOOLTIP_SELECTOR = "[data-tooltip]";
interface Bubble {
  text: string;
  fontFamily?: string;
}

const NO_BUBBLE: Bubble = { text: "" };

/** A descendant with its own `[data-tooltip]` (a chat row's pause mark) speaks
 *  for itself, so over it the trigger's bubble steps aside and the two never
 *  stack. theme.css's `:has()` rule can't do this for the shared bubble: it
 *  lives in the top layer, outside the trigger, so only a document-wide
 *  `:has()` could link them - the selector its PERF note bans. */
function isOverNestedTooltip({ elTrigger, target }: {
  elTrigger: HTMLElement;
  target: EventTarget | null | undefined;
}) {
  if (!(target instanceof Element)) {
    return false;
  }

  const elTooltipOwner = target.closest(NESTED_TOOLTIP_SELECTOR);
  return elTooltipOwner !== elTrigger && elTrigger.contains(elTooltipOwner);
}

/** Shared plumbing for both attachments: name the trigger as an anchor, then
 *  show/hide the single top-level bubble on hover/focus. `textToShow` returns the
 *  text for the current state (empty string = no bubble), so the truncation and
 *  always-on variants differ only in that one function.
 *
 *  The tooltip renders through the shared overflow-tooltip layer rather than a
 *  local `::after`, so it is never clipped, scroll-width-inflating, or painted
 *  behind a composited scroll container's contents - a `[data-tooltip]` pseudo is
 *  `position: fixed`, and inside a `.picker`-style scroller Chromium paints that
 *  escaped fixed box *behind* the scrolled rows, so a following row occludes it
 *  (see overflow-tooltip.svelte). */
function overflowTooltip(bubbleToShow: (elTrigger: HTMLElement) => Bubble) {
  return (elTrigger: HTMLElement) => {
    const anchorName = `--overflow-tooltip-${nextAnchorId++}`;
    elTrigger.style.setProperty(ANCHOR_NAME_PROPERTY, anchorName);

    function activate(e?: Event) {
      const { text, fontFamily } = isOverNestedTooltip({
        elTrigger,
        target: e?.target
      })
        ? NO_BUBBLE
        : bubbleToShow(elTrigger);
      if (!text) {
        deactivate();
        return;
      }

      showOverflowTooltip({
        anchorName,
        text,
        fontFamily
      });
      // Any scroll dismisses the bubble, as a native tooltip does: its row
      // may slide under a pinned section (which `anchors-visible` can't see,
      // since nothing clips it) while the bubble would still be riding it.
      addEventListener("scroll", deactivate, {
        capture: true,
        passive: true
      });
    }

    function deactivate() {
      removeEventListener("scroll", deactivate, { capture: true });
      clearOverflowTooltip(anchorName);
    }

    // Focus speaks only when the browser shows its ring - keyboard focus. A
    // menu opened by click still focuses its first row, and that row's bubble
    // would pop up under a cursor that is somewhere else.
    function activateOnVisibleFocus() {
      const isFocusVisible = elTrigger.matches(":focus-visible, :has(:focus-visible)");
      if (isFocusVisible) {
        activate();
      }
    }

    // `pointerover`, not `pointerenter`: it fires on every descendant crossing,
    // so the bubble steps aside over a nested tooltip and comes back after.
    elTrigger.addEventListener("pointerover", activate);
    elTrigger.addEventListener("pointerleave", deactivate);
    elTrigger.addEventListener("focusin", activateOnVisibleFocus);
    elTrigger.addEventListener("focusout", deactivate);
    return () => {
      deactivate();
      elTrigger.removeEventListener("pointerover", activate);
      elTrigger.removeEventListener("pointerleave", deactivate);
      elTrigger.removeEventListener("focusin", activateOnVisibleFocus);
      elTrigger.removeEventListener("focusout", deactivate);
      elTrigger.style.removeProperty(ANCHOR_NAME_PROPERTY);
    };
  };
}

/** Attachment that always surfaces `tooltip` on hover/focus, through the shared
 *  top-level bubble. Use this instead of a `data-tooltip` pseudo whenever the
 *  trigger sits inside a scroll container, a `[popover]`/`content-visibility`
 *  ancestor, or an element carrying a `view-transition-name` - each of those
 *  makes the ancestor a containing block (and stacking context) for the
 *  `position: fixed` pseudo, so the bubble is clipped, or trapped in the
 *  ancestor's paint order and occluded by whatever follows it. The launcher's
 *  recent list is one; the switcher's drag grips (their row is named for the
 *  reorder morph) are another, where the bubble showed *underneath* the next
 *  row's icon and text. */
export function tooltip(text: TooltipText) {
  return overflowTooltip(() => ({ text: resolve(text) }));
}

/** Attachment that surfaces `tooltip` only while the element's text is actually
 *  clipped (ellipsized) - an already-readable line never grows a redundant
 *  bubble. `restingTooltip`, when given, is what the unclipped state shows
 *  instead (an action hint like "Open in WebStorm" stays useful without the
 *  redundant path); omitted, the unclipped state has no tooltip.
 *  `measureSelector` points at the descendants that actually clip when they
 *  differ from the hover host (a button whose inner spans clip); any one of
 *  them clipped counts. Clipped means cut short either way: an ellipsized line
 *  or a line-clamped block.
 *
 *  The clip check runs on hover (`pointerover`) - the one moment that both guarantees a
 *  settled layout (a mount-time measure can read a mid-layout width and brand an
 *  untruncated line clipped forever) and precedes the bubble showing. With real
 *  layout in hand the DOM's own overflow answer is exact, and re-checking per
 *  hover tracks panel resizes for free. */
export function truncationTooltip({ tooltip: text, restingTooltip, measureSelector }: {
  tooltip: TooltipText;
  restingTooltip?: TooltipText;
  measureSelector?: string;
}) {
  return overflowTooltip(elTrigger => {
    const elsMeasured = measureSelector ? [...elTrigger.querySelectorAll(measureSelector)] : [elTrigger];
    const elClipped = elsMeasured.find(elMeasured =>
      elMeasured.scrollWidth > elMeasured.clientWidth || elMeasured.scrollHeight > elMeasured.clientHeight);
    if (elClipped) {
      // The bubble finishes the clipped line, so it reads in that line's face.
      return {
        text: resolve(text),
        fontFamily: getComputedStyle(elClipped).fontFamily
      };
    }

    // An empty resting text (e.g. "no editor ranked yet") means no bubble.
    return { text: restingTooltip === undefined ? "" : resolve(restingTooltip) };
  });
}
