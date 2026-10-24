import { clearOverflowTooltip, showOverflowTooltip } from "@/lib/ui/overflow-tooltip.svelte";

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
function overflowTooltip(textToShow: (elTrigger: HTMLElement) => string) {
  return (elTrigger: HTMLElement) => {
    const anchorName = `--overflow-tooltip-${nextAnchorId++}`;
    elTrigger.style.setProperty(ANCHOR_NAME_PROPERTY, anchorName);

    function activate() {
      const text = textToShow(elTrigger);
      if (!text) {
        clearOverflowTooltip(anchorName);
        return;
      }

      showOverflowTooltip({
        anchorName,
        text
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

    elTrigger.addEventListener("pointerenter", activate);
    elTrigger.addEventListener("pointerleave", deactivate);
    elTrigger.addEventListener("focusin", activate);
    elTrigger.addEventListener("focusout", deactivate);
    return () => {
      deactivate();
      elTrigger.removeEventListener("pointerenter", activate);
      elTrigger.removeEventListener("pointerleave", deactivate);
      elTrigger.removeEventListener("focusin", activate);
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
  return overflowTooltip(() => resolve(text));
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
 *  The clip check runs on `pointerenter` - the one moment that both guarantees a
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
    const isClipped = elsMeasured.some(elMeasured =>
      elMeasured.scrollWidth > elMeasured.clientWidth || elMeasured.scrollHeight > elMeasured.clientHeight);
    if (isClipped) {
      return resolve(text);
    }

    // An empty resting text (e.g. "no editor ranked yet") means no bubble.
    return restingTooltip === undefined ? "" : resolve(restingTooltip);
  });
}
