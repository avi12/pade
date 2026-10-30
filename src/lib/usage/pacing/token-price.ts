// What tokens cost, relative to each other - the one price table usage pacing
// reads, for a session's measured burn and for its estimated one alike. A
// plan's limit meters spend, and spend follows list price: an output token
// costs five input tokens, a cache read a tenth of one, and the same token
// costs more on a dearer model. So each count is weighted by its type's price
// relative to an input token, and each model by its input price relative to the
// cheapest tier's - a weighted total reads in cheapest-tier input tokens.
//
// Anthropic's list prices (late 2026), per million input tokens, with the model
// placed by the family word in its id (`costTier`). Agent sessions run long
// contexts, so Haiku is priced at its rate above 100k-token prompts.

import { costTier, NO_TIER } from "@/lib/model/model-cost";
import type { CostTiers } from "@/lib/model/model-cost";
import type { ModelTokenUsage } from "@/lib/types";

/** One number per cost tier, in the tiers' own order. */
type PerTier<Tiers extends readonly unknown[]> = { readonly [Tier in keyof Tiers]: number };

/** Dollars per million input tokens by cost tier: Haiku, Sonnet, Opus, Fable. */
const TIER_INPUT_PRICES: PerTier<CostTiers> = [0.5, 2, 4, 10];
const CHEAPEST_TIER_PRICE = TIER_INPUT_PRICES[0];
/** A model no tier places is priced as Sonnet, the middle of the range. */
const UNKNOWN_MODEL_PRICE = TIER_INPUT_PRICES[1];

/** Each token type's price relative to an input token on the same model. */
const TOKEN_TYPE_WEIGHTS = {
  inputTokens: 1,
  outputTokens: 5,
  cacheReadTokens: 0.1,
  cacheWriteFiveMinuteTokens: 1.25,
  cacheWriteOneHourTokens: 2
} as const satisfies Record<Exclude<keyof ModelTokenUsage, "model">, number>;

/** How much a model's tokens cost relative to the cheapest tier's. */
export function priceRatio(model: string) {
  const tier = costTier(model);
  const price = tier === NO_TIER ? UNKNOWN_MODEL_PRICE : TIER_INPUT_PRICES[tier];
  return price / CHEAPEST_TIER_PRICE;
}

/** What a set of per-model token counts costs, in cheapest-tier input tokens. */
export function weightedTokens(usage: readonly ModelTokenUsage[]) {
  let total = 0;
  for (const entry of usage) {
    const inputEquivalent = entry.inputTokens * TOKEN_TYPE_WEIGHTS.inputTokens
      + entry.outputTokens * TOKEN_TYPE_WEIGHTS.outputTokens
      + entry.cacheReadTokens * TOKEN_TYPE_WEIGHTS.cacheReadTokens
      + entry.cacheWriteFiveMinuteTokens * TOKEN_TYPE_WEIGHTS.cacheWriteFiveMinuteTokens
      + entry.cacheWriteOneHourTokens * TOKEN_TYPE_WEIGHTS.cacheWriteOneHourTokens;
    total += inputEquivalent * priceRatio(entry.model);
  }

  return total;
}
