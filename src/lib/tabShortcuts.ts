// App-level keyboard shortcuts for the agent tab strip. A single capture-phase
// keydown listener so the shortcuts win over a focused terminal — xterm would
// otherwise hand Ctrl+W / Ctrl+T etc. to the agent as raw control codes. The
// pure `matchTabShortcut` maps a key chord to an action and is unit-tested; the
// registrar wires actions to the app's handlers and leaves text fields alone.

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
 *  Ctrl+T new · Ctrl+Shift+T launch menu · Ctrl+W / Ctrl+F4 close ·
 *  Ctrl+Tab / Alt+Right next · Ctrl+Shift+Tab / Alt+Left previous. */
export function matchTabShortcut(chord: KeyChord): TabAction | null {
  const { key, ctrlKey, shiftKey, altKey, metaKey } = chord;
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

  // Every remaining shortcut is Ctrl-based, with Alt/Meta absent.
  if (!ctrlKey || altKey || metaKey) {
    return null;
  }

  const lowerKey = key.toLowerCase();
  if (lowerKey === "t") {
    return shiftKey ? TabAction.LaunchMenu : TabAction.New;
  }

  if (key === "Tab") {
    return shiftKey ? TabAction.Previous : TabAction.Next;
  }

  if (!shiftKey && (lowerKey === "w" || key === "F4")) {
    return TabAction.Close;
  }

  return null;
}

/** The action → handler wiring the app supplies. */
export interface TabShortcutHandlers {
  newTab: () => void;
  launchMenu: () => void;
  closeTab: () => void;
  next: () => void;
  previous: () => void;
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
    if (action === null) {
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
    run[action]();
  }

  window.addEventListener("keydown", onKeyDown, { capture: true });
  return () => window.removeEventListener("keydown", onKeyDown, { capture: true });
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
