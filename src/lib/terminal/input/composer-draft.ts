// An estimate of how much unsent text the user has in an agent's composer,
// read off the keystrokes the pane sends - so PADE never types its own prompt
// (a handoff request, pacing's "continue") into the middle of the user's.
// The agent's screen is the truth, but parsing every TUI's prompt box is
// brittle; the keystrokes are PADE's own to read.

import { BRACKETED_PASTE_END, BRACKETED_PASTE_START } from "@/lib/terminal/input/terminal-input";
import { CONTROL_SEQUENCE_INTRODUCER, ENTER, ESCAPE } from "@/lib/terminal/input/terminal-sequences";

/** ^C - an agent TUI clears its composer (or interrupts) on it. */
const CLEAR_LINE_INTERRUPT = "\x03";
/** ^U - the line discipline's "erase to start of line". */
const CLEAR_LINE_TO_START = "\x15";
/** Esc twice - how Claude Code clears its composer. */
const DOUBLE_ESCAPE = `${ESCAPE}${ESCAPE}`;
/** The two bytes a Backspace sends, by terminal. */
const BACKSPACES = new Set(["\x7f", "\b"]);
/** The first printable character code; anything below is a control byte. */
const FIRST_PRINTABLE_CODE = 0x20;

const CLEARING_INPUTS = new Set([CLEAR_LINE_INTERRUPT, CLEAR_LINE_TO_START, DOUBLE_ESCAPE]);

function printableLength(text: string) {
  let length = 0;
  for (const character of text) {
    const isPrintable = (character.codePointAt(0) ?? 0) >= FIRST_PRINTABLE_CODE;
    if (isPrintable) {
      length += 1;
    }
  }

  return length;
}

/** The composer's estimated length after the user sends `data`: Enter and the
 *  clearing keys empty it, Backspace takes a character, a key's escape sequence
 *  (an arrow, a function key) leaves it, and typed or pasted text adds its
 *  printable characters. */
export function nextDraftLength({ length, data }: { length: number; data: string }) {
  const isSubmitted = data.endsWith(ENTER) && !data.startsWith(BRACKETED_PASTE_START);
  if (isSubmitted || CLEARING_INPUTS.has(data)) {
    return 0;
  }

  if (BACKSPACES.has(data)) {
    return Math.max(0, length - 1);
  }

  if (data.startsWith(BRACKETED_PASTE_START)) {
    return length + printableLength(data.slice(BRACKETED_PASTE_START.length).replace(BRACKETED_PASTE_END, ""));
  }

  const isKeySequence = data.startsWith(CONTROL_SEQUENCE_INTRODUCER) || data === ESCAPE;
  if (isKeySequence) {
    return length;
  }

  return length + printableLength(data);
}
