/** WebGL is useful only while the user can see the terminal. Releasing it for
 * background tabs and unfocused PADE windows frees the context's GPU memory
 * without pausing xterm's PTY parsing or scrollback updates. */
export function shouldUseWebgl({
  shown,
  windowFocused
}: {
  shown: boolean;
  windowFocused: boolean;
}): boolean {
  return shown && windowFocused;
}
