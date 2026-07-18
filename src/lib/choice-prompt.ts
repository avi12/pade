// Best-effort detector for the multiple-choice question an agent CLI (Claude
// Code) puts up in the terminal and then waits on: a short numbered list of
// options with a highlighted selection cursor (`❯ 1. Yes` / `  2. No`). Spotting
// it lets the session's tab flash for attention while the pick is pending (see
// stores/sessionAttention). This is a heuristic coupled to the CLI's *observable*
// output — deliberately conservative so ordinary numbered lists in agent prose
// never trip it. What it keys on, and its limits, are documented at the export.

import { stripAnsi } from "@/lib/ansi";

/** The selection-cursor glyph Claude Code (Ink's select prompt) draws next to
 *  the highlighted option — U+276F, distinct from the input line's plain ">". */
const SELECTION_CURSOR = "❯";

// A numbered option token — "1." / "12)" — counted loosely (no surrounding
// whitespace required) so a framebuffer that joins the rows without a newline
// still tallies each option.
const NUMBERED_OPTION_RE = /\d{1,2}[.)]/g;

// The highlighted row: the selection cursor immediately before a numbered
// option, with at most a few spaces between.
const CURSOR_ON_OPTION_RE = new RegExp(`${SELECTION_CURSOR}\\s{0,3}\\d{1,2}[.)]`);

/** A single "1." can appear in prose; a real menu offers at least two options. */
const MIN_OPTIONS = 2;

/** Whether a chunk of PTY output is an agent's multiple-choice prompt awaiting a
 *  pick. Conservative on purpose — it requires BOTH the selection cursor sitting
 *  on a numbered option AND two or more numbered options in view — so a plain
 *  "1. … 2. …" list in the agent's prose (no cursor) and a lone "❯ 1." never
 *  register.
 *
 *  Known limits (documented so the flashing stays trustworthy): it keys on the
 *  U+276F cursor + a NUMBERED list, so a bullet/radio-only menu (`❯ ● …`) or a
 *  CLI using a different cursor glyph won't be detected, and it can't see a
 *  choice made off-screen. It errs toward missing a prompt over flashing a
 *  false one. */
export function detectChoicePrompt(text: string): boolean {
  if (!text.includes(SELECTION_CURSOR)) {
    return false;
  }

  const plain = stripAnsi(text);
  if (!CURSOR_ON_OPTION_RE.test(plain)) {
    return false;
  }

  const options = plain.match(NUMBERED_OPTION_RE);
  return options !== null && options.length >= MIN_OPTIONS;
}
