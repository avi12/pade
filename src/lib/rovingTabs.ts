// ARIA tabs keyboard pattern for pill tablists: the arrow keys move focus AND
// selection to the neighboring tab (wrapping; Home/End jump to the ends),
// while Tab itself leaves the list — the markup keeps a roving tabindex
// (0 on the selected tab, -1 on the rest), so pressing Tab navigates into the
// active panel's inputs instead of walking every pill.

/** Svelte action for a `role="tablist"` element whose tabs select on click. */
export function rovingTablist(node: HTMLElement) {
  function onKeydown(e: KeyboardEvent) {
    const origin = e.target instanceof HTMLElement
      ? e.target.closest<HTMLElement>("[role=tab]")
      : null;
    if (!origin) {
      return;
    }

    const tabs = [...node.querySelectorAll<HTMLElement>("[role=tab]")];
    const from = tabs.indexOf(origin);
    const to = nextIndex({
      key: e.key,
      from,
      count: tabs.length
    });
    if (to === null) {
      return;
    }

    e.preventDefault();
    tabs[to].focus();
    tabs[to].click();
  }

  node.addEventListener("keydown", onKeydown);
  return {
    destroy() {
      node.removeEventListener("keydown", onKeydown);
    }
  };
}

/** The tab index a key moves to, or null for keys the pattern doesn't handle. */
function nextIndex({ key, from, count }: {
  key: string;
  from: number;
  count: number;
}): number | null {
  switch (key) {
    case "ArrowRight":
      return (from + 1) % count;
    case "ArrowLeft":
      return (from - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}
