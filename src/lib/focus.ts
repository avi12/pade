// Where keyboard focus sits - the one place the app-level shortcut registrars
// (tab-shortcuts, pane-shortcuts) ask "is the user typing in a field?" before
// deciding whether to swallow a chord. One authoritative home (DRY).

/** The `KeyboardEvent.key` values the app's keyboard handling reacts to - one
 *  home so the roving lists, the shortcut matchers and the drag's cancel key
 *  never spell a key twice. */
export const Key = {
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
  Home: "Home",
  End: "End",
  Escape: "Escape",
  Tab: "Tab",
  F4: "F4"
} as const;

/** The class xterm gives the hidden `<textarea>` that actually receives keystrokes
 *  for a terminal - the single home for that selector, shared by the queries below. */
const XTERM_HELPER_TEXTAREA = "xterm-helper-textarea";

const PANE_ID_ATTRIBUTE = "data-pane-id";

/** True when focus sits in an editable field OTHER than the terminal's own hidden
 *  textarea - so app shortcuts leave real form inputs (rename boxes, the picker)
 *  alone but still fire over xterm, whose helper `<textarea>` must not count as
 *  "editing" or the shortcuts would never reach a focused terminal. */
export function isEditingText(elFocused: Element | null) {
  if (!(elFocused instanceof HTMLElement)) {
    return false;
  }

  if (elFocused.classList.contains(XTERM_HELPER_TEXTAREA)) {
    return false;
  }

  return (
    elFocused.isContentEditable ||
    elFocused instanceof HTMLInputElement ||
    elFocused instanceof HTMLTextAreaElement
  );
}

/** Hand the keyboard to the terminal pane with this id (its xterm helper
 *  `<textarea>`). Used after an out-of-pane action - clicking "Send to agent"
 *  moves DOM focus to that button - so the next keystroke reaches the agent
 *  without a manual click back into the pane. No-op if the pane isn't mounted. */
export function focusTerminalPane(paneId: string) {
  const elSlot = [...document.querySelectorAll<HTMLElement>(`[${PANE_ID_ATTRIBUTE}]`)]
    .find(elCandidate => elCandidate.getAttribute(PANE_ID_ATTRIBUTE) === paneId);
  elSlot?.querySelector<HTMLTextAreaElement>(`.${XTERM_HELPER_TEXTAREA}`)?.focus();
}
