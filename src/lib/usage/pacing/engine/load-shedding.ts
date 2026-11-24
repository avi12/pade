// How usage pacing shares one account's slowdown among the sessions spending
// it - pure, so every rule is tested on its own (lib/stores/usage/usagePacing
// feeds it live sessions).
//
// An account's burn rate is one number for everything on it: every session, and
// every subagent those sessions run. So when it runs over pace, the question is
// not "should this session slow down" but "how much of the account's spend must
// go, and from whom". Each session's share is estimated from what drives its
// spend. Where the agent's own transcripts say what it spent, the share is
// measured: the priced tokens it and its subagents burned per minute lately
// (`token-price`, `measured-burn`). Otherwise it is estimated:
//
//     load = model price × effort weight × working share × (1 + subagents)
//
// with the model's price the same list-price ratio the measured tokens are
// priced by (Haiku 1, Sonnet 4, Opus 8, Fable 20), and the estimate rescaled
// onto the measured sessions' scale (`measuredPerEstimated`) so both kinds of
// load share one account. The account must shed what its window doesn't let it
// keep (lib/usage/pacing/engine/usage-pace):
//
//     shed = 1 - keep share
//
// The biggest saving goes first, so a session on a dear model - Fable, whose
// step down to Opus saves more than half of a large load - is the first slowed.
// A session fanning work out to subagents multiplies its spend, so its steps
// and its subagents' rank by their saving times its fan-out (1 + the subagents
// it runs): a session running three subagents gives way about four times
// sooner than a lone one spending as much, and a lone session is slowed only
// when the fanned-out ones can't cover the shed.
//
// Every candidate session offers its next step and what that step would save
// (a fraction of its own load). The steps are taken in order of the share of
// the account they save - the biggest saving first - until together they cover
// the shed. A session takes at most one step per round: the next round measures
// again before asking for more.

import { effortStep } from "@/lib/model/model-label";
import type { ModelChoice } from "@/lib/types/agent";
import { priceRatio } from "@/lib/usage/pacing/engine/token-price";

/** An effort ladder spans these weights, lowest level to highest - more effort
 *  means more thinking tokens on the same model. */
const LOWEST_EFFORT_WEIGHT = 0.5;
const HIGHEST_EFFORT_WEIGHT = 1.5;
/** A model with no effort, or a level no ladder knows, sits at the middle. */
const MIDDLE_EFFORT_WEIGHT = 1;

/** What a choice costs per working minute, relative to the others. */
export function costWeight({
  model,
  effort,
  ladder = []
}: ModelChoice & {
  ladder?: readonly string[];
}) {
  const tierWeight = priceRatio(model ?? "");
  const place =
    effort === undefined
      ? null
      : effortStep({
          model: model ?? "",
          effort,
          ladder
        });
  if (place === null) {
    return tierWeight * MIDDLE_EFFORT_WEIGHT;
  }

  const height = place.steps > 1 ? (place.step - 1) / (place.steps - 1) : 1;
  return tierWeight * (LOWEST_EFFORT_WEIGHT + height * (HIGHEST_EFFORT_WEIGHT - LOWEST_EFFORT_WEIGHT));
}

/** A session's estimated share-driver of its account's spend. */
export function sessionLoad({
  cost,
  workingShare,
  subagents
}: {
  cost: number;
  /** How much of the recent time it spent working, 0 to 1. */
  workingShare: number;
  /** The subagents it runs right now, each spending on the same account. */
  subagents: number;
}) {
  return cost * workingShare * (1 + subagents);
}

/** How much measured load one unit of estimated load stands for, over the
 *  sessions both measured and estimated - so a session nothing measured yet
 *  joins the measured ones on their scale. 1 while nothing is measured. */
export function measuredPerEstimated(
  sessions: readonly {
    measured: number;
    estimated: number;
  }[]
) {
  let measured = 0;
  let estimated = 0;
  for (const session of sessions) {
    measured += session.measured;
    estimated += session.estimated;
  }

  const isComparable = measured > 0 && estimated > 0;
  return isComparable ? measured / estimated : 1;
}

/** The fraction of its spend an account must shed to keep the share its window
 *  allows (lib/usage/pacing/engine/usage-pace) - null when it is on pace. */
export function shedFraction(keep: number) {
  const fraction = 1 - keep;
  return fraction > 0 ? Math.min(1, fraction) : null;
}

/** One session that could take a step, and what that step would save. */
export interface ShedCandidate {
  id: string;
  load: number;
  /** The fraction of its own load its next step saves, 0 to 1. */
  saving: number;
  /** Its session's fan-out: 1 plus the subagents that session runs. */
  fanOut: number;
}

/** Which candidates step this round: the biggest share of the account saved,
 *  weighted by fan-out, first, until the savings cover `fraction` of
 *  `totalLoad`. With the shed unknown (`fraction` null - no burn measured yet)
 *  only the first ranked steps, the gentlest move that still acts. */
export function planShedding({
  candidates,
  totalLoad,
  fraction
}: {
  candidates: readonly ShedCandidate[];
  /** Every working session's load on the account, the candidates' included. */
  totalLoad: number;
  fraction: number | null;
}) {
  const ranked = candidates
    .filter(candidate => candidate.saving > 0)
    .map(candidate => ({
      id: candidate.id,
      saved: candidate.load * candidate.saving,
      priority: candidate.load * candidate.saving * candidate.fanOut
    }))
    .toSorted((first, second) => second.priority - first.priority);
  if (fraction === null) {
    return ranked.slice(0, 1).map(candidate => candidate.id);
  }

  const needed = fraction * totalLoad;
  const chosen: string[] = [];
  let saved = 0;
  for (const candidate of ranked) {
    if (saved >= needed) {
      break;
    }

    chosen.push(candidate.id);
    saved += candidate.saved;
  }

  return chosen;
}

/** The stepped-down session to step back up first: the one adding the least
 *  load back, so the account climbs back as gently as it came down. */
export function lightestToRestore(candidates: readonly Pick<ShedCandidate, "id" | "load">[]) {
  let lightest: Pick<ShedCandidate, "id" | "load"> | undefined;
  for (const candidate of candidates) {
    const isLighterCandidate = lightest === undefined || candidate.load < lightest.load;
    if (isLighterCandidate) {
      lightest = candidate;
    }
  }

  return lightest?.id ?? null;
}
