// The decisions behind usage pacing's "pause, don't kill" rest: the agent's
// hook holds each tool call (the session's and every subagent's) until the rest
// ends, so nothing is stopped and nothing needs a "continue". A session whose
// hook can't do that - the agent has none, the rest outlasts the longest hold,
// or the hook predates the feature - is stopped as before (`terminal-stop`).
// Pure; lib/stores/usage/usagePacing runs them.

import type { InterruptKeys } from "@/lib/agent/interrupt-keys";
import { MILLISECONDS_PER_MINUTE } from "@/lib/usage/pacing/pacing-policy";

/** The longest a hook holds one tool call through a rest. Mirrors
 *  `MAXIMUM_REST_HOLD_SECONDS` in `agents/running_model/pacing_control.rs`. */
export const MAXIMUM_REST_HOLD_MS = 55 * MILLISECONDS_PER_MINUTE;

/** Whether a rest of `restMs` is paused by holding tool calls rather than
 *  stopping the session. */
export function canHoldRest({ keys, restMs }: {
  keys: Pick<InterruptKeys, "canHoldToolCalls"> | null;
  restMs: number | null;
}) {
  const isWithinHold = restMs !== null && restMs <= MAXIMUM_REST_HOLD_MS;
  return keys !== null && keys.canHoldToolCalls && isWithinHold;
}

/** The rest end the hook is asked to hold until (epoch ms): a held pause's, else
 *  null - nothing is held. */
export function heldRestUntil(state: {
  isHeld: boolean;
  restUntil: number | null;
} | undefined) {
  return state?.isHeld === true ? state.restUntil : null;
}

/** Whether a held rest must fall back to the stop: the session still works, and
 *  its hook never noted holding a call since the rest was written - it predates
 *  the hold, or has not run a tool since. */
export function isHoldFallbackDue({ isWorking, stopSeenAt, restWrittenAt }: {
  isWorking: boolean;
  stopSeenAt: number | null;
  restWrittenAt: number;
}) {
  const isHoldNoted = stopSeenAt !== null && stopSeenAt >= restWrittenAt;
  return isWorking && !isHoldNoted;
}
