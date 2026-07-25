// One shared target for the truncation tooltip. Each clipped trigger reports
// itself here on hover so a SINGLE bubble renders once at the top level
// (OverflowTooltipLayer), anchored back to the trigger by name.
//
// Why a shared layer instead of a per-trigger `::after`: a truncation trigger
// often sits inside a containing block that clips or mis-sizes a fixed pseudo —
// a [popover] menu's top layer (the workspace switcher) traps the bubble and
// inflates the menu's scroll width, and a `content-visibility` row is a
// containing block that clips it. Rendering the bubble once, outside every such
// context, sidesteps both. Inspired by youtube-time-manager's OverflowTooltip.

interface ActiveOverflowTooltip {
  /** The trigger's `anchor-name`, so the layer positions itself back onto it. */
  anchorName: string;
  text: string;
}

export const overflowTooltipState = $state<{ active: ActiveOverflowTooltip | null }>({
  active: null
});

export function showOverflowTooltip(tooltip: ActiveOverflowTooltip): void {
  overflowTooltipState.active = tooltip;
}

/** Clear only if the given trigger still owns the bubble — a stale pointerleave
 *  from a trigger the pointer already moved off of never hides a newer one. */
export function clearOverflowTooltip(anchorName: string): void {
  if (overflowTooltipState.active?.anchorName === anchorName) {
    overflowTooltipState.active = null;
  }
}
