// The steps usage pacing takes on one running subagent - pure, so each rule is
// tested on its own (lib/usage/pacing/engine/account-round ranks them against the
// sessions' steps). A subagent works inside its parent's turn, so the parent's
// between-turn steps never reach it; the agent's own hook does, before every
// tool call the subagent makes (agents/running_model/pacing_control). Its
// steps, cheapest first:
//
//   gate     - held a few seconds before each of its tool calls. A subagent
//              spends in short cycles of one model reply and one tool call, so
//              a hold of h seconds on a cycle of c cuts its rate by h / (c + h).
//              The first hold is sized to the share the account must shed, and
//              every further step doubles it, up to a bound well inside the
//              hook's own timeout.
//   wrap up  - at the longest hold, its next tool call is refused with "finish
//              now with what you have and report back", so it returns.
//
// While the account must shed more than NO_SPAWN_SHED, no session on it may
// start a subagent at all. As room comes back each hold halves, and goes once
// it is shorter than the shortest; a subagent told to wrap up is finishing and
// is let be.

import { SubagentStepKind } from "@/lib/types/pacing";
import type { SubagentStep } from "@/lib/types/pacing";

/** A subagent's typical cycle of one reply and one tool call, in seconds. */
const TOOL_CYCLE_SECONDS = 20;
/** The shortest hold worth a step, and the longest before wrapping up. */
export const MINIMUM_HOLD_SECONDS = 5;
export const MAXIMUM_HOLD_SECONDS = 45;
/** Each further gate step multiplies the hold by this; easing divides by it. */
const HOLD_GROWTH = 2;
/** Above this share of its spend to shed, an account starts no new subagents. */
export const NO_SPAWN_SHED = 0.3;

/** The share of a subagent's unheld rate a hold of `holdSeconds` saves. */
export function holdSaving(holdSeconds: number) {
  return holdSeconds / (TOOL_CYCLE_SECONDS + holdSeconds);
}

/** The hold that saves `fraction` of a subagent's rate, within the bounds. */
export function holdForSaving(fraction: number) {
  const isBeyondReach = fraction >= 1;
  const exact = isBeyondReach ? MAXIMUM_HOLD_SECONDS : TOOL_CYCLE_SECONDS * fraction / (1 - fraction);
  return Math.min(MAXIMUM_HOLD_SECONDS, Math.max(MINIMUM_HOLD_SECONDS, Math.round(exact)));
}

/** Whether an account sheds so much it starts no new subagents - not while its
 *  shed is unknown. */
export function isBlockingSpawns(shed: number | null) {
  return shed !== null && shed >= NO_SPAWN_SHED;
}

/** A subagent's next step and what it saves of the load it spends now (already
 *  held by its current step) - null once it is wrapping up. With the shed
 *  unknown, the first hold is the shortest. */
export function nextSubagentStep({ current, shed }: {
  current: SubagentStep | undefined;
  shed: number | null;
}) {
  if (current === undefined) {
    const holdSeconds = holdForSaving(shed ?? 0);
    return {
      step: gate(holdSeconds),
      saving: holdSaving(holdSeconds)
    };
  }

  if (current.step === SubagentStepKind.enum.wrapUp) {
    return null;
  }

  if (current.holdSeconds >= MAXIMUM_HOLD_SECONDS) {
    return {
      step: {
        step: SubagentStepKind.enum.wrapUp
      } satisfies SubagentStep,
      saving: 1
    };
  }

  const holdSeconds = Math.min(MAXIMUM_HOLD_SECONDS, current.holdSeconds * HOLD_GROWTH);
  return {
    step: gate(holdSeconds),
    saving: 1 - (TOOL_CYCLE_SECONDS + current.holdSeconds) / (TOOL_CYCLE_SECONDS + holdSeconds)
  };
}

/** A subagent's step once its account has room again: a hold halves, and goes
 *  once it falls under the shortest; a wrap-up stays, the subagent finishing. */
export function easedSubagentStep(current: SubagentStep) {
  if (current.step === SubagentStepKind.enum.wrapUp) {
    return current;
  }

  const holdSeconds = Math.floor(current.holdSeconds / HOLD_GROWTH);
  return holdSeconds < MINIMUM_HOLD_SECONDS ? null : gate(holdSeconds);
}

function gate(holdSeconds: number) {
  return {
    step: SubagentStepKind.enum.gate,
    holdSeconds
  } satisfies SubagentStep;
}
