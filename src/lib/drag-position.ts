import type { DragPosition } from "@/lib/types";

/** Anything with a layout box — a DOM element, or a stand-in in a test. */
interface Measurable {
  getBoundingClientRect: () => Pick<DOMRect, "left" | "right" | "top" | "bottom" | "width" | "height">;
}

/** Does a native OS drag (Tauri's `dragDrop` events) sit over `element`? Tauri
 *  reports the cursor in physical pixels while layout rects are CSS pixels, so
 *  the position is scaled by the device pixel ratio first. An element that is
 *  not rendered (a hidden terminal) has an empty rect and never matches. */
export function isDragOverElement({ element, position }: {
  element: Measurable;
  position: DragPosition;
}): boolean {
  const rectangle = element.getBoundingClientRect();
  const x = position.x / devicePixelRatio;
  const y = position.y / devicePixelRatio;
  const hasArea = rectangle.width > 0 && rectangle.height > 0;
  return hasArea && x >= rectangle.left && x <= rectangle.right && y >= rectangle.top && y <= rectangle.bottom;
}
