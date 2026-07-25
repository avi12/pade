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

/** Attachment that surfaces `tooltip` only while the element's text is actually
 *  clipped (ellipsized) — an already-readable line never grows a redundant
 *  bubble. `restingTooltip`, when given, is what the unclipped state shows
 *  instead (an action hint like "Open in WebStorm" stays useful without the
 *  redundant path); omitted, the unclipped state has no tooltip.
 *  `measureSelector` points at the descendant that actually ellipsizes when the
 *  clipping child differs from the hover host (a button whose inner span clips).
 *
 *  The tooltip renders through the shared overflow-tooltip layer rather than a
 *  local `::after`, so it is never clipped or scroll-width-inflating inside a
 *  [popover] menu or a `content-visibility` row (see overflow-tooltip.svelte).
 *
 *  The clip check runs on `pointerenter` — the one moment that both guarantees a
 *  settled layout (a mount-time measure can read a mid-layout width and brand an
 *  untruncated line clipped forever) and precedes the bubble showing. With real
 *  layout in hand the DOM's own overflow answer is exact, and re-checking per
 *  hover tracks panel resizes for free. */
export function truncationTooltip({ tooltip, restingTooltip, measureSelector }: {
  tooltip: TooltipText;
  restingTooltip?: TooltipText;
  measureSelector?: string;
}): Attachment<HTMLElement> {
  return element => {
    const anchorName = `--overflow-tooltip-${nextAnchorId++}`;
    element.style.setProperty("anchor-name", anchorName);

    function tooltipToShow(): string {
      const measured = (measureSelector ? element.querySelector(measureSelector) : null) ?? element;
      if (measured.scrollWidth > measured.clientWidth) {
        return resolve(tooltip);
      }

      // An empty resting text (e.g. "no editor ranked yet") means no bubble.
      return restingTooltip === undefined ? "" : resolve(restingTooltip);
    }

    function activate() {
      const text = tooltipToShow();
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
