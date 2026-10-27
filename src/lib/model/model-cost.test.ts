import { cheaperModelChoice, costTier, lowerEffortChoice } from "@/lib/model/model-cost";
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

describe("lowerEffortChoice", () => {
  it("keeps the model and turns its effort down a level", () => {
    expect(
      lowerEffortChoice({
        model: "claude-opus-5-5",
        effort: "high",
        options: CLAUDE_OPTIONS
      })
    ).toEqual({
      model: "claude-opus-5-5",
      effort: "medium"
    });
  });

  it("has nothing lower at the bottom of the ladder, or without an effort", () => {
    expect(
      lowerEffortChoice({
        model: "claude-opus-5-5",
        effort: "low",
        options: CLAUDE_OPTIONS
      })
    ).toBeNull();
    expect(
      lowerEffortChoice({
        model: "claude-haiku-4-5-20251001",
        effort: null,
        options: CLAUDE_OPTIONS
      })
    ).toBeNull();
  });
});

describe("cheaperModelChoice", () => {
  it("moves to the dearest cheaper model, at its recommended level", () => {
    expect(
      cheaperModelChoice({
        model: "claude-opus-5-5",
        options: CLAUDE_OPTIONS
      })
    ).toEqual({
      model: "claude-sonnet-5-5",
      effort: "medium"
    });
  });

  it("never steps below Sonnet, the cheapest tier trusted with code", () => {
    expect(
      cheaperModelChoice({
        model: "claude-sonnet-5-5",
        options: CLAUDE_OPTIONS
      })
    ).toBeNull();
  });

  it("has nothing cheaper below the cheapest tier, or for a model in no known tier", () => {
    expect(
      cheaperModelChoice({
        model: "claude-haiku-4-5-20251001",
        options: CLAUDE_OPTIONS
      })
    ).toBeNull();
    expect(
      cheaperModelChoice({
        model: "gpt-5.6-sol",
        options: [option({
          id: "gpt-5.6-sol",
          efforts: ["minimal", "low"]
        })]
      })
    ).toBeNull();
  });
});
