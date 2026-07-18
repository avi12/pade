// App-level keyboard shortcuts for the agent tab strip. A single capture-phase
// keydown listener so the shortcuts win over a focused terminal — xterm would
// otherwise hand Ctrl+W / Ctrl+Alt+T / Ctrl+<number> etc. to the agent as raw
// control codes. Two pure matchers do the deciding and are unit-tested:
// `matchTabShortcut` (chord → action) and `matchTabSelection` (Ctrl+number →
// which tab to activate); the registrar wires both to the app's handlers and
// leaves app text fields alone.

/** The closed set of actions a tab shortcut can trigger. */
export const TabAction = {
  New: "new",
  LaunchMenu: "launchMenu",
  Close: "close",
  Next: "next",
  Previous: "previous"
} as const;
export type TabAction = (typeof TabAction)[keyof typeof TabAction];

/** The modifier + key fields a shortcut is decided from. A real KeyboardEvent
 *  has these, so it's accepted structurally; a plain object keeps the matcher
 *  unit-testable without a DOM. */
export interface KeyChord {
  key: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

/** Map a key chord to the tab action it triggers, or null when it isn't one.
 *  Ctrl+Shift+T new · Ctrl+Alt+T launch menu · Ctrl+W / Ctrl+F4 close ·
 *  Ctrl+Tab / Alt+Right next · Ctrl+Shift+Tab / Alt+Left previous. */
export function matchTabShortcut(chord: KeyChord): TabAction | null {
  const { key, ctrlKey, shiftKey, altKey, metaKey } = chord;
  const lowerKey = key.toLowerCase();
  // Alt+Arrow (alone) cycles tabs, echoing the browser's back/forward gesture.
  if (altKey && !ctrlKey && !metaKey && !shiftKey) {
    if (key === "ArrowRight") {
      return TabAction.Next;
    }

    if (key === "ArrowLeft") {
      return TabAction.Previous;
    }

    return null;
  }

  // Ctrl+Alt+T opens the launch menu — off plain Ctrl+T so a focused terminal
  // keeps that chord for itself.
  if (ctrlKey && altKey && !metaKey && !shiftKey && lowerKey === "t") {
    return TabAction.LaunchMenu;
  }

  // Every remaining shortcut is Ctrl-based, with Alt/Meta absent.
  if (!ctrlKey || altKey || metaKey) {
    return null;
  }

  // Ctrl+Shift+T opens a new agent tab; plain Ctrl+T is left to the terminal.
  if (lowerKey === "t") {
    return shiftKey ? TabAction.New : null;
  }

  if (key === "Tab") {
    return shiftKey ? TabAction.Previous : TabAction.Next;
  }

  if (!shiftKey && (lowerKey === "w" || key === "F4")) {
    return TabAction.Close;
  }

  return null;
}

/** The digit keys that pick a tab by position — Ctrl+1..8 select the 1st..8th
 *  tab. Ctrl+9 is special-cased (the LAST tab, whatever the count), so it's kept
 *  out of here. This literal list is their authoritative home. */
const TAB_NUMBER_KEYS: readonly string[] = ["1", "2", "3", "4", "5", "6", "7", "8"];
/** Ctrl+9 always jumps to the last tab — the browser/terminal convention. */
const LAST_TAB_KEY = "9";

/** Map a Ctrl+number chord to the 0-based index of the tab it selects, given how
 *  many tabs exist, or null when it isn't a tab-number chord. Ctrl+1 → first …
 *  Ctrl+8 → eighth, but only while that tab exists — a number past the last tab
 *  is a no-op; Ctrl+9 → the LAST tab, however many there are. Plain Ctrl only:
 *  any Shift/Alt/Meta, or no tabs at all, disqualifies it. */
export function matchTabSelection({ chord, count }: {
  chord: KeyChord;
  count: number;
}): number | null {
  const { key, ctrlKey, shiftKey, altKey, metaKey } = chord;
  const isPlainCtrl = ctrlKey && !shiftKey && !altKey && !metaKey;
  if (!isPlainCtrl || count === 0) {
    return null;
  }

  if (key === LAST_TAB_KEY) {
    return count - 1;
  }

  const position = TAB_NUMBER_KEYS.indexOf(key);
  const selectsExistingTab = position !== -1 && position < count;
  return selectsExistingTab ? position : null;
}

/** The action → handler wiring the app supplies. */
export interface TabShortcutHandlers {
  newTab: () => void;
  launchMenu: () => void;
  closeTab: () => void;
  next: () => void;
  previous: () => void;
  /** Activate the tab at this 0-based index (Ctrl+number). */
  selectTab: (index: number) => void;
  /** How many session tabs currently exist — decides Ctrl+9 (last tab) and the
   *  out-of-range no-op. */
  tabCount: () => number;
}

/** Register the capture-phase listener; returns the matching unregister. */
export function registerTabShortcuts(handlers: TabShortcutHandlers): () => void {
  const run: Record<TabAction, () => void> = {
    [TabAction.New]: handlers.newTab,
    [TabAction.LaunchMenu]: handlers.launchMenu,
    [TabAction.Close]: handlers.closeTab,
    [TabAction.Next]: handlers.next,
    [TabAction.Previous]: handlers.previous
  };

  function onKeyDown(event: KeyboardEvent) {
    const action = matchTabShortcut(event);
    // Only reach for the number matcher when it isn't an action chord already.
    const selectedTab =
      action === null ? matchTabSelection({
        chord: event,
        count: handlers.tabCount()
      }) : null;
    if (action === null && selectedTab === null) {
      return;
    }

    // Leave app text fields (session rename, commit message) to their own key
    // handling. The terminal's textarea is deliberately not exempt — these
    // shortcuts must work while an agent has focus, which is the common case.
    if (isEditingText(document.activeElement)) {
      return;
    }

    // Win over the terminal and any browser default (Ctrl+W closing the webview,
    // Alt+Arrow history nav) by consuming the event here in the capture phase.
    event.preventDefault();
    event.stopImmediatePropagation();

    if (action !== null) {
      run[action]();
      return;
    }

    if (selectedTab !== null) {
      handlers.selectTab(selectedTab);
    }
  }

  addEventListener("keydown", onKeyDown, { capture: true });
  return () => removeEventListener("keydown", onKeyDown, { capture: true });
}

/** True when focus sits in an editable field other than the terminal textarea. */
function isEditingText(element: Element | null): boolean {
  if (!(element instanceof HTMLElement)) {
    return false;
  }

  // xterm's hidden input is a <textarea> too, but shortcuts should still fire.
  if (element.classList.contains("xterm-helper-textarea")) {
    return false;
  }

  return (
    element.isContentEditable ||
    element instanceof HTMLInputElement ||
    element instanceof HTMLTextAreaElement
  );
}
