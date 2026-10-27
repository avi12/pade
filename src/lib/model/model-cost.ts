// Which way is cheaper, for usage pacing to spend a window more slowly: the same
// model thinking less (one effort level down), or a cheaper model. No catalog
// states a price, so a model's cost tier is read from the
// family word in its id, the one thing every vendor keeps stable: Haiku and the
// mini/nano/flash/lite lines are the cheap tier, Sonnet the middle, Opus the top.
// A model in no known tier is never swapped, only turned down. Pacing can't tell
// a coding turn from a docs one, so it never swaps below the coding floor.

import { bareModel, lowerEffort } from "@/lib/model/model-label";
import type { ModelChoice, ModelOption } from "@/lib/types";

/** Family words by cost tier, cheapest tier first. */
const COST_TIERS = [
  ["haiku", "mini", "nano", "flash", "lite"],
  ["sonnet"],
  ["opus"]
] as const;
const NO_TIER = -1;
/** Sonnet's tier: the cheapest one trusted to write code. */
const CODING_FLOOR_TIER = 1;

/** A model's cost tier - its index in COST_TIERS - or -1 when no family word
 *  places it. */
export function costTier(model: string) {
  const id = bareModel(model).toLowerCase();
  return COST_TIERS.findIndex(words => words.some(word => id.includes(word)));
}

/** The dearest catalog model in a tier below `model`'s but not below the
 *  coding floor - the gentlest cheaper step - or undefined when there is none. */
function cheaperModel({ model, options }: {
  model: string;
  options: readonly ModelOption[];
}) {
  const tier = costTier(model);
  if (tier === NO_TIER) {
    return undefined;
  }

  let best: ModelOption | undefined;
  let bestTier = NO_TIER;
  for (const option of options) {
    const optionTier = costTier(option.id);
    const isCheaper = optionTier >= CODING_FLOOR_TIER && optionTier < tier;
    if (isCheaper && optionTier > bestTier) {
      best = option;
      bestTier = optionTier;
    }
  }

  return best;
}

/** The same model one effort level lower, on its own ladder from the catalog -
 *  or null at the bottom, or for a model with no effort. */
export function lowerEffortChoice({ model, effort, options }: {
  model: string;
  effort: string | null;
  options: readonly ModelOption[];
}) {
  if (effort === null) {
    return null;
  }

  const bare = bareModel(model);
  const ladder = options.find(option => bareModel(option.id) === bare)?.efforts.map(level => level.id) ?? [];
  const lower = lowerEffort({
    model,
    effort,
    ladder
  });
  if (lower === null) {
    return null;
  }

  return {
    model,
    effort: lower
  } satisfies ModelChoice;
}

/** What a paced session's subagents may run: `model` when they ask for
 *  nothing it allows, and every catalog model no dearer than it. A model in no
 *  known tier allows only itself. */
export function subagentCap({ model, options }: {
  model: string;
  options: readonly ModelOption[];
}) {
  const tier = costTier(model);
  const cheaperOrEqual = options
    .filter(option => tier !== NO_TIER && costTier(option.id) !== NO_TIER && costTier(option.id) <= tier)
    .map(option => option.id);
  return {
    model,
    allowedModels: [model, ...cheaperOrEqual.filter(id => id !== model)]
  };
}

/** The next cheaper model at its recommended effort level - or null when no
 *  cheaper one is known. */
export function cheaperModelChoice({ model, options }: {
  model: string;
  options: readonly ModelOption[];
}) {
  const cheaper = cheaperModel({
    model,
    options
  });
  if (cheaper === undefined) {
    return null;
  }

  return {
    model: cheaper.id,
    effort: cheaper.recommendedEffort ?? undefined
  } satisfies ModelChoice;
}
