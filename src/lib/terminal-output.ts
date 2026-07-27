export const TerminalFlushMode = {
  AnimationFrame: "animation-frame",
  Background: "background"
} as const;

/** Foreground output tracks the display; background output stays live at a
 * bounded cadence so token streams cannot continuously repaint other apps. */
export function terminalFlushMode({
  shown,
  windowFocused
}: {
  shown: boolean;
  windowFocused: boolean;
}): (typeof TerminalFlushMode)[keyof typeof TerminalFlushMode] {
  return shown && windowFocused
    ? TerminalFlushMode.AnimationFrame
    : TerminalFlushMode.Background;
}
