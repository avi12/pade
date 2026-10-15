// The window-level key chords App.svelte handles in the capture phase, read
// from a key event. Pure, so each chord's exact modifiers are unit-tested.

/** A window-level action a chord asks for. */
export const WindowShortcut = {
  /** Dev-only: F5 / Ctrl+Shift+R reloads the WebView - the escape hatch when
   *  Vite's HMR socket drops (a Tauri window wires no reload of its own).
   *  Ctrl+R is left to the terminal's shell reverse-search. */
  reload: "reload",
  /** Ctrl+Alt+[ cycles to the previous open PADE window. */
  previousWindow: "previousWindow",
  /** Ctrl+Alt+] cycles to the next open PADE window. */
  nextWindow: "nextWindow",
  /** Ctrl+Shift+N spawns a fresh empty window (the app-menu shortcut chip). */
  newWindow: "newWindow"
} as const;

const RELOAD_KEY = "F5";
const RELOAD_LETTER = "r";
const NEW_WINDOW_LETTER = "n";
// Keyed off `code`, not `key`: across layouts a bracket key can carry a
// different character, so the layout position is the modifier-independent match.
const PREVIOUS_WINDOW_CODE = "BracketLeft";
const NEXT_WINDOW_CODE = "BracketRight";

type ChordEvent = Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "shiftKey" | "altKey">;

function cycleShortcut(event: ChordEvent) {
  // Shift must be absent so the chord stays distinct (and off any future
  // Ctrl+Shift variant).
  const isCycleChord = event.ctrlKey && event.altKey && !event.shiftKey;
  if (!isCycleChord) {
    return null;
  }

  if (event.code === PREVIOUS_WINDOW_CODE) {
    return WindowShortcut.previousWindow;
  }

  return event.code === NEXT_WINDOW_CODE ? WindowShortcut.nextWindow : null;
}

/** The window-level action `event` asks for, or null for any other key. */
export function windowShortcutFor({ event, isDevelopment }: {
  event: ChordEvent;
  isDevelopment: boolean;
}) {
  const letter = event.key.toLowerCase();
  const isReload = event.key === RELOAD_KEY || (event.ctrlKey && event.shiftKey && letter === RELOAD_LETTER);
  if (isDevelopment && isReload) {
    return WindowShortcut.reload;
  }

  const isNewWindow = event.ctrlKey && event.shiftKey && letter === NEW_WINDOW_LETTER;
  if (isNewWindow) {
    return WindowShortcut.newWindow;
  }

  return cycleShortcut(event);
}
