export const TerminalFlushMode = {
  AnimationFrame: "animation-frame",
  Background: "background",
  Deferred: "deferred"
} as const;

/** Whether a wheel tick moves xterm's OWN viewport — the only case where the
 * wheel competes with incoming output for paints, and therefore the only case
 * that may defer them.
 *
 * When the agent has grabbed the mouse (a fullscreen TUI: Claude, Codex, a
 * pager) the tick is forwarded to it as a mouse report, and when there is no
 * scrollback it is forwarded as PageUp/PageDown. Either way the wheel is *input*
 * — the frame the agent paints back IS the scroll — so deferring output there
 * does not protect a scroll position, it withholds the scroll itself. That was
 * the lag: the agent answered every tick immediately while the terminal sat on
 * the frames until the gesture went quiet. */
export function wheelScrollsTerminalDocument({
  agentOwnsMouse,
  hasNativeScrollback
}: {
  agentOwnsMouse: boolean;
  hasNativeScrollback: boolean;
}): boolean {
  return !agentOwnsMouse && hasNativeScrollback;
}

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
