// Where keyboard focus sits — the one place the app-level shortcut registrars
// (tab-shortcuts, pane-shortcuts) ask "is the user typing in a field?" before
// deciding whether to swallow a chord. One authoritative home (DRY).

/** The class xterm gives the hidden `<textarea>` that actually receives keystrokes
 *  for a terminal — the single home for that selector, shared by the queries below. */
const XTERM_HELPER_TEXTAREA = "xterm-helper-textarea";

/** True when focus sits in an editable field OTHER than the terminal's own hidden
 *  textarea — so app shortcuts leave real form inputs (rename boxes, the picker)
 *  alone but still fire over xterm, whose helper `<textarea>` must not count as
 *  "editing" or the shortcuts would never reach a focused terminal. */
export function isEditingText(element: Element | null): boolean {
  if (!(element instanceof HTMLElement)) {
    return false;
  }

  if (element.classList.contains(XTERM_HELPER_TEXTAREA)) {
    return false;
  }

  return (
    element.isContentEditable ||
    element instanceof HTMLInputElement ||
    element instanceof HTMLTextAreaElement
  );
}

/** Hand the keyboard to the terminal pane with this id (its xterm helper
 *  `<textarea>`). Used after an out-of-pane action — clicking "Send to agent"
 *  moves DOM focus to that button — so the next keystroke reaches the agent
 *  without a manual click back into the pane. No-op if the pane isn't mounted. */
export function focusTerminalPane(paneId: string): void {
  for (const slot of document.querySelectorAll<HTMLElement>("[data-pane-id]")) {
    if (slot.getAttribute("data-pane-id") !== paneId) {
      continue;
    }

    slot.querySelector<HTMLTextAreaElement>(`.${XTERM_HELPER_TEXTAREA}`)?.focus();
    return;
  }
}
