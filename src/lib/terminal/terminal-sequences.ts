// The terminal control sequences the pane writes - to the agent, or into xterm -
// composed from named parts so each one reads without a comment.

export const CONTROL_SEQUENCE_INTRODUCER = "\x1b[";

const ALTERNATE_SCREEN_PRIVATE_MODE = "?1049";
const SET_MODE = "h";

/** Written into xterm, not to the agent, when re-attaching to a session that is
 *  already painting the alternate screen. Wire constant shared with pty.rs, which
 *  detects this exact sequence to set `history.alternate` - change them together. */
export const ENTER_ALTERNATE_SCREEN = `${CONTROL_SEQUENCE_INTRODUCER}${ALTERNATE_SCREEN_PRIVATE_MODE}${SET_MODE}`;

// A fullscreen agent scrolls its own transcript a half-page per PageUp/PageDown
// (CSI 5 ~ / CSI 6 ~) - see the pane's wheel forwarding.
const PAGE_UP_PARAMETER = "5";
const PAGE_DOWN_PARAMETER = "6";
const TILDE_FINAL_BYTE = "~";
export const PAGE_UP = `${CONTROL_SEQUENCE_INTRODUCER}${PAGE_UP_PARAMETER}${TILDE_FINAL_BYTE}`;
export const PAGE_DOWN = `${CONTROL_SEQUENCE_INTRODUCER}${PAGE_DOWN_PARAMETER}${TILDE_FINAL_BYTE}`;

/** Carriage return - the "Enter" a CLI reads as "submit this line". */
export const ENTER = "\r";

/** The Up/Down arrow keys (CSI A / CSI B) - how a select menu's cursor moves. */
export const CURSOR_UP = `${CONTROL_SEQUENCE_INTRODUCER}A`;
export const CURSOR_DOWN = `${CONTROL_SEQUENCE_INTRODUCER}B`;

/** ^W (ETB) - the line discipline's "erase word backward". Sent for
 *  Ctrl+Backspace, whose legacy byte (^H) a TUI can't tell from a bare
 *  backspace; see the pane's key overrides. */
export const ERASE_WORD = "\x17";

// Focus reports (mode 1004) xterm emits when the pane gains/loses DOM focus.
// PADE never forwards them: a PADE pane is either front (focused) or hidden
// (where unfocused chrome is invisible anyway), and every tab switch would
// otherwise make the outgoing agent repaint its chrome - output the status
// heuristic has no way to tell from real work, flashing a ready session's
// badge to "working". Agents simply always render as focused.
const FOCUS_IN_FINAL_BYTE = "I";
const FOCUS_OUT_FINAL_BYTE = "O";
const FOCUS_IN = `${CONTROL_SEQUENCE_INTRODUCER}${FOCUS_IN_FINAL_BYTE}`;
const FOCUS_OUT = `${CONTROL_SEQUENCE_INTRODUCER}${FOCUS_OUT_FINAL_BYTE}`;

/** Whether xterm's outgoing data is a focus report, which never reaches the agent. */
export function isFocusReport(data: string) {
  return data === FOCUS_IN || data === FOCUS_OUT;
}
