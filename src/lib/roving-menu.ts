// Keyboard focus movement for a vertical menu: Arrow Up/Down walk its items
// (wrapping; Home/End jump to the ends), from whichever item currently holds
// focus. Section labels and separators aren't items, so they're skipped for
// free. Sibling of `rovingTablist` (that one is horizontal and selects on move;
// a menu is vertical and only moves focus — the click/Enter activates).
//
// Two attachments share that one keydown wiring, differing only in what a move
// does besides focusing:
//   • `rovingMenu` — a popover menu of plain `<button>`s. Its items carry a
//     ROVING tabindex (0 on the focused item, -1 on the rest) so a single Tab
//     LEAVES the menu instead of walking every entry, and focus lands on the
//     first item when the popover opens.
//   • `arrowFocus` — a menu whose items must stay in the natural Tab order
//     because they sit among other controls (the project switcher: its rows are
//     interleaved with per-row kebabs, and its filter field is one of the stops).
//     Nothing but focus moves.
import type { Attachment } from "svelte/attachments";

/** Where a move lands: the items as found in the DOM, and the index to focus. */
type Move = {
  items: HTMLElement[];
  index: number;
};

/** The shared keydown wiring. `focusItem` performs the move, so a caller can
 *  carry extra state (the roving tabindex) alongside the focus call. */
function bindArrowKeys({ node, itemSelector, focusItem }: {
  node: HTMLElement;
  itemSelector: string;
  focusItem: (move: Move) => void;
}): () => void {
  function onKeydown(event: KeyboardEvent): void {
    if (!(event.target instanceof HTMLElement)) {
      return;
    }

    // While a nested menu is up (a switcher row's kebab popover) it owns the
    // arrow keys — wherever focus sits, this list must not move underneath it.
    const nestedMenuOpen = node.querySelector(":scope [popover]:popover-open") !== null;
    if (nestedMenuOpen) {
      return;
    }

    // Home/End belong to a focused text field's caret; only the arrows pull
    // focus out of one.
    const isTextEntry = event.target instanceof HTMLInputElement;
    const isArrowKey = event.key === "ArrowDown" || event.key === "ArrowUp";
    if (isTextEntry && !isArrowKey) {
      return;
    }

    const items = [...node.querySelectorAll<HTMLElement>(itemSelector)];
    const origin = event.target.closest<HTMLElement>(itemSelector);
    const index = nextIndex({
      key: event.key,
      from: origin ? items.indexOf(origin) : -1,
      count: items.length
    });
    if (index === null) {
      return;
    }

    event.preventDefault();
    focusItem({
      items,
      index
    });
  }

  node.addEventListener("keydown", onKeydown);
  return () => node.removeEventListener("keydown", onKeydown);
}

/** Attachment (`{@attach rovingMenu}`) for a popover menu; its items are its
 *  descendant `<button>`s. Returns the teardown Svelte runs when it detaches. */
export function rovingMenu(node: HTMLElement): () => void {
  function focusItem({ items, index }: Move): void {
    for (const [position, item] of items.entries()) {
      item.tabIndex = position === index ? 0 : -1;
    }

    items[index]?.focus();
  }

  function onToggle(event: ToggleEvent): void {
    if (event.newState === "open") {
      focusItem({
        items: [...node.querySelectorAll<HTMLElement>("button")],
        index: 0
      });
    }
  }

  const unbindArrowKeys = bindArrowKeys({
    node,
    itemSelector: "button",
    focusItem
  });
  node.addEventListener("toggle", onToggle);
  return () => {
    unbindArrowKeys();
    node.removeEventListener("toggle", onToggle);
  };
}

/** Attachment factory (`{@attach arrowFocus("[data-arrow-stop]")}`) for a menu
 *  whose items stay in the natural Tab order: Up/Down move focus between the
 *  `itemSelector` matches and nothing else changes. */
export function arrowFocus(itemSelector: string): Attachment<HTMLElement> {
  return node => bindArrowKeys({
    node,
    itemSelector,
    focusItem: ({ items, index }) => items[index]?.focus()
  });
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
