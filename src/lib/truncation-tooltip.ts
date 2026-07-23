import type { Attachment } from "svelte/attachments";

/** Which attribute carries the tooltip. `title` (the OS-drawn tooltip) for an
 *  element inside a popover — the top layer traps the CSS bubble's fixed pseudo
 *  and inflates the panel's scroll width; the `data-tooltip` CSS bubble for a
 *  normal surface, where it matches the app's own tooltip styling. */
export const TooltipAttribute = {
  Title: "title",
  Bubble: "data-tooltip"
} as const;
export type TooltipAttribute = (typeof TooltipAttribute)[keyof typeof TooltipAttribute];

/** A tooltip text, or a thunk for one that must stay live (e.g. it names the
 *  currently ranked editor, which can change after the attachment mounts). */
type TooltipText = string | (() => string);

function resolve(text: TooltipText): string {
  return typeof text === "function" ? text() : text;
}

/** Attachment that surfaces `tooltip` only while the element's text is
 *  actually clipped (ellipsized) — an already-readable line never grows a
 *  redundant bubble. `restingTooltip`, when given, is what the unclipped state
 *  shows instead (an action hint like "Open in WebStorm" stays useful without
 *  the redundant path); omitted, the unclipped state has no tooltip.
 *  `measureSelector` points at the descendant that actually ellipsizes when
 *  the clipping child differs from the hover/tooltip host (a button whose
 *  inner span clips).
 *
 *  The check runs on `pointerenter` — the one moment that both guarantees a
 *  settled layout (a mount-time measure can read a mid-layout width and brand
 *  an untruncated line clipped forever) and precedes either tooltip becoming
 *  visible (the CSS bubble and the OS title are hover-triggered, and hover
 *  styles resolve after the handler). With real layout in hand the DOM's own
 *  overflow answer is exact — no font re-measuring that misses letter-spacing —
 *  and re-checking per hover tracks panel resizes for free. */
export function truncationTooltip({ tooltip, restingTooltip, measureSelector, attribute = TooltipAttribute.Title }: {
  tooltip: TooltipText;
  restingTooltip?: TooltipText;
  measureSelector?: string;
  attribute?: TooltipAttribute;
}): Attachment {
  return element => {
    function measure() {
      const measured = (measureSelector ? element.querySelector(measureSelector) : null) ?? element;
      if (measured.scrollWidth > measured.clientWidth) {
        element.setAttribute(attribute, resolve(tooltip));
        return;
      }

      // An empty resting text (e.g. "no editor ranked yet") means no bubble.
      const resting = restingTooltip === undefined ? "" : resolve(restingTooltip);
      if (resting) {
        element.setAttribute(attribute, resting);
        return;
      }

      element.removeAttribute(attribute);
    }

    element.addEventListener("pointerenter", measure);
    return () => element.removeEventListener("pointerenter", measure);
  };
}
