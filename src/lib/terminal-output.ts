export const TerminalFlushMode = {
  AnimationFrame: "animation-frame",
  Background: "background"
} as const;

/** Foreground output tracks the display unless the user is reading scrollback;
 * background and scrolled-back output stays live at a bounded cadence so token
 * streams cannot continuously compete with another app or the user's scrolling. */
export function terminalFlushMode({
  shown,
  windowFocused,
  readingScrollback
}: {
  shown: boolean;
  windowFocused: boolean;
  readingScrollback: boolean;
}): (typeof TerminalFlushMode)[keyof typeof TerminalFlushMode] {
  return shown && windowFocused && !readingScrollback
    ? TerminalFlushMode.AnimationFrame
    : TerminalFlushMode.Background;
}
