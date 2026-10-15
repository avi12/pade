// xterm's DOM renderer is the remaining output hot path. Join token-sized PTY
// chunks before writing: one write per display frame in front, and a bounded
// live cadence in the background so another app never competes with dozens of
// terminal layouts and paints per second. While the user wheel-scrolls xterm's
// own document, output is held back until the gesture goes quiet (or a
// deadline passes), so the scroll position is not fought over.

import { terminalFlushMode, TerminalFlushMode } from "@/lib/terminal/terminal-output";

const BACKGROUND_TERMINAL_FLUSH_MS = 250;
const SCROLL_OUTPUT_QUIET_MS = 120;
const SCROLL_OUTPUT_MAXIMUM_DELAY_MS = 1000;

export function createTerminalOutputScheduler({ write, flushContext }: {
  /** Hand the joined output to xterm. */
  write: (output: string) => void;
  /** Where the pane stands right now - decides the cadence of the next flush. */
  flushContext: () => {
    isShown: boolean;
    isWindowFocused: boolean;
    isReadingScrollback: boolean;
  };
}) {
  let queuedOutput = "";
  let outputFrame: number | undefined;
  let outputTimer: ReturnType<typeof setTimeout> | undefined;
  let isActivelyScrolling = false;
  let scrollQuietTimer: ReturnType<typeof setTimeout> | undefined;
  let scrollDeadlineTimer: ReturnType<typeof setTimeout> | undefined;
  let isDisposed = false;

  function flush() {
    outputFrame = undefined;
    outputTimer = undefined;

    if (!queuedOutput || isDisposed) {
      return;
    }

    const output = queuedOutput;
    queuedOutput = "";
    write(output);
  }

  function cancelScheduled() {
    if (outputFrame !== undefined) {
      cancelAnimationFrame(outputFrame);
      outputFrame = undefined;
    }

    clearTimeout(outputTimer);
    outputTimer = undefined;
  }

  function scheduleForeground() {
    clearTimeout(outputTimer);
    outputTimer = undefined;
    outputFrame ??= requestAnimationFrame(flush);
  }

  function scheduleBackground() {
    if (outputFrame !== undefined) {
      cancelAnimationFrame(outputFrame);
      outputFrame = undefined;
    }

    outputTimer ??= setTimeout(flush, BACKGROUND_TERMINAL_FLUSH_MS);
  }

  function schedule() {
    if (!queuedOutput) {
      return;
    }

    const mode = terminalFlushMode({
      ...flushContext(),
      isScrolling: isActivelyScrolling
    });
    if (mode === TerminalFlushMode.Deferred) {
      cancelScheduled();
      return;
    }

    if (mode === TerminalFlushMode.Background) {
      scheduleBackground();
      return;
    }

    scheduleForeground();
  }

  return {
    /** Re-pick the cadence for whatever is queued - the pane was shown, hidden,
     *  focused or scrolled. */
    schedule,
    queue(output: string) {
      queuedOutput += output;
      schedule();
    },
    /** A wheel tick moved xterm's own document: hold output until it settles. */
    noteScroll() {
      isActivelyScrolling = true;
      cancelScheduled();
      clearTimeout(scrollQuietTimer);
      scrollQuietTimer = setTimeout(() => {
        isActivelyScrolling = false;
        scrollQuietTimer = undefined;
        clearTimeout(scrollDeadlineTimer);
        scrollDeadlineTimer = undefined;
        flush();
      }, SCROLL_OUTPUT_QUIET_MS);
      scrollDeadlineTimer ??= setTimeout(() => {
        scrollDeadlineTimer = undefined;
        flush();
      }, SCROLL_OUTPUT_MAXIMUM_DELAY_MS);
    },
    dispose() {
      isDisposed = true;
      clearTimeout(outputTimer);
      clearTimeout(scrollQuietTimer);
      clearTimeout(scrollDeadlineTimer);

      if (outputFrame !== undefined) {
        cancelAnimationFrame(outputFrame);
      }
    }
  };
}
