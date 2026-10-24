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
// A third, `arrowLists`, serves a popover holding several lists among other
// controls (the composer's project menu): data attributes name each list,
// the element a list is entered from, and who takes focus on open.

import { ToggleState } from "@/lib/ui/controls/toggle-state";
import { Key } from "@/lib/ui/keyboard/keys";

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

/** Make `iItem` the list's one Tab stop (a roving tabindex) without moving
 *  focus. */
function setTabStop({ elItems, iItem }: Move) {
  for (const [i, elItem] of elItems.entries()) {
    elItem.tabIndex = i === iItem ? FOCUSABLE_TAB_INDEX : SKIPPED_TAB_INDEX;
  }
}

/** Move the roving Tab stop to `iItem` and focus it. */
function roveTo(move: Move) {
  setTabStop(move);
  move.elItems[move.iItem]?.focus();
}

/** Attachment (`{@attach rovingMenu}`) for a popover menu; its items are its
 *  descendant `<button>`s. Returns the teardown Svelte runs when it detaches. */
export function rovingMenu(elMenu: HTMLElement) {
  function onToggle(e: ToggleEvent) {
    if (e.newState !== ToggleState.Open) {
      return;
    }

    roveTo({
      elItems: [...elMenu.querySelectorAll<HTMLElement>(MENU_ITEM_SELECTOR)],
      iItem: 0
    });
  }

  const unbindArrowKeys = bindArrowKeys({
    elMenu,
    itemSelector: MENU_ITEM_SELECTOR,
    focusItem: roveTo
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

const ARROW_LIST_ITEM_SELECTOR = "[data-arrow-list]";
const ARROW_LIST_ENTRY_SELECTOR = "[data-arrow-enters]";
const OPEN_FOCUS_SELECTOR = "[data-arrow-open-focus]";

/** Where a key moves within one named list, or null for keys it leaves
 *  alone. From an item (`from` its index) Up/Down walk and wrap and Home/End
 *  jump; from the list's entry point (a tab whose panel holds the list) only
 *  Up/Down step in - Down to the first item, Up to the last - and Home/End
 *  stay with the entry's own keyboard pattern. */
export function arrowListTarget({ key, isEntry, from, count }: {
  key: string;
  isEntry: boolean;
  from: number;
  count: number;
}) {
  const isVerticalArrow = key === Key.ArrowDown || key === Key.ArrowUp;
  if (isEntry && !isVerticalArrow) {
    return null;
  }

  return nextIndex({
    key,
    from: isEntry ? -1 : from,
    count
  });
}

/** Which of a popover's open-focus candidates (in DOM order) takes focus
 *  when it opens: the first one on screen, or -1 when none is. */
export function openFocusIndex(visibilities: readonly boolean[]) {
  return visibilities.indexOf(true);
}

/** Attachment (`{@attach arrowLists}`) for a popover holding several lists
 *  side by side, wired by data attributes alone:
 *   - `data-arrow-list="<name>"` on each item: Up/Down/Home/End move among
 *     the displayed items of that list, which keep one roving Tab stop, so
 *     a single Tab leaves the list;
 *   - `data-arrow-enters="<name>"` on an element outside the list (a tab
 *     whose panel holds it): Down steps into its first item, Up its last;
 *   - `data-arrow-open-focus` on the candidates for focus on open: the first
 *     one displayed takes it (the first project row, else the selected tab).
 *  Opening also resets every list's Tab stop to its first item. The rest of
 *  the Tab order is plain markup. */
export function arrowLists(elRoot: HTMLElement) {
  function listItems(listName: string) {
    return [...elRoot.querySelectorAll<HTMLElement>(ARROW_LIST_ITEM_SELECTOR)]
      .filter(elItem => elItem.dataset.arrowList === listName);
  }

  function displayedItems(listName: string) {
    return listItems(listName).filter(elItem => elItem.checkVisibility());
  }

  function onKeydown(e: KeyboardEvent) {
    if (!(e.target instanceof HTMLElement)) {
      return;
    }

    const isNestedMenuOpen = elRoot.querySelector(OPEN_NESTED_MENU_SELECTOR) !== null;
    if (isNestedMenuOpen) {
      return;
    }

    const elItem = e.target.closest<HTMLElement>(ARROW_LIST_ITEM_SELECTOR);
    const elEntry = elItem ? null : e.target.closest<HTMLElement>(ARROW_LIST_ENTRY_SELECTOR);
    const listName = elItem?.dataset.arrowList ?? elEntry?.dataset.arrowEnters;
    if (listName === undefined) {
      return;
    }

    const elItems = displayedItems(listName);
    const iItem = arrowListTarget({
      key: e.key,
      isEntry: elEntry !== null,
      from: elItem ? elItems.indexOf(elItem) : -1,
      count: elItems.length
    });
    if (iItem === null) {
      return;
    }

    e.preventDefault();
    roveTo({
      elItems,
      iItem
    });
  }

  function onToggle(e: ToggleEvent) {
    if (e.newState !== ToggleState.Open) {
      return;
    }

    const listNames = new Set(
      [...elRoot.querySelectorAll<HTMLElement>(ARROW_LIST_ITEM_SELECTOR)].map(elItem => elItem.dataset.arrowList ?? "")
    );
    for (const listName of listNames) {
      setTabStop({
        elItems: listItems(listName),
        iItem: 0
      });
    }

    const elCandidates = [...elRoot.querySelectorAll<HTMLElement>(OPEN_FOCUS_SELECTOR)];
    elCandidates[openFocusIndex(elCandidates.map(elCandidate => elCandidate.checkVisibility()))]?.focus();
  }

  elRoot.addEventListener("keydown", onKeydown);
  elRoot.addEventListener("toggle", onToggle);
  return () => {
    elRoot.removeEventListener("keydown", onKeydown);
    elRoot.removeEventListener("toggle", onToggle);
  };
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
