import { cheaperChoice, costTier } from "@/lib/model/model-cost";
import type { ModelOption } from "@/lib/types";
import { describe, expect, it } from "vitest";

function option({ id, efforts, recommendedEffort = null }: {
  id: string;
  efforts: string[];
  recommendedEffort?: string | null;
}) {
  return {
    id,
    name: id,
    efforts: efforts.map(level => ({
      id: level,
      name: level
    })),
    recommendedEffort
  } satisfies ModelOption;
}

const CLAUDE_OPTIONS = [
  option({
    id: "claude-opus-5-5",
    efforts: ["low", "medium", "high"]
  }),
  option({
    id: "claude-sonnet-5-5",
    efforts: ["low", "medium", "high"],
    recommendedEffort: "medium"
  }),
  option({
    id: "claude-haiku-4-5-20251001",
    efforts: []
  })
];

describe("costTier", () => {
  it("reads the tier from the family word", () => {
    expect(costTier("claude-haiku-4-5-20251001")).toBeLessThan(costTier("claude-sonnet-5-5"));
    expect(costTier("claude-sonnet-5-5")).toBeLessThan(costTier("claude-opus-5-5[1m]"));
    expect(costTier("openai/gpt-5.6-mini")).toBe(costTier("claude-haiku-4-5-20251001"));
    expect(costTier("gpt-5.6-sol")).toBe(-1);
  });
});

describe("cheaperChoice", () => {
  it("turns the effort down before anything else", () => {
    expect(
      cheaperChoice({
        model: "claude-opus-5-5",
        effort: "high",
        options: CLAUDE_OPTIONS
      })
    ).toEqual({
      model: "claude-opus-5-5",
      effort: "medium"
    });
  });

  it("moves to the next cheaper model, at its recommended level, at the bottom of the ladder", () => {
    expect(
      cheaperChoice({
        model: "claude-opus-5-5",
        effort: "low",
        options: CLAUDE_OPTIONS
      })
    ).toEqual({
      model: "claude-sonnet-5-5",
      effort: "medium"
    });
  });

  it("has nothing cheaper below the cheapest tier, or for a model in no known tier", () => {
    expect(
      cheaperChoice({
        model: "claude-haiku-4-5-20251001",
        effort: null,
        options: CLAUDE_OPTIONS
      })
    ).toBeNull();
    expect(
      cheaperChoice({
        model: "gpt-5.6-sol",
        effort: "minimal",
        options: [option({
          id: "gpt-5.6-sol",
          efforts: ["minimal", "low"]
        })]
      })
    ).toBeNull();
  });
});
