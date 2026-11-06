// The decisions behind usage pacing's terminal stop - the fallback for a stop
// due mid-turn that a session can't take through its hook (it predates the
// hook, or the hook never fired). Pure; lib/stores/usage/pacingInterrupt runs
// them against the live session.
//
//   1. Did the hook take it? Only when the agent has a mid-turn hook and that
//      hook answered a call after the stop was written.
//   2. Which keys? Esc alone ends the running turn but leaves background
//      subagents running, so a session with one running gets the agent's
//      stop-all action first, then the interrupt.
//   3. Did it work? A subagent that still writes to its transcript once the
//      keys had time to land is still running: try once more, then tell the
//      user instead of looping.

import type { InterruptKeys, KeyPress } from "@/lib/agent/interrupt-keys";
import { z } from "zod";

/** How long after the keys a stopped subagent may still write its last line. */
const INTERRUPT_SETTLE_MS = 3000;
/** Presses before PADE gives up and asks the user: the first and one retry. */
export const MAXIMUM_STOP_ATTEMPTS = 2;

/** Whether the session's own hook took the stop: it answered a call while
 *  stopping, after the stop was written. */
export function isHookTakingStop({ keys, stopSeenAt, stopWrittenAt }: {
  keys: Pick<InterruptKeys, "hasMidTurnStopHook">;
  stopSeenAt: number | null;
  stopWrittenAt: number;
}) {
  return keys.hasMidTurnStopHook && stopSeenAt !== null && stopSeenAt >= stopWrittenAt;
}

/** The shortcuts to press, in order: the stop-all action while a subagent
 *  runs, then the interrupt (also ending the turn a stop-all starts, in which
 *  the agent answers that its subagents were stopped). The interrupt alone
 *  when no subagent runs - and nothing when the session has nothing running. */
export function interruptPresses({ keys, hasRunningSubagents, isWorking }: {
  keys: InterruptKeys;
  hasRunningSubagents: boolean;
  isWorking: boolean;
}) {
  const presses: KeyPress[] = [];
  if (hasRunningSubagents && keys.stopAll !== null) {
    presses.push(keys.stopAll);
  }

  const isInterruptDue = hasRunningSubagents || isWorking;
  if (isInterruptDue && keys.interrupt !== null) {
    presses.push(keys.interrupt);
  }

  return presses;
}

/** The subagents still writing once the keys had time to land. */
export function stillWritingSubagents<Subagent extends { writtenAt: number }>({ subagents, interruptedAt }: {
  subagents: readonly Subagent[];
  interruptedAt: number;
}) {
  return subagents.filter(subagent => subagent.writtenAt > interruptedAt + INTERRUPT_SETTLE_MS);
}

export const StopVerdict = z.enum(["stopped", "retry", "unstoppable"]);

/** What follows attempt `attempt` (from 1): done once nothing still writes,
 *  one more press while attempts remain, else hand it to the user. */
export function stopVerdict({ stillWritingCount, attempt }: {
  stillWritingCount: number;
  attempt: number;
}) {
  if (stillWritingCount === 0) {
    return StopVerdict.enum.stopped;
  }

  return attempt < MAXIMUM_STOP_ATTEMPTS ? StopVerdict.enum.retry : StopVerdict.enum.unstoppable;
}
