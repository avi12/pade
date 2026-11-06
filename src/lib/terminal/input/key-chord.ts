// A keyboard chord as an agent's own config spells it ("ctrl+x ctrl+k",
// "escape", "meta+p") turned into the bytes a terminal sends for each of its
// keystrokes - how PADE presses an agent's shortcut through its PTY. Pure.

import { ENTER, ESCAPE } from "@/lib/terminal/input/terminal-sequences";

/** Ctrl with a letter sends that letter's code with its top three bits
 *  cleared: Ctrl+X is 0x18, Ctrl+K 0x0b. */
const CONTROL_KEY_MASK = 0b0001_1111;
/** A terminal sends DEL, not BS, for the Backspace key. */
const DELETE_CHARACTER_CODE = 0x7f;
const BACKSPACE = String.fromCharCode(DELETE_CHARACTER_CODE);
const TAB = "\t";
const SPACE = " ";
/** Separates the keystrokes of a chord, and the modifiers within one. */
const STEP_SEPARATOR = /\s+/u;
const MODIFIER_SEPARATOR = "+";
const LETTER = /^[a-z]$/u;
/** One printable character other than a letter: a digit or punctuation. */
const PLAIN_CHARACTER = /^[\x21-\x7e]$/u;

const Modifier = {
  Control: "ctrl",
  Alt: "alt",
  Meta: "meta",
  Option: "option",
  Shift: "shift"
} as const;
/** Alt, Meta and Option all reach a terminal program as an Esc prefix. */
const ESCAPE_PREFIX_MODIFIERS: ReadonlySet<string> = new Set([Modifier.Alt, Modifier.Meta, Modifier.Option]);
const KNOWN_MODIFIERS: ReadonlySet<string> = new Set(Object.values(Modifier));

/** The named keys a chord may end in, by every name configs use for them. */
const NAMED_KEYS: Readonly<Record<string, string>> = {
  escape: ESCAPE,
  esc: ESCAPE,
  enter: ENTER,
  return: ENTER,
  tab: TAB,
  space: SPACE,
  backspace: BACKSPACE
};

function controlKey(letter: string) {
  return String.fromCharCode(letter.charCodeAt(0) & CONTROL_KEY_MASK);
}

/** The bytes for a key with Ctrl and Shift as held, or null for a combination
 *  a terminal can't send apart from the bare key (Ctrl+Shift+K, Ctrl+Enter). */
function encodeKey({ key, isControl, isShift }: {
  key: string;
  isControl: boolean;
  isShift: boolean;
}) {
  if (LETTER.test(key)) {
    return encodeLetter({
      letter: key,
      isControl,
      isShift
    });
  }

  const unmodified = NAMED_KEYS[key] ?? (PLAIN_CHARACTER.test(key) ? key : null);
  const isModified = isControl || isShift;
  return isModified ? null : unmodified;
}

/** A letter with Ctrl or Shift - not both, which a terminal sends as Ctrl alone. */
function encodeLetter({ letter, isControl, isShift }: {
  letter: string;
  isControl: boolean;
  isShift: boolean;
}) {
  if (isControl) {
    return isShift ? null : controlKey(letter);
  }

  return isShift ? letter.toUpperCase() : letter;
}

/** The bytes for one keystroke ("ctrl+k"), or null for one a terminal can't
 *  send (Cmd+anything, an unknown modifier). */
function encodeKeystroke(keystroke: string) {
  const parts = keystroke.toLowerCase().split(MODIFIER_SEPARATOR);
  const key = parts.at(-1) ?? "";
  const modifiers = parts.slice(0, -1);
  const isKnown = modifiers.every(modifier => KNOWN_MODIFIERS.has(modifier));
  if (!isKnown || key === "") {
    return null;
  }

  const encoded = encodeKey({
    key,
    isControl: modifiers.includes(Modifier.Control),
    isShift: modifiers.includes(Modifier.Shift)
  });
  const isEscapePrefixed = modifiers.some(modifier => ESCAPE_PREFIX_MODIFIERS.has(modifier));
  if (encoded === null) {
    return null;
  }

  return isEscapePrefixed ? `${ESCAPE}${encoded}` : encoded;
}

/** Each keystroke of `chord` as the bytes to write, in order - or null when
 *  any of them can't be sent through a terminal. */
export function encodeChord(chord: string) {
  const steps = chord.trim().split(STEP_SEPARATOR).filter(step => step !== "");
  const keystrokes = steps.map(step => encodeKeystroke(step));
  if (keystrokes.length === 0 || keystrokes.some(keystroke => keystroke === null)) {
    return null;
  }

  return keystrokes.filter(keystroke => keystroke !== null);
}

/** A chord in one canonical spelling, for comparing two configs' spellings. */
export function normalizeChord(chord: string) {
  return chord.trim().toLowerCase().split(STEP_SEPARATOR).join(SPACE);
}
