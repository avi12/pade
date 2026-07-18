// One home for stripping a terminal's ANSI/control sequences out of a chunk of
// output, so text matchers (the choice-prompt detector, the task-run detector)
// see the glyphs the TUI wrote rather than the colour and cursor-move codes
// interleaved with them. Built from a string (the ESC/BEL bytes written as
// unicode escapes) so no raw control character ever sits in the source.
const ANSI_ESCAPE_RE = new RegExp(
  // OSC: ESC ] … terminated by BEL or ST (ESC \).
  "\\u001b\\][^\\u0007\\u001b]*(?:\\u0007|\\u001b\\\\)"
    // Two-byte escapes: ESC then a single @–Z byte.
    + "|\\u001b[@-Z]"
    // CSI: ESC [ params intermediates final (SGR colours, cursor moves…).
    + "|\\u001b\\[[0-9;?]*[ -/]*[@-~]",
  "g"
);

/** Strip ANSI/control sequences from a chunk of terminal output. */
export function stripAnsi(text: string): string {
  return text.replaceAll(ANSI_ESCAPE_RE, "");
}
