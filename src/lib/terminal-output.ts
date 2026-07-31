export const TerminalFlushMode = {
  AnimationFrame: "animation-frame",
  Background: "background",
  Deferred: "deferred"
} as const;

/** Foreground output tracks the display unless the user is reading scrollback;
 * background and scrolled-back output stays live at a bounded cadence so token
 * streams cannot continuously compete with another app or the user's scrolling. */
export function terminalFlushMode({
  shown,
  windowFocused,
  readingScrollback,
  scrolling
}: {
  shown: boolean;
  windowFocused: boolean;
  readingScrollback: boolean;
  scrolling: boolean;
}): (typeof TerminalFlushMode)[keyof typeof TerminalFlushMode] {
  if (scrolling) {
    return TerminalFlushMode.Deferred;
  }

  return shown && windowFocused && !readingScrollback
    ? TerminalFlushMode.AnimationFrame
    : TerminalFlushMode.Background;
}
