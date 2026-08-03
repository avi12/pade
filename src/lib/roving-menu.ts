// Keyboard navigation for a popover menu whose items are plain `<button>`s.
// Arrow Up/Down move focus between the items (wrapping; Home/End jump to the
// ends), and the items carry a ROVING tabindex — 0 on the focused item, -1 on
// the rest — so a single Tab LEAVES the menu instead of walking every entry.
// Focus lands on the first item when the popover opens. Section labels and
// separators aren't buttons, so they're skipped for free. Sibling of
// `rovingTablist` (that one is horizontal and selects on move; a menu is
// vertical and only moves focus — the click/Enter activates).

/** Attachment (`{@attach rovingMenu}`) for a popover menu; its items are its
 *  descendant `<button>`s. Returns the teardown Svelte runs when it detaches. */
export function rovingMenu(node: HTMLElement): () => void {
  function items(): HTMLElement[] {
    return [...node.querySelectorAll<HTMLButtonElement>("button")];
  }

  function focusItem(index: number): void {
    const all = items();
    for (const [position, item] of all.entries()) {
      item.tabIndex = position === index ? 0 : -1;
    }

    all[index]?.focus();
  }

  function onKeydown(event: KeyboardEvent): void {
    const all = items();
    const origin = event.target instanceof HTMLElement
      ? event.target.closest<HTMLElement>("button")
      : null;
    const to = nextIndex({
      key: event.key,
      from: origin ? all.indexOf(origin) : -1,
      count: all.length
    });
    if (to === null) {
      return;
    }

    event.preventDefault();
    focusItem(to);
  }

  function onToggle(event: ToggleEvent): void {
    if (event.newState === "open") {
      focusItem(0);
    }
  }

  node.addEventListener("keydown", onKeydown);
  node.addEventListener("toggle", onToggle);
  return () => {
    node.removeEventListener("keydown", onKeydown);
    node.removeEventListener("toggle", onToggle);
  };
}

/** The item index a key moves focus to, or null for keys the menu doesn't handle.
 *  `from` is -1 when focus isn't yet on an item (Down → first, Up → last). */
export function nextIndex({ key, from, count }: {
  key: string;
  from: number;
  count: number;
}): number | null {
  if (count === 0) {
    return null;
  }

  switch (key) {
    case "ArrowDown":
      return from < 0 ? 0 : (from + 1) % count;
    case "ArrowUp":
      return from < 0 ? count - 1 : (from - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}
