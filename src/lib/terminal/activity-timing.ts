// The timing behind a pane's working/ready status, shared with everything that
// reads that status to tell whether a prompt it typed was really submitted.

// How long the PTY must fall quiet before a working session settles back to
// "ready". This is a hysteresis window, not just a debounce: it must comfortably
// outlast the *slowest periodic repaint* a long-running task emits, or the badge
// flickers. A dev server started by the agent (e.g. `pnpm netlify` → Vite) keeps
// Claude repainting its elapsed-time counter ("1m 1s", "1m 2s", …) about once a
// second and dribbles the odd log line - if this window were shorter than that
// ~1 s cadence, the status would drop to "ready" in the gap after each tick and
// snap back to "working" on the next, a ~1 Hz flicker. Sitting above 1 s, one
// periodic tick re-arms the timer before it fires, so the session stays steadily
// "working" for the whole command and only settles ~1.5 s after output truly
// stops. The cost is the "ready" badge on a normal quiet turn appears ~0.8 s
// later than the old 700 ms - an acceptable trade for no flicker.
export const IDLE_MS = 1500;

const ECHO_SETTLE_MARGIN_MS = 500;

/** How long after a submitting Enter to check that the agent took the prompt
 *  up. The paste, and an Enter the composer swallowed, both echo a repaint that
 *  reads as `working` for one idle window; checked inside that window, a prompt
 *  left sitting unsent passes for submitted. Past it, only a session genuinely
 *  running the prompt still reads as `working`. */
export const SUBMIT_VERIFY_MS = IDLE_MS + ECHO_SETTLE_MARGIN_MS;
