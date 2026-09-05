// Find-in-terminal, minus its UI: the chord that opens the bar, the direction a
// search runs in, and how a result count reads. The bar itself is
// `lib/TerminalFind.svelte`; the searching is xterm's own search addon. Keeping
// these three here means the chord and the wording are unit-tested without a DOM
// and have exactly one home.

import type { KeyChord } from "@/lib/tab-shortcuts";

/** Ctrl+F over a terminal pane — open the find bar and put the caret in it.
 *  Plain Ctrl only: Ctrl+Shift+F and Ctrl+Alt+F belong to whatever the agent
 *  binds them to, and the terminal still forwards those. */
export function isFindShortcut({ key, ctrlKey, shiftKey, altKey, metaKey }: KeyChord): boolean {
  return key.toLowerCase() === "f" && ctrlKey && !shiftKey && !altKey && !metaKey;
}

/** Which way through the matches a search step moves. */
export const FindDirection = {
  Next: "next",
  Previous: "previous"
} as const;
export type FindDirection = (typeof FindDirection)[keyof typeof FindDirection];

/** The find bar's result readout. Nothing typed yet reads as "" — a count of
 *  zero matches is a finding, an empty box is not. `resultIndex` is -1 when the
 *  addon has more matches than it will decorate (it stops tracking which one is
 *  active), so the total is all that can honestly be shown there. */
export function findResultLabel({ term, resultIndex, resultCount }: {
  term: string;
  resultIndex: number;
  resultCount: number;
}): string {
  if (term.length === 0) {
    return "";
  }

  if (resultCount === 0) {
    return "No matches";
  }

  if (resultIndex < 0) {
    return `${resultCount} matches`;
  }

  return `${resultIndex + 1} of ${resultCount}`;
}
