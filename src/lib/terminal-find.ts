// Find-in-terminal, minus its UI: the chords that open and drive the bar, the
// options a search runs under, the direction it steps in, and how a result count
// reads. The bar itself is `lib/TerminalFind.svelte`; the searching is xterm's
// own search addon. Keeping this here means the chords, the option set and the
// wording are unit-tested without a DOM and have exactly one home.

import type { IconName } from "@/lib/Icon.svelte";
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

/** How the term is matched. The names are the search addon's own option keys, so
 *  the toggled state IS the object handed to it — nothing maps in between. */
export const FindOption = {
  CaseSensitive: "caseSensitive",
  WholeWord: "wholeWord",
  Regex: "regex"
} as const;
export type FindOption = (typeof FindOption)[keyof typeof FindOption];

/** Which options a search runs under, all off by default: a plain substring,
 *  case-insensitive, anywhere in the line. */
export type FindOptions = Record<FindOption, boolean>;

export function defaultFindOptions(): FindOptions {
  return {
    [FindOption.CaseSensitive]: false,
    [FindOption.WholeWord]: false,
    [FindOption.Regex]: false
  };
}

/** The toggles as the bar draws them, in the order they sit in it. Glyph, label
 *  and chord live beside the option they belong to, so a button, its tooltip and
 *  the key that flips it can never describe three different things. */
export const FIND_OPTION_TOGGLES = [
  {
    option: FindOption.CaseSensitive,
    icon: "caseSensitive",
    label: "Match case",
    chordKey: "c"
  },
  {
    option: FindOption.WholeWord,
    icon: "wholeWord",
    label: "Whole word",
    chordKey: "w"
  },
  {
    option: FindOption.Regex,
    icon: "regex",
    label: "Regular expression",
    chordKey: "r"
  }
] as const satisfies readonly {
  option: FindOption;
  icon: IconName;
  label: string;
  chordKey: string;
}[];

/** Alt+C / Alt+W / Alt+R flip a toggle while the caret stays in the find box —
 *  the browser-and-editor convention. Alt alone: Ctrl+Alt is AltGr on a European
 *  keyboard, where it types characters people search for. */
export function matchFindOptionChord({ key, ctrlKey, shiftKey, altKey, metaKey }: KeyChord): FindOption | null {
  if (!altKey || ctrlKey || shiftKey || metaKey) {
    return null;
  }

  const pressed = key.toLowerCase();
  for (const toggle of FIND_OPTION_TOGGLES) {
    if (toggle.chordKey === pressed) {
      return toggle.option;
    }
  }

  return null;
}

/** Whether the term can be searched for as typed. A literal always can; a
 *  half-written pattern ("(", "a{2,") cannot, and asking the engine to compile it
 *  would throw on the keystroke that got there — so the bar says so instead. */
export function isSearchablePattern({ term, regex }: {
  term: string;
  regex: boolean;
}): boolean {
  if (!regex) {
    return true;
  }

  try {
    new RegExp(term);
    return true;
  } catch {
    return false;
  }
}

/** The find bar's result readout. Nothing typed yet reads as "" — a count of
 *  zero matches is a finding, an empty box is not. `resultIndex` is -1 when the
 *  addon has more matches than it will decorate (it stops tracking which one is
 *  active), so the total is all that can honestly be shown there. */
export function findResultLabel({ term, resultIndex, resultCount, searchable = true }: {
  term: string;
  resultIndex: number;
  resultCount: number;
  /** False while a regex is still half-typed — the count is stale, not zero. */
  searchable?: boolean;
}): string {
  if (term.length === 0) {
    return "";
  }

  if (!searchable) {
    return "Bad pattern";
  }

  if (resultCount === 0) {
    return "No matches";
  }

  if (resultIndex < 0) {
    return `${resultCount} matches`;
  }

  return `${resultIndex + 1} of ${resultCount}`;
}
