import { clearOverflowTooltip, showOverflowTooltip } from "@/lib/overflow-tooltip.svelte";
import type { Attachment } from "svelte/attachments";

/** A tooltip text, or a thunk for one that must stay live (e.g. it names the
 *  currently ranked editor, which can change after the attachment mounts). */
type TooltipText = string | (() => string);

function resolve(text: TooltipText): string {
  return typeof text === "function" ? text() : text;
}

// Unique anchor id per attached trigger, so the shared top-level bubble
// (OverflowTooltipLayer) can position itself back onto whichever one is hovered.
let nextAnchorId = 0;

/** Shared plumbing for both attachments: name the trigger as an anchor, then
 *  show/hide the single top-level bubble on hover/focus. `textToShow` returns the
 *  text for the current state (empty string = no bubble), so the truncation and
 *  always-on variants differ only in that one function.
 *
 *  The tooltip renders through the shared overflow-tooltip layer rather than a
 *  local `::after`, so it is never clipped, scroll-width-inflating, or painted
 *  behind a composited scroll container's contents — a `[data-tooltip]` pseudo is
 *  `position: fixed`, and inside a `.picker`-style scroller Chromium paints that
 *  escaped fixed box *behind* the scrolled rows, so a following row occludes it
 *  (see overflow-tooltip.svelte). */
function overflowTooltip(textToShow: (element: HTMLElement) => string): Attachment<HTMLElement> {
  return element => {
    const anchorName = `--overflow-tooltip-${nextAnchorId++}`;
    element.style.setProperty("anchor-name", anchorName);

    function activate() {
      const text = textToShow(element);
      if (text) {
        showOverflowTooltip({
          anchorName,
          text
        });
      } else {
        clearOverflowTooltip(anchorName);
      }
    }

    function deactivate() {
      clearOverflowTooltip(anchorName);
    }

    element.addEventListener("pointerenter", activate);
    element.addEventListener("pointerleave", deactivate);
    element.addEventListener("focusin", activate);
    element.addEventListener("focusout", deactivate);
    return () => {
      deactivate();
      element.removeEventListener("pointerenter", activate);
      element.removeEventListener("pointerleave", deactivate);
      element.removeEventListener("focusin", activate);
      element.removeEventListener("focusout", deactivate);
      element.style.removeProperty("anchor-name");
    };
  };
}

/** Attachment that always surfaces `tooltip` on hover/focus, through the shared
 *  top-level bubble. Use this instead of a `data-tooltip` pseudo whenever the
 *  trigger sits inside a scroll container or a `[popover]`/`content-visibility`
 *  ancestor, where the `position: fixed` pseudo would be clipped or painted
 *  behind the scrolled content (the launcher's recent list is one). */
export function tooltip(text: TooltipText): Attachment<HTMLElement> {
  return overflowTooltip(() => resolve(text));
}

/** Attachment that surfaces `tooltip` only while the element's text is actually
 *  clipped (ellipsized) — an already-readable line never grows a redundant
 *  bubble. `restingTooltip`, when given, is what the unclipped state shows
 *  instead (an action hint like "Open in WebStorm" stays useful without the
 *  redundant path); omitted, the unclipped state has no tooltip.
 *  `measureSelector` points at the descendant that actually ellipsizes when the
 *  clipping child differs from the hover host (a button whose inner span clips).
 *
 *  The clip check runs on `pointerenter` — the one moment that both guarantees a
 *  settled layout (a mount-time measure can read a mid-layout width and brand an
 *  untruncated line clipped forever) and precedes the bubble showing. With real
 *  layout in hand the DOM's own overflow answer is exact, and re-checking per
 *  hover tracks panel resizes for free. */
export function truncationTooltip({ tooltip: text, restingTooltip, measureSelector }: {
  tooltip: TooltipText;
  restingTooltip?: TooltipText;
  measureSelector?: string;
}): Attachment<HTMLElement> {
  return overflowTooltip(element => {
    const measured = (measureSelector ? element.querySelector(measureSelector) : null) ?? element;
    if (measured.scrollWidth > measured.clientWidth) {
      return resolve(text);
    }

    // An empty resting text (e.g. "no editor ranked yet") means no bubble.
    return restingTooltip === undefined ? "" : resolve(restingTooltip);
  });
}
