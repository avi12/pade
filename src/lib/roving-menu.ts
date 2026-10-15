// Keyboard focus movement for a vertical menu: Arrow Up/Down walk its items
// (wrapping; Home/End jump to the ends), from whichever item currently holds
// focus. Section labels and separators aren't items, so they're skipped for
// free. Sibling of `rovingTablist` (that one is horizontal and selects on move;
// a menu is vertical and only moves focus - the click/Enter activates).
//
// Two attachments share that one keydown wiring, differing only in what a move
// does besides focusing:
//   • `rovingMenu` - a popover menu of plain `<button>`s. Its items carry a
//     ROVING tabindex (0 on the focused item, -1 on the rest) so a single Tab
//     LEAVES the menu instead of walking every entry, and focus lands on the
//     first item when the popover opens.
//   • `arrowFocus` - a menu whose items must stay in the natural Tab order
//     because they sit among other controls (the project switcher: its rows are
//     interleaved with per-row kebabs, and its filter field is one of the stops).
//     Nothing but focus moves.

import { Key } from "@/lib/keys";
import { ToggleState } from "@/lib/toggle-state";

const MENU_ITEM_SELECTOR = "button";
const OPEN_NESTED_MENU_SELECTOR = ":scope [popover]:popover-open";
const FOCUSABLE_TAB_INDEX = 0;
const SKIPPED_TAB_INDEX = -1;

/** Where a move lands: the items as found in the DOM, and the index to focus. */
type Move = {
  elItems: HTMLElement[];
  iItem: number;
};

/** The shared keydown wiring. `focusItem` performs the move, so a caller can
 *  carry extra state (the roving tabindex) alongside the focus call. */
function bindArrowKeys({ elMenu, itemSelector, focusItem }: {
  elMenu: HTMLElement;
  itemSelector: string;
  focusItem: (move: Move) => void;
}) {
  function onKeydown(e: KeyboardEvent) {
    if (!(e.target instanceof HTMLElement)) {
      return;
    }

    // While a nested menu is up (a switcher row's kebab popover) it owns the
    // arrow keys - wherever focus sits, this list must not move underneath it.
    const isNestedMenuOpen = elMenu.querySelector(OPEN_NESTED_MENU_SELECTOR) !== null;
    if (isNestedMenuOpen) {
      return;
    }

    // Home/End belong to a focused text field's caret; only the arrows pull
    // focus out of one.
    const isTextEntry = e.target instanceof HTMLInputElement;
    const isArrowKey = e.key === Key.ArrowDown || e.key === Key.ArrowUp;
    if (isTextEntry && !isArrowKey) {
      return;
    }

    const elItems = [...elMenu.querySelectorAll<HTMLElement>(itemSelector)];
    const elOrigin = e.target.closest<HTMLElement>(itemSelector);
    const iItem = nextIndex({
      key: e.key,
      from: elOrigin ? elItems.indexOf(elOrigin) : -1,
      count: elItems.length
    });
    if (iItem === null) {
      return;
    }

    e.preventDefault();
    focusItem({
      elItems,
      iItem
    });
  }

  elMenu.addEventListener("keydown", onKeydown);
  return () => elMenu.removeEventListener("keydown", onKeydown);
}

/** Attachment (`{@attach rovingMenu}`) for a popover menu; its items are its
 *  descendant `<button>`s. Returns the teardown Svelte runs when it detaches. */
export function rovingMenu(elMenu: HTMLElement) {
  function focusItem({ elItems, iItem }: Move) {
    for (const [i, elItem] of elItems.entries()) {
      elItem.tabIndex = i === iItem ? FOCUSABLE_TAB_INDEX : SKIPPED_TAB_INDEX;
    }

    elItems[iItem]?.focus();
  }

  function onToggle(e: ToggleEvent) {
    if (e.newState !== ToggleState.Open) {
      return;
    }

    focusItem({
      elItems: [...elMenu.querySelectorAll<HTMLElement>(MENU_ITEM_SELECTOR)],
      iItem: 0
    });
  }

  const unbindArrowKeys = bindArrowKeys({
    elMenu,
    itemSelector: MENU_ITEM_SELECTOR,
    focusItem
  });
  elMenu.addEventListener("toggle", onToggle);
  return () => {
    unbindArrowKeys();
    elMenu.removeEventListener("toggle", onToggle);
  };
}

/** Attachment factory (`{@attach arrowFocus("[data-arrow-stop]")}`) for a menu
 *  whose items stay in the natural Tab order: Up/Down move focus between the
 *  `itemSelector` matches and nothing else changes. */
export function arrowFocus(itemSelector: string) {
  return (elMenu: HTMLElement) => bindArrowKeys({
    elMenu,
    itemSelector,
    focusItem: ({ elItems, iItem }) => elItems[iItem]?.focus()
  });
}

/** The item index a key moves focus to, or null for keys the menu doesn't handle.
 *  `from` is -1 when focus isn't yet on an item (Down → first, Up → last). */
export function nextIndex({ key, from, count }: {
  key: string;
  from: number;
  count: number;
}) {
  if (count === 0) {
    return null;
  }

  const isUnfocused = from < 0;
  switch (key) {
    case Key.ArrowDown:
      return isUnfocused ? 0 : (from + 1) % count;
    case Key.ArrowUp:
      return isUnfocused ? count - 1 : (from - 1 + count) % count;
    case Key.Home:
      return 0;
    case Key.End:
      return count - 1;
    default:
      return null;
  }
}
