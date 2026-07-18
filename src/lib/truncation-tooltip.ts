import type { Attachment } from "svelte/attachments";

// One reused 2D context: measuring text from font metrics is deterministic and
// sub-pixel exact, and avoids laying out the text the way reading `scrollWidth`
// would.
const measureContext = document.createElement("canvas").getContext("2d");

/** Attachment that surfaces `text` as a native `title` tooltip only while the
 *  element's box is too narrow to show it in full — measured with the Canvas
 *  text-metrics API against the element's own font. `visible` is the switcher's
 *  open state: the check re-runs when it flips true, which is both the only moment
 *  a row inside the (closed = `display:none`) popover has a real width to measure
 *  and the point a stale title would matter — so no observer is needed. `title`
 *  (not the CSS `[data-tooltip]` bubble) because the trigger sits inside a popover,
 *  whose top layer traps a `position: fixed` pseudo and inflates the panel's scroll
 *  width; the OS-drawn `title` floats above all UI and never affects layout. */
export function truncationTooltip({ text, visible }: { text: string; visible: boolean }): Attachment {
  return element => {
    if (visible && isTextClipped({ element, text })) {
      element.setAttribute("title", text);
      return;
    }

    element.removeAttribute("title");
  };
}

/** True when `text`, rendered in `element`'s font, is wider than its content box. */
function isTextClipped({ element, text }: { element: Element; text: string }): boolean {
  if (!measureContext) {
    return false;
  }

  const style = getComputedStyle(element);
  measureContext.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
  const available =
    element.clientWidth - parseInt(style.paddingInlineStart, 10) - parseInt(style.paddingInlineEnd, 10);
  return measureContext.measureText(text).width > available;
}
