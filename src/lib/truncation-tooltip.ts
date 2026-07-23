import type { Attachment } from "svelte/attachments";

// One reused 2D context: measuring text from font metrics is deterministic and
// sub-pixel exact, and avoids laying out the text the way reading `scrollWidth`
// would.
const measureContext = document.createElement("canvas").getContext("2d");

/** Which attribute carries the tooltip. `title` (the OS-drawn tooltip) for an
 *  element inside a popover — the top layer traps the CSS bubble's fixed pseudo
 *  and inflates the panel's scroll width; the `data-tooltip` CSS bubble for a
 *  normal surface, where it matches the app's own tooltip styling. */
export const TooltipAttribute = {
  Title: "title",
  Bubble: "data-tooltip"
} as const;
export type TooltipAttribute = (typeof TooltipAttribute)[keyof typeof TooltipAttribute];

/** Attachment that surfaces a tooltip only while the element's box is too
 *  narrow to show `text` in full — measured with the Canvas text-metrics API
 *  against the element's own font. `tooltip` is what the tooltip says (defaults
 *  to the measured `text` — pass a fuller form, e.g. the whole path behind a
 *  clipped parent-dir). `visible` is the host's shown state: the check re-runs
 *  when it flips true, which is both the only moment a row inside a
 *  (closed = `display:none`) popover has a real width to measure and the point
 *  a stale tooltip would matter — so no observer is needed. */
export function truncationTooltip({ text, tooltip = text, visible, attribute = TooltipAttribute.Title }: {
  text: string;
  tooltip?: string;
  visible: boolean;
  attribute?: TooltipAttribute;
}): Attachment {
  return element => {
    if (visible && isTextClipped({
      element,
      text
    })) {
      element.setAttribute(attribute, tooltip);
      return;
    }

    element.removeAttribute(attribute);
  };
}

/** True when `text`, rendered in `element`'s font, is wider than its content box. */
function isTextClipped({ element, text }: {
  element: Element;
  text: string;
}): boolean {
  if (!measureContext) {
    return false;
  }

  const style = getComputedStyle(element);
  measureContext.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
  const available =
    element.clientWidth - parseInt(style.paddingInlineStart, 10) - parseInt(style.paddingInlineEnd, 10);
  return measureContext.measureText(text).width > available;
}
