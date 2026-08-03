/** Resolve after `delayMs` milliseconds — a deliberate, awaitable pause, e.g.
 *  letting a bracketed paste settle in a TUI's composer before its submitting
 *  Enter so the post-paste guard can't fold the CR into the paste burst. (A
 *  caller that must cancel the wait on teardown tracks its own timer instead.) */
export function afterDelay(delayMs: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, delayMs));
}
