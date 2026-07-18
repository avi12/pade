// Where keyboard focus sits — the one place the app-level shortcut registrars
// (tab-shortcuts, pane-shortcuts) ask "is the user typing in a field?" before
// deciding whether to swallow a chord. One authoritative home (DRY).

/** True when focus sits in an editable field OTHER than the terminal's own hidden
 *  textarea — so app shortcuts leave real form inputs (rename boxes, the picker)
 *  alone but still fire over xterm, whose helper `<textarea>` must not count as
 *  "editing" or the shortcuts would never reach a focused terminal. */
export function isEditingText(element: Element | null): boolean {
  if (!(element instanceof HTMLElement)) {
    return false;
  }

  if (element.classList.contains("xterm-helper-textarea")) {
    return false;
  }

  return (
    element.isContentEditable ||
    element instanceof HTMLInputElement ||
    element instanceof HTMLTextAreaElement
  );
}
