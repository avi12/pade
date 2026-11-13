// Which of an agent's limits is the worst one: the priority and ranking rules.

import { UsageWindowKind } from "@/lib/types/usage";
import type { Limit } from "@/lib/usage/groups/usage-model";

/** A quota counts as spent only when the vendor says it is fully spent. Anything
 *  below that is still usable: a weekly cap at 96% has work left in it, and
 *  moving that work elsewhere costs far more than the few percent it saves. */
export const USAGE_SPENT_PERCENTAGE = 100;

// Where a limit sits in the "what actually stops work" order, which the meter
// ranks by before it ranks by percentage.
const LimitPriority = {
  /** Extra usage while every real window still has room: spending credits never
   *  stops an agent, so a part-spent spend cap is not what anyone is about to
   *  hit - 79% of the monthly spend beside a session window at 44% is headroom. */
  Headroom: 0,
  /** A session / weekly / per-model window: the real walls. */
  Window: 1,
  /** Extra usage once a real window IS spent and credits carry the work: the
   *  spend cap is then the last thing between the agent and a stop. */
  Carrying: 2
} as const;

type LimitPriorityValue = (typeof LimitPriority)[keyof typeof LimitPriority];

function limitPriority({ limit, limits }: { limit: Limit; limits: Limit[] }) {
  const isCreditsCap = limit.kind === UsageWindowKind.enum.credits;
  if (!isCreditsCap) {
    return LimitPriority.Window;
  }

  if (limit.isUncapped) {
    return LimitPriority.Headroom;
  }

  const isSpentWindowRidingOnCredits =
    limit.isCarryingWork &&
    limits.some(other => other.kind !== UsageWindowKind.enum.credits && other.percentage >= USAGE_SPENT_PERCENTAGE);
  return isSpentWindowRidingOnCredits ? LimitPriority.Carrying : LimitPriority.Headroom;
}

// A limit paired with its priority, so the ranking below is written once and
// reused across agents (the spotlight compares each agent's winner).
export interface RankedLimit {
  limit: Limit;
  priority: LimitPriorityValue;
}

function rank({ limit, limits }: { limit: Limit; limits: Limit[] }) {
  return {
    limit,
    priority: limitPriority({
      limit,
      limits
    })
  };
}

// The one comparison behind every "worst" in this module: priority first, then
// the more-consumed limit.
export function outranks({ candidate, incumbent }: { candidate: RankedLimit; incumbent: RankedLimit }) {
  if (candidate.priority !== incumbent.priority) {
    return candidate.priority > incumbent.priority;
  }

  return candidate.limit.percentage > incumbent.limit.percentage;
}

export function worstRanked(limits: Limit[]) {
  let worst: RankedLimit | null = null;
  for (const limit of limits) {
    const candidate = rank({
      limit,
      limits
    });
    const isWorstSoFar =
      !worst ||
      outranks({
        candidate,
        incumbent: worst
      });
    if (isWorstSoFar) {
      worst = candidate;
    }
  }

  return worst;
}

/** The worst-consumed limit in a set - drives an agent's chip/pill color and the
 *  panel's "closest to a limit" signal. Ranked by `LimitPriority` before
 *  percentage, so extra usage never crowds out the windows that actually stop
 *  work. `null` for an agent with no limits. */
export function worstLimit(limits: Limit[]) {
  return worstRanked(limits)?.limit ?? null;
}
