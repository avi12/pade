// App-level keyboard shortcuts for the agent tab strip. A single capture-phase
// keydown listener so the shortcuts win over a focused terminal - xterm would
// otherwise hand Ctrl+W / Ctrl+Alt+T / Ctrl+<number> etc. to the agent as raw
// control codes. Two pure matchers do the deciding and are unit-tested:
// `matchTabShortcut` (chord → action) and `matchTabSelection` (Ctrl+number →
// which tab to activate); the registrar wires both to the app's handlers and
// leaves app text fields alone.

import { isEditingText } from "@/lib/focus";
import { Key } from "@/lib/keys";

const NEW_TAB_KEY = "t";
const CLOSE_TAB_KEY = "w";

/** The closed set of actions a tab shortcut can trigger. */
export const TabAction = {
  New: "new",
  LaunchMenu: "launchMenu",
  Close: "close",
  Next: "next",
  Previous: "previous"
} as const;
type TabAction = (typeof TabAction)[keyof typeof TabAction];

/** The modifier + key fields a shortcut is decided from. A real KeyboardEvent
 * has these, so it is accepted structurally; a plain object keeps the matcher
 * unit-testable without a DOM. */
export interface KeyChord {
  key: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

/** Matching and display vocabulary live together, so changing a chord cannot
 * leave the tab-strip help text describing the previous binding. */
export const TAB_SHORTCUTS = {
  [TabAction.New]: {
    label: "Ctrl+Shift+T",
    description: "New agent tab",
    matches: ({ key, ctrlKey, shiftKey, altKey, metaKey }) =>
      key.toLowerCase() === NEW_TAB_KEY && ctrlKey && shiftKey && !altKey && !metaKey
  },
  [TabAction.LaunchMenu]: {
    label: "Ctrl+Alt+T",
    description: "Agent launch options",
    matches: ({ key, ctrlKey, shiftKey, altKey, metaKey }) =>
      key.toLowerCase() === NEW_TAB_KEY && ctrlKey && altKey && !shiftKey && !metaKey
  },
  [TabAction.Close]: {
    label: "Ctrl+W or Ctrl+F4",
    description: "Close the active tab",
    matches: ({ key, ctrlKey, shiftKey, altKey, metaKey }) =>
      ctrlKey && !shiftKey && !altKey && !metaKey && (key.toLowerCase() === CLOSE_TAB_KEY || key === Key.F4)
  },
  [TabAction.Next]: {
    label: "Ctrl+Tab or Alt+Right",
    description: "Select the next tab",
    matches: ({ key, ctrlKey, shiftKey, altKey, metaKey }) =>
      !shiftKey && !metaKey && (
        (key === Key.Tab && ctrlKey && !altKey)
        || (key === Key.ArrowRight && altKey && !ctrlKey)
      )
  },
  [TabAction.Previous]: {
    label: "Ctrl+Shift+Tab or Alt+Left",
    description: "Select the previous tab",
    matches: ({ key, ctrlKey, shiftKey, altKey, metaKey }) =>
      !metaKey && (
        (key === Key.Tab && ctrlKey && shiftKey && !altKey)
        || (key === Key.ArrowLeft && altKey && !ctrlKey && !shiftKey)
      )
  }
} as const satisfies Record<TabAction, {
  label: string;
  description: string;
  matches: (chord: KeyChord) => boolean;
}>;

/** Map a key chord to the tab action it triggers, or null when it is not one. */
export function matchTabShortcut(chord: KeyChord) {
  for (const action of Object.values(TabAction)) {
    if (TAB_SHORTCUTS[action].matches(chord)) {
      return action;
    }
  }

  return null;
}

/** The digit keys that pick a tab by position - Ctrl+1..8 select the 1st..8th
 *  tab. Ctrl+9 is special-cased (the LAST tab, whatever the count), so it's kept
 *  out of here. This literal list is their authoritative home. */
const TAB_NUMBER_KEYS: readonly string[] = ["1", "2", "3", "4", "5", "6", "7", "8"];
/** Ctrl+9 always jumps to the last tab - the browser/terminal convention. */
const LAST_TAB_KEY = "9";

/** Map a Ctrl+number chord to the 0-based index of the tab it selects, given how
 *  many tabs exist, or null when it isn't a tab-number chord. Ctrl+1 → first …
 *  Ctrl+8 → eighth, but only while that tab exists - a number past the last tab
 *  is a no-op; Ctrl+9 → the LAST tab, however many there are. Plain Ctrl only:
 *  any Shift/Alt/Meta, or no tabs at all, disqualifies it. */
export function matchTabSelection({ chord, count }: {
  chord: KeyChord;
  count: number;
}) {
  const { key, ctrlKey, shiftKey, altKey, metaKey } = chord;
  const isPlainCtrl = ctrlKey && !shiftKey && !altKey && !metaKey;
  if (!isPlainCtrl || count === 0) {
    return null;
  }

  if (key === LAST_TAB_KEY) {
    return count - 1;
  }

  const iTab = TAB_NUMBER_KEYS.indexOf(key);
  const isExistingTab = iTab !== -1 && iTab < count;
  return isExistingTab ? iTab : null;
}

/** The action → handler wiring the app supplies. */
export interface TabShortcutHandlers {
  newTab: () => void;
  launchMenu: () => void;
  closeTab: () => void;
  next: () => void;
  previous: () => void;
  /** Activate the tab at this 0-based index (Ctrl+number). */
  selectTab: (iTab: number) => void;
  /** How many session tabs currently exist - decides Ctrl+9 (last tab) and the
   *  out-of-range no-op. */
  tabCount: () => number;
}

/** Register the capture-phase listener; returns the matching unregister. */
export function registerTabShortcuts(handlers: TabShortcutHandlers) {
  const actionHandlers: Record<TabAction, () => void> = {
    [TabAction.New]: handlers.newTab,
    [TabAction.LaunchMenu]: handlers.launchMenu,
    [TabAction.Close]: handlers.closeTab,
    [TabAction.Next]: handlers.next,
    [TabAction.Previous]: handlers.previous
  };

  function onKeyDown(e: KeyboardEvent) {
    const action = matchTabShortcut(e);
    // Only reach for the number matcher when it isn't an action chord already.
    const iSelectedTab =
      action === null ? matchTabSelection({
        chord: e,
        count: handlers.tabCount()
      }) : null;
    if (action === null && iSelectedTab === null) {
      return;
    }

    // Leave app text fields (session rename, commit message) to their own key
    // handling. The terminal's textarea is deliberately not exempt - these
    // shortcuts must work while an agent has focus, which is the common case.
    if (isEditingText(document.activeElement)) {
      return;
    }

    // Win over the terminal and any browser default (Ctrl+W closing the webview,
    // Alt+Arrow history nav) by consuming the event here in the capture phase.
    e.preventDefault();
    e.stopImmediatePropagation();

    if (action !== null) {
      actionHandlers[action]();
      return;
    }

    if (iSelectedTab !== null) {
      handlers.selectTab(iSelectedTab);
    }
  }

  addEventListener("keydown", onKeyDown, { capture: true });
  return () => removeEventListener("keydown", onKeyDown, { capture: true });
}
