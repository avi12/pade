// ARIA tabs keyboard pattern for pill tablists: the arrow keys move focus AND
// selection to the neighboring tab (wrapping; Home/End jump to the ends),
// while Tab itself leaves the list - the markup keeps a roving tabindex
// (0 on the selected tab, -1 on the rest), so pressing Tab navigates into the
// active panel's inputs instead of walking every pill.

import { Key } from "@/lib/keys";

const TAB_SELECTOR = "[role=tab]";

/** Svelte action for a `role="tablist"` element whose tabs select on click. */
export function rovingTablist(elTablist: HTMLElement) {
  function onKeydown(e: KeyboardEvent) {
    const elOrigin = e.target instanceof HTMLElement
      ? e.target.closest<HTMLElement>(TAB_SELECTOR)
      : null;
    if (!elOrigin) {
      return;
    }

    const elTabs = [...elTablist.querySelectorAll<HTMLElement>(TAB_SELECTOR)];
    const iTarget = nextIndex({
      key: e.key,
      from: elTabs.indexOf(elOrigin),
      count: elTabs.length
    });
    if (iTarget === null) {
      return;
    }

    e.preventDefault();
    elTabs[iTarget].focus();
    elTabs[iTarget].click();
  }

  elTablist.addEventListener("keydown", onKeydown);
  return {
    destroy() {
      elTablist.removeEventListener("keydown", onKeydown);
    }
  };
}

/** The tab index a key moves to, or null for keys the pattern doesn't handle. */
function nextIndex({ key, from, count }: {
  key: string;
  from: number;
  count: number;
}) {
  switch (key) {
    case Key.ArrowRight:
      return (from + 1) % count;
    case Key.ArrowLeft:
      return (from - 1 + count) % count;
    case Key.Home:
      return 0;
    case Key.End:
      return count - 1;
    default:
      return null;
  }
}
