// App-level keyboard shortcuts for the split panes — the sibling of
// `tab-shortcuts.ts` for panes instead of tabs. One capture-phase keydown
// listener so the chords win over a focused terminal, which would otherwise
// hand them to the agent as raw control codes. A pure matcher does the deciding
// and is unit-tested: `matchPaneShortcut` (chord + pane count → which pane
// action), and the registrar wires it to the app's handlers, resolving the
// target pane through `pane-nav` and leaving app text fields alone.
//
// The chords are chosen to never collide with `tab-shortcuts.ts`, which owns
// plain Ctrl+number (select tab), Ctrl+9 (last tab) and Ctrl+W (close tab):
//   Ctrl+[  previous pane   Ctrl+]  next pane
//   Ctrl+Alt+1..9  select the 1st..9th pane   Ctrl+Alt+W  close the active pane

import { isEditingText } from "@/lib/focus";
import { nextPaneId, paneIdAt, previousPaneId } from "@/lib/pane-nav";
import type { KeyChord } from "@/lib/tab-shortcuts";

/** The closed set of actions a pane shortcut can trigger. */
export const PaneAction = {
  Previous: "previous",
  Next: "next",
  SelectAt: "selectAt",
  Close: "close"
} as const;
export type PaneAction = (typeof PaneAction)[keyof typeof PaneAction];

/** What `matchPaneShortcut` resolves a chord to: a bare action, or the
 *  by-position pick that carries the 0-based pane index to select. */
export type PaneShortcut =
  | { action: typeof PaneAction.Previous | typeof PaneAction.Next | typeof PaneAction.Close }
  | {
    action: typeof PaneAction.SelectAt;
    index: number;
  };

/** The digit keys that pick a pane by position — Ctrl+Alt+1..9 select the
 *  1st..9th pane. This literal list is their authoritative home. */
const PANE_NUMBER_KEYS: readonly string[] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

/** Map a key chord to the pane action it triggers, or null when it isn't one.
 *  Ctrl+[ previous · Ctrl+] next (both plain Ctrl) · Ctrl+Alt+1..9 select that
 *  pane by position (only while it exists — a number past the last pane is a
 *  no-op) · Ctrl+Alt+W close the active pane. Every action needs at least one
 *  pane and the exact modifier set; Meta always disqualifies. */
export function matchPaneShortcut({ chord, paneCount }: {
  chord: KeyChord;
  paneCount: number;
}): PaneShortcut | null {
  const { key, ctrlKey, shiftKey, altKey, metaKey } = chord;
  if (paneCount === 0 || !ctrlKey || metaKey) {
    return null;
  }

  // Ctrl+[ / Ctrl+] cycle the panes — plain Ctrl, off the Ctrl+Alt chords below.
  const isPlainCtrl = !altKey && !shiftKey;
  if (isPlainCtrl) {
    if (key === "[") {
      return { action: PaneAction.Previous };
    }

    if (key === "]") {
      return { action: PaneAction.Next };
    }

    return null;
  }

  // Everything else is a Ctrl+Alt chord (Shift absent) — deliberately off the
  // plain Ctrl+number / Ctrl+W tab shortcuts so both sets coexist.
  const isCtrlAlt = altKey && !shiftKey;
  if (!isCtrlAlt) {
    return null;
  }

  if (key.toLowerCase() === "w") {
    return { action: PaneAction.Close };
  }

  const position = PANE_NUMBER_KEYS.indexOf(key);
  const selectsExistingPane = position !== -1 && position < paneCount;
  return selectsExistingPane ? {
    action: PaneAction.SelectAt,
    index: position
  } : null;
}

/** The handler wiring the app supplies. Prev/next/by-position all resolve to a
 *  single session id via `pane-nav`, so one `selectPane(id)` covers them; the
 *  registrar reads the live pane list and active id through the getters. */
export interface PaneShortcutHandlers {
  /** Make the pane with this session id active (previous / next / by-position). */
  selectPane: (id: string) => void;
  /** Close the active pane, animating it out (Ctrl+Alt+W). */
  closeActivePane: () => void;
  /** The shown panes, left→right — decides wrapping and the by-position pick. */
  paneIds: () => readonly string[];
  /** The active pane's session id, or null when no pane is focused. */
  activeId: () => string | null;
}

/** Register the capture-phase listener; returns the matching unregister. */
export function registerPaneShortcuts(handlers: PaneShortcutHandlers): () => void {
  function onKeyDown(event: KeyboardEvent) {
    const paneIds = handlers.paneIds();
    const shortcut = matchPaneShortcut({
      chord: event,
      paneCount: paneIds.length
    });
    if (shortcut === null) {
      return;
    }

    // Leave app text fields (session rename, commit message) to their own key
    // handling. The terminal's textarea is deliberately not exempt — these
    // shortcuts must work while an agent has focus, which is the common case.
    if (isEditingText(document.activeElement)) {
      return;
    }

    // Win over the terminal and any browser default by consuming the event here
    // in the capture phase, so xterm never sees the raw chord.
    event.preventDefault();
    event.stopImmediatePropagation();

    runShortcut({
      shortcut,
      handlers,
      paneIds
    });
  }

  addEventListener("keydown", onKeyDown, { capture: true });
  return () => removeEventListener("keydown", onKeyDown, { capture: true });
}

function runShortcut({ shortcut, handlers, paneIds }: {
  shortcut: PaneShortcut;
  handlers: PaneShortcutHandlers;
  paneIds: readonly string[];
}) {
  if (shortcut.action === PaneAction.Close) {
    handlers.closeActivePane();
    return;
  }

  const targetId = resolveTargetId({
    shortcut,
    paneIds,
    activeId: handlers.activeId()
  });
  if (targetId !== null) {
    handlers.selectPane(targetId);
  }
}

/** The session id a selection shortcut lands on: the pane at a picked position,
 *  or the pane a step away from the active one. Null when it can't resolve (no
 *  active pane for a step, or an empty split). */
function resolveTargetId({ shortcut, paneIds, activeId }: {
  shortcut: PaneShortcut;
  paneIds: readonly string[];
  activeId: string | null;
}): string | null {
  if (shortcut.action === PaneAction.SelectAt) {
    return paneIdAt({
      paneIds,
      index: shortcut.index
    });
  }

  if (activeId === null) {
    return null;
  }

  if (shortcut.action === PaneAction.Previous) {
    return previousPaneId({
      paneIds,
      activeId
    });
  }

  if (shortcut.action === PaneAction.Next) {
    return nextPaneId({
      paneIds,
      activeId
    });
  }

  return null;
}
