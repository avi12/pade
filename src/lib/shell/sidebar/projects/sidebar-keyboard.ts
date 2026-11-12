// Arrow-key movement through the left rail. Tab stops only on project headers
// (the markup takes every other control out of the tab order); the arrows walk
// the rest. The rail reads as lines - a project header, one chat, or the
// inactive fold's summary - each a
// main control followed by its own small buttons. Up and down move between
// lines and land on a line's main control; left and right move along a line.

export const SidebarKey = {
  Up: "ArrowUp",
  Down: "ArrowDown",
  Left: "ArrowLeft",
  Right: "ArrowRight",
  Home: "Home",
  End: "End"
} as const;

const SIDEBAR_KEYS = new Set<string>(Object.values(SidebarKey));

/** Where focus sits: which line, and which control along it. */
export interface SidebarPosition {
  line: number;
  item: number;
}

const LINE_SELECTOR = "[data-sidebar-line]";
const ITEM_SELECTOR = "[data-sidebar-item]";

/** Where `key` moves focus from `position`, given each line's control count;
 *  null when it would leave the rail or stay put. */
export function nextSidebarPosition({ key, position, lineLengths }: {
  key: string;
  position: SidebarPosition;
  lineLengths: readonly number[];
}): SidebarPosition | null {
  const lastLine = lineLengths.length - 1;
  const lastItem = (lineLengths[position.line] ?? 1) - 1;
  if (key === SidebarKey.Down && position.line < lastLine) {
    return {
      line: position.line + 1,
      item: 0
    };
  }

  if (key === SidebarKey.Up && position.line > 0) {
    return {
      line: position.line - 1,
      item: 0
    };
  }

  if (key === SidebarKey.Right && position.item < lastItem) {
    return {
      line: position.line,
      item: position.item + 1
    };
  }

  if (key === SidebarKey.Left && position.item > 0) {
    return {
      line: position.line,
      item: position.item - 1
    };
  }

  if (key === SidebarKey.Home && lastLine >= 0) {
    return {
      line: 0,
      item: 0
    };
  }

  if (key === SidebarKey.End && lastLine >= 0) {
    return {
      line: lastLine,
      item: 0
    };
  }

  return null;
}

/** The rail's arrow-key movement over `element`'s lines (`data-sidebar-line`,
 *  each control along one marked `data-sidebar-item`). An attachment rather
 *  than a template listener: the list itself is not a control. */
export function sidebarArrowKeys(element: HTMLElement) {
  function moveFocus(e: KeyboardEvent) {
    const { target } = e;
    if (!SIDEBAR_KEYS.has(e.key) || !(target instanceof HTMLElement)) {
      return;
    }

    // A line can be its own control (a fold's summary), and a closed fold's
    // rows are skipped - they can't take focus.
    const lines = [...element.querySelectorAll<HTMLElement>(LINE_SELECTOR)]
      .filter(line => line.checkVisibility())
      .map(line => [
        ...line.matches(ITEM_SELECTOR) ? [line] : [],
        ...line.querySelectorAll<HTMLElement>(ITEM_SELECTOR)
      ]);
    const line = lines.findIndex(items => items.includes(target));
    if (line === -1) {
      return;
    }

    const next = nextSidebarPosition({
      key: e.key,
      position: {
        line,
        item: lines[line].indexOf(target)
      },
      lineLengths: lines.map(items => items.length)
    });
    if (next === null) {
      return;
    }

    e.preventDefault();
    lines[next.line][next.item]?.focus();
  }

  element.addEventListener("keydown", moveFocus);
  return () => element.removeEventListener("keydown", moveFocus);
}
