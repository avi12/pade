import type { ModelTokenUsage } from "@/lib/types";
import { priceRatio, weightedTokens } from "@/lib/usage/pacing/token-price";
import { describe, expect, it } from "vitest";

function usage(counts: Partial<ModelTokenUsage> & Pick<ModelTokenUsage, "model">) {
  return {
    inputTokens: 0,
    outputTokens: 0,
    cacheReadTokens: 0,
    cacheWriteFiveMinuteTokens: 0,
    cacheWriteOneHourTokens: 0,
    ...counts
  } satisfies ModelTokenUsage;
}

describe("priceRatio", () => {
  it("prices each tier by its long-context list price, Haiku first", () => {
    expect(priceRatio("claude-haiku-5-5")).toBe(1);
    expect(priceRatio("claude-sonnet-5-5")).toBe(4);
    expect(priceRatio("claude-opus-5-5[1m]")).toBe(8);
    expect(priceRatio("claude-fable-5-1")).toBe(20);
  });

  it("prices a model no tier places as Sonnet", () => {
    expect(priceRatio("mystery-model")).toBe(priceRatio("claude-sonnet-5-5"));
  });
});

describe("weightedTokens", () => {
  it("weighs each token type by its price against an input token", () => {
    expect(
      weightedTokens([
        usage({
          model: "claude-haiku-5-5",
          inputTokens: 100,
          outputTokens: 100,
          cacheReadTokens: 100,
          cacheWriteFiveMinuteTokens: 100,
          cacheWriteOneHourTokens: 100
        })
      ])
    ).toBeCloseTo(100 + 500 + 10 + 125 + 200);
  });

  it("weighs the same tokens on a dearer model by its price, and sums the models", () => {
    const tokens = {
      outputTokens: 1000
    };
    expect(
      weightedTokens([
        usage({
          model: "claude-opus-5-5",
          ...tokens
        }),
        usage({
          model: "claude-haiku-5-5",
          ...tokens
        })
      ])
    ).toBe(5000 * 8 + 5000);
  });

  it("is nothing for nothing measured", () => {
    expect(weightedTokens([])).toBe(0);
  });
});
