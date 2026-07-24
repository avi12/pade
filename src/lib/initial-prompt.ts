// Delivering a new session's first prompt reliably.
//
// A freshly-spawned agent isn't ready for a prompt the instant its PTY exists:
// Claude Code first puts up a "trust this folder?" safety gate (a numbered
// choice prompt) and only then shows its input line. A prompt written blindly
// on mount collides with that gate — it lands in the menu, or sits in the input
// as an un-submitted paste. So the terminal watches the stream and drives the
// delivery: auto-accept the trust gate, wait for the REPL, then type and submit.
//
// This module owns the one piece of that flow worth testing in isolation —
// recognizing the trust gate from a frame of terminal output. The byte I/O and
// timing stay in Terminal.svelte, next to the rest of the PTY plumbing.
//
// Coupled to the CLI's *observable* output (the gate's wording), the same
// deliberate, documented Hyrum dependency as the context-percent regexes — not a
// stable PADE contract.

import { stripAnsi } from "@/lib/ansi";
import { detectChoicePrompt } from "@/lib/choice-prompt";

// The trust gate is the one choice prompt it is always safe to auto-answer with
// its default ("Yes, I trust this folder") before the first prompt — it's the
// user's own project, opened in PADE. Keying on the word "trust" keeps the
// auto-accept scoped to exactly that gate, never a real question the agent asks
// later (which the user must answer). Claude Code's gate reads
// "Is this a project you created or one you trust? … 1. Yes, I trust this folder".
const TRUST_KEYWORD_RE = /\btrust\b/i;

/** Whether a frame of terminal output is the agent's first-run "trust this
 *  folder?" gate: a numbered choice prompt (`❯ 1. …`) whose text mentions trust.
 *  Conservative by construction — it inherits `detectChoicePrompt`'s cursor +
 *  two-options requirement, then adds the trust keyword — so an ordinary numbered
 *  list, and any later multiple-choice question the agent genuinely needs
 *  answered, never register. */
export function isTrustGate(screen: string): boolean {
  return detectChoicePrompt(screen) && TRUST_KEYWORD_RE.test(stripAnsi(screen));
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
