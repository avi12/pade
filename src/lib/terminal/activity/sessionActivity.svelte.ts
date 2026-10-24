// A pane's session status, read off its PTY. Output flowing = working; a quiet
// gap while the process is alive = ready (done with its task, waiting for you);
// exit = done.

import { IDLE_MS } from "@/lib/terminal/activity/activity-timing";
import { ENTER } from "@/lib/terminal/input/terminal-sequences";
import { SessionStatus } from "@/lib/types";

// A resize makes the agent repaint; output within this window after one is
// treated as that repaint's echo, not fresh activity - so revealing a hidden
// pane (which refits it) can't flash the badge from "ready" to "working".
const RESIZE_SETTLE_MS = 400;
// How soon after a local keystroke an output chunk still reads as the prompt
// echoing that keystroke back. The echo round-trips in a few ms; the next
// chunk after typing pauses longer than this is genuinely the agent.
const KEYSTROKE_ECHO_MS = 250;

export function createSessionActivity({ onSettled }: {
  /** A working session just fell quiet and is ready again. */
  onSettled: () => void;
}) {
  let status = $state<SessionStatus>(SessionStatus.enum.starting);
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let lastKeystrokeAt = 0;
  let lastResizeAt = 0;

  return {
    get status() {
      return status;
    },
    /** A chunk of output arrived - a sign of life, unless it is an echo. */
    markActivity() {
      if (status === SessionStatus.enum.exited) {
        return;
      }

      // Ignore the agent's own resize-repaint: it isn't the agent working, so a
      // settled "ready" session shouldn't blink to "working" when its pane is
      // revealed and refitted. Real work arrives outside the settle window.
      const isResizeEcho =
        status === SessionStatus.enum.ready && Date.now() - lastResizeAt < RESIZE_SETTLE_MS;
      if (isResizeEcho) {
        return;
      }

      // Typing at the prompt echoes every keystroke back through the PTY. That
      // echo is the user composing, not the agent working, so a settled session
      // must not flash its indicator per keypress. Enter never stamps the
      // window (see noteInput), so a submitted prompt flips to working
      // immediately.
      const isKeystrokeEcho =
        status === SessionStatus.enum.ready && Date.now() - lastKeystrokeAt < KEYSTROKE_ECHO_MS;
      if (isKeystrokeEcho) {
        return;
      }

      status = SessionStatus.enum.working;
      clearTimeout(idleTimer);
      idleTimer = setTimeout(() => {
        if (status === SessionStatus.enum.working) {
          status = SessionStatus.enum.ready;
          onSettled();
        }
      }, IDLE_MS);
    },
    /** The user typed into the pane. Composing keystrokes arm the echo window so
     *  their round-trip doesn't flash the status; the submitting Enter
     *  deliberately does not - the burst it triggers is the agent starting real
     *  work. */
    noteInput(data: string) {
      if (data !== ENTER) {
        lastKeystrokeAt = Date.now();
      }
    },
    /** The grid or the agent's size changed - its repaint is not activity. */
    noteResize() {
      lastResizeAt = Date.now();
    },
    /** A session with history to replay was already running before this pane
     *  attached - it sits at its prompt, not "starting". */
    markAttached() {
      if (status === SessionStatus.enum.starting) {
        status = SessionStatus.enum.ready;
      }
    },
    /** The process is gone, or this WebView can no longer reach it. */
    markExited() {
      clearTimeout(idleTimer);
      status = SessionStatus.enum.exited;
    },
    dispose() {
      clearTimeout(idleTimer);
    }
  };
}
