// Greedy three-tier packing for the session-tab strip (pure, DOM-free): tabs
// that fit render as full pills, the next few collapse to status dots, and the
// remainder lives behind a "+N" overflow popover. Pill widths are supplied by
// the caller (measured from an off-layout mirror row), so packing never feeds
// back into the layout it packs against.

/** Pixels between tab items — must mirror the tab strip's CSS flex gap. */
export const TAB_GAP = 6;

/** A collapsed status-dot button plus its gap. */
export const DOT_SLOT = 22 + TAB_GAP;

/** The "+N" overflow button plus its gap. */
export const MORE_SLOT = 34 + TAB_GAP;

/** The trailing add-agent button plus its gap — always reserved at the end. */
export const ADD_SLOT = 30 + TAB_GAP;

/** How the strip splits: full pills, collapsed status dots, "+N" overflow. */
export interface TabPack {
  visible: string[];
  dots: string[];
  more: string[];
}

/** Pack `ids` (in strip order) into the three tiers against `stripWidth`.
 *  `widthOf` supplies each tab's measured full-pill width; a `stripWidth` of 0
 *  (not yet measured) keeps every tab a full pill. */
export function packTabs({ ids, widthOf, stripWidth }: {
  ids: string[];
  widthOf: (id: string) => number;
  stripWidth: number;
}): TabPack {
  const total = ids.reduce((sum, id, index) => sum + widthOf(id) + (index ? TAB_GAP : 0), 0);
  // Everything fits (or we haven't measured yet) — all as full pills.
  if (stripWidth === 0 || total <= stripWidth) {
    return {
      visible: [...ids],
      dots: [],
      more: []
    };
  }

  // We know we'll overflow, so reserve room for the "+N" button.
  const budget = stripWidth - MORE_SLOT;
  const visible: string[] = [];
  let used = 0;
  for (const id of ids) {
    const next = used + widthOf(id) + (visible.length ? TAB_GAP : 0);
    if (next > budget) {
      break;
    }

    visible.push(id);
    used = next;
  }

  // Always keep at least one pill so the bar is never only a "+N".
  if (visible.length === 0 && ids.length > 0) {
    visible.push(ids[0]);
    used = widthOf(ids[0]);
  }

  const rest = ids.slice(visible.length);
  const dots: string[] = [];
  let dotRoom = budget - used;
  for (const id of rest) {
    if (dotRoom < DOT_SLOT) {
      break;
    }

    dots.push(id);
    dotRoom -= DOT_SLOT;
  }

  return {
    visible,
    dots,
    more: rest.slice(dots.length)
  };
}
