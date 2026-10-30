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
// load share one account. The account must shed the fraction its burn runs over the pace that lasts
// to the reset:
//
//     shed = 1 - target rate / measured rate,  target = left / time to reset
//
// Every candidate session offers its next step and what that step would save
// (a fraction of its own load). The steps are taken in order of the share of
// the account they save - the biggest saving first - until together they cover
// the shed. A session takes at most one step per round: the next round measures
// again before asking for more.

import { effortStep } from "@/lib/model/model-label";
import type { ModelChoice, UsageWindow } from "@/lib/types";
import { priceRatio } from "@/lib/usage/pacing/token-price";

/** An effort ladder spans these weights, lowest level to highest - more effort
 *  means more thinking tokens on the same model. */
const LOWEST_EFFORT_WEIGHT = 0.5;
const HIGHEST_EFFORT_WEIGHT = 1.5;
/** A model with no effort, or a level no ladder knows, sits at the middle. */
const MIDDLE_EFFORT_WEIGHT = 1;
const FULL_PERCENTAGE = 100;

/** What a choice costs per working minute, relative to the others. */
export function costWeight({ model, effort, ladder = [] }: ModelChoice & {
  ladder?: readonly string[];
}) {
  const tierWeight = priceRatio(model ?? "");
  const place = effort === undefined ? null : effortStep({
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
export function sessionLoad({ cost, workingShare, subagents }: {
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
export function measuredPerEstimated(sessions: readonly {
  measured: number;
  estimated: number;
}[]) {
  let measured = 0;
  let estimated = 0;
  for (const session of sessions) {
    measured += session.measured;
    estimated += session.estimated;
  }

  const isComparable = measured > 0 && estimated > 0;
  return isComparable ? measured / estimated : 1;
}

/** The fraction of its spend an account must shed to last until `freeAt` -
 *  null while its burn isn't measured, or when it is already on pace. */
export function shedFraction({ window, rate, freeAt, now }: {
  window: UsageWindow;
  rate: number | null;
  freeAt: number;
  now: number;
}) {
  const timeLeft = freeAt - now;
  if (rate === null || rate <= 0 || timeLeft <= 0) {
    return null;
  }

  const target = (FULL_PERCENTAGE - window.utilization) / timeLeft;
  const fraction = 1 - target / rate;
  return fraction > 0 ? fraction : null;
}

/** One session that could take a step, and what that step would save. */
export interface ShedCandidate {
  id: string;
  load: number;
  /** The fraction of its own load its next step saves, 0 to 1. */
  saving: number;
}

/** Which candidates step this round: biggest share of the account saved first,
 *  until the savings cover `fraction` of `totalLoad`. With the shed unknown
 *  (`fraction` null - no burn measured yet) only the one biggest saving steps,
 *  the gentlest move that still acts. */
export function planShedding({ candidates, totalLoad, fraction }: {
  candidates: readonly ShedCandidate[];
  /** Every working session's load on the account, the candidates' included. */
  totalLoad: number;
  fraction: number | null;
}) {
  const ranked = candidates
    .filter(candidate => candidate.saving > 0)
    .map(candidate => ({
      id: candidate.id,
      saved: candidate.load * candidate.saving
    }))
    .toSorted((first, second) => second.saved - first.saved);
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
    if (lightest === undefined || candidate.load < lightest.load) {
      lightest = candidate;
    }
  }

  return lightest?.id ?? null;
}
