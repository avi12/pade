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

/** Attachment that surfaces `tooltip` only while the element's text is
 *  actually clipped (ellipsized), so an already-readable line never grows a
 *  redundant bubble. Pass a `tooltip` fuller than the visible text where the
 *  clipped form loses information (a feed card shows the parent dir, the
 *  tooltip the whole path).
 *
 *  The check runs on `pointerenter` — the one moment that both guarantees a
 *  settled layout (a mount-time measure can read a mid-layout width and brand
 *  an untruncated line clipped forever) and precedes either tooltip becoming
 *  visible (the CSS bubble and the OS title are hover-triggered, and hover
 *  styles resolve after the handler). With real layout in hand the DOM's own
 *  overflow answer is exact — no font re-measuring that misses letter-spacing —
 *  and re-checking per hover tracks panel resizes for free. */
export function truncationTooltip({ tooltip, attribute = TooltipAttribute.Title }: {
  tooltip: string;
  attribute?: TooltipAttribute;
}): Attachment {
  return element => {
    function measure() {
      if (element.scrollWidth > element.clientWidth) {
        element.setAttribute(attribute, tooltip);
        return;
      }

      element.removeAttribute(attribute);
    }

    element.addEventListener("pointerenter", measure);
    return () => element.removeEventListener("pointerenter", measure);
  };
}
