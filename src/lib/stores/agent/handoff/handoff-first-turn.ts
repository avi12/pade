import { agentReportsBusy } from "@/lib/stores/agent/agentActivity.svelte";
import type { HandoffTimers } from "@/lib/stores/agent/handoff/handoff-timers";

// How often the successor is checked for having finished its first turn (the
// doc is certainly consumed by then), and how long before we stop watching.
const SUCCESSOR_POLL_MS = 3_000;
const SUCCESSOR_DEADLINE_MS = 10 * 60_000;

// True once the successor has really run its first turn - the turn that reads
// the doc. Only the agent's OWN report counts (stores/agentActivity: busy while
// a turn runs, idle when it ends). PADE's session status can't stand in for it:
// it reads any output as work, and a successor that merely booted noisily and
// then fell over - a CLI printing an update banner and rejecting its configured
// model - looked exactly like a finished turn, which deleted the handoff doc it
// had never read. Anything else (an agent that reports no activity at all, a
// session that disappears, a first turn still running at the deadline) answers
// false, and the doc stays.
export function waitForFirstTurn({
  timers,
  isStillOpen,
  id
}: {
  timers: HandoffTimers;
  isStillOpen: (sessionId: string) => boolean;
  id: string;
}) {
  return new Promise<boolean>(resolve => {
    let hasSeenTurn = false;
    const startedAt = Date.now();
    function poll() {
      const isBusy = agentReportsBusy(id);
      hasSeenTurn ||= isBusy;
      const isTurnFinished = hasSeenTurn && !isBusy;
      if (isTurnFinished) {
        resolve(true);
        return;
      }

      const isExpired = Date.now() - startedAt > SUCCESSOR_DEADLINE_MS;
      const isNotIsStillOpenIdOr = !isStillOpen(id) || isExpired;
      if (isNotIsStillOpenIdOr) {
        resolve(false);
        return;
      }

      timers.trackTimer({
        handler: poll,
        delayMs: SUCCESSOR_POLL_MS
      });
    }

    timers.trackTimer({
      handler: poll,
      delayMs: SUCCESSOR_POLL_MS
    });
  });
}
