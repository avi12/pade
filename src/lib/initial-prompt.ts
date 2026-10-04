// Delivering a new session's first prompt reliably.
//
// A freshly-spawned agent isn't ready for a prompt the instant its PTY exists:
// Claude Code first puts up a "trust this folder?" safety gate (a numbered
// choice prompt) and only then shows its input line. A prompt written blindly
// on mount collides with that gate — it lands in the menu, or sits in the input
// as an un-submitted paste. So the terminal watches the stream and drives the
// delivery: auto-accept the trust gate, wait for the REPL, then type and submit.
//
// This module owns the pieces of that flow worth testing in isolation — reading
// the trust gate off the rendered screen and confirming the prompt's echo. The
// byte I/O and timing stay in Terminal.svelte, next to the rest of the PTY
// plumbing.
//
// Coupled to the CLI's *observable* output (the gate's wording), the same
// deliberate, documented Hyrum dependency as the context-percent regexes — not a
// stable PADE contract.

import { stripAnsi } from "@/lib/ansi";
import { SELECTION_CURSOR } from "@/lib/choice-prompt";

// The trust gate is the one choice prompt it is always safe to auto-answer before
// the first prompt — it's the user's own project, opened in PADE. Keying on the
// accepting option's own wording keeps the auto-answer scoped to exactly that
// gate, never a real question the agent asks later (which the user must answer).
const TRUST_OPTION_RE = /\btrust this folder\b/i;

// The gate's options sit on adjacent rows. A selection cursor further away than
// this belongs to something else — the REPL's own `❯` input line, which takes
// over the screen once the gate is answered.
const MAX_OPTION_ROWS_APART = 4;

/** The key that walks the trust gate's selection toward accepting it. */
export const TrustGateKey = {
  Confirm: "confirm",
  Down: "down",
  Up: "up"
} as const;
export type TrustGateKey = (typeof TrustGateKey)[keyof typeof TrustGateKey];

/** Which key moves the agent's first-run "trust this folder?" gate toward
 *  accepting, read off the rendered screen rows — or null when no gate is up.
 *  The rendered screen, not the raw stream, because the gate's selection moves by
 *  partial repaints a single chunk can't be read from, and its layout is the
 *  CLI's to change: Claude Code 2.1.286 lists `❯ No, exit` FIRST with
 *  `Yes, I trust this folder` below, so a blind Enter would quit the agent. */
export function trustGateKey(rows: readonly string[]): TrustGateKey | null {
  const trustRow = rows.findIndex(row => TRUST_OPTION_RE.test(row));
  if (trustRow === -1) {
    return null;
  }

  const firstOptionRow = Math.max(0, trustRow - MAX_OPTION_ROWS_APART);
  const optionRows = rows.slice(firstOptionRow, trustRow + MAX_OPTION_ROWS_APART + 1);
  const cursorOffset = optionRows.findIndex(row => row.trimStart().startsWith(SELECTION_CURSOR));
  if (cursorOffset === -1) {
    return null;
  }

  const cursorRow = firstOptionRow + cursorOffset;
  if (cursorRow === trustRow) {
    return TrustGateKey.Confirm;
  }

  return cursorRow < trustRow ? TrustGateKey.Down : TrustGateKey.Up;
}

/** How much of the prompt's collapsed text must reappear in the output to count
 *  as the composer echoing it — long enough to be distinctive, short enough
 *  that a TUI truncating a long prompt still matches. */
const ECHO_PREFIX_LENGTH = 24;

/** Terminal text reduced to bare glyphs: ANSI stripped, all whitespace removed —
 *  so a composer that wraps the prompt across lines still matches it. */
function collapsed(text: string): string {
  return stripAnsi(text).replaceAll(/\s+/g, "");
}

/** Whether the agent's output shows the delivered prompt — the composer (or the
 *  transcript) echoing its text back. A freshly-spawned TUI can paint its splash
 *  while not yet reading stdin, silently swallowing a prompt written "on ready";
 *  only this echo confirms the delivery landed, so the terminal re-delivers
 *  until it appears. */
export function promptEchoed({ output, prompt }: {
  output: string;
  prompt: string;
}): boolean {
  const needle = collapsed(prompt).slice(0, ECHO_PREFIX_LENGTH);
  return needle.length > 0 && collapsed(output).includes(needle);
}
