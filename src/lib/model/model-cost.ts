// Which way is cheaper: one step down from a model and effort level, for usage
// pacing to spend a window more slowly. Effort comes first - the same model,
// thinking less - and only at the bottom of its ladder does a cheaper model
// take over. No catalog states a price, so a model's cost tier is read from the
// family word in its id, the one thing every vendor keeps stable: Haiku and the
// mini/nano/flash/lite lines are the cheap tier, Sonnet the middle, Opus the top.
// A model in no known tier is never swapped, only turned down.

import { bareModel, lowerEffort } from "@/lib/model/model-label";
import type { ModelChoice, ModelOption } from "@/lib/types";

/** Family words by cost tier, cheapest tier first. */
const COST_TIERS = [
  ["haiku", "mini", "nano", "flash", "lite"],
  ["sonnet"],
  ["opus"]
] as const;
const NO_TIER = -1;

/** A model's cost tier - its index in COST_TIERS - or -1 when no family word
 *  places it. */
export function costTier(model: string) {
  const id = bareModel(model).toLowerCase();
  return COST_TIERS.findIndex(words => words.some(word => id.includes(word)));
}

/** The dearest catalog model in a tier below `model`'s - the gentlest cheaper
 *  step - or undefined when there is none. */
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
    const isCheaper = optionTier !== NO_TIER && optionTier < tier;
    if (isCheaper && optionTier > bestTier) {
      best = option;
      bestTier = optionTier;
    }
  }

  return best;
}

/** One step cheaper than running `model` at `effort`: the next effort level
 *  down its own ladder, else a cheaper model at its recommended level. Null
 *  when nothing cheaper is known. */
export function cheaperChoice({ model, effort, options }: {
  model: string;
  effort: string | null;
  options: readonly ModelOption[];
}) {
  const bare = bareModel(model);
  const ladder = options.find(option => bareModel(option.id) === bare)?.efforts.map(level => level.id) ?? [];
  const lower = effort === null ? null : lowerEffort({
    model,
    effort,
    ladder
  });
  if (lower !== null) {
    return {
      model,
      effort: lower
    } satisfies ModelChoice;
  }

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
