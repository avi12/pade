import { effortHeat, effortStep, modelLabel, modelName } from "@/lib/model-label";
import { describe, expect, it } from "vitest";

describe("modelName", () => {
  it("reads a Claude id as family and dotted version", () => {
    expect(modelName("claude-opus-5-5")).toBe("Opus 5.5");
    expect(modelName("claude-fable-5-1")).toBe("Fable 5.1");
  });

  it("drops the release date", () => {
    expect(modelName("claude-haiku-4-5-20251001")).toBe("Haiku 4.5");
  });

  it("drops a provider prefix", () => {
    expect(modelName("anthropic/claude-opus-5")).toBe("Opus 5");
    expect(modelName("openai/gpt-5.6-sol")).toBe("gpt-5.6-sol");
  });

  it("shows any other id exactly as reported", () => {
    expect(modelName("gpt-5.6-sol")).toBe("gpt-5.6-sol");
    expect(modelName("claude-opus-5-5-preview")).toBe("claude-opus-5-5-preview");
  });
});

describe("effortStep", () => {
  it("places a Claude level on Claude's ladder", () => {
    expect(
      effortStep({
        model: "claude-opus-5",
        effort: "high"
      })
    ).toEqual({
      step: 3,
      steps: 5
    });
    expect(
      effortStep({
        model: "claude-opus-5",
        effort: "max"
      })
    ).toEqual({
      step: 5,
      steps: 5
    });
  });

  it("places an OpenAI level on the OpenAI ladder", () => {
    expect(
      effortStep({
        model: "gpt-5.6-sol",
        effort: "minimal"
      })
    ).toEqual({
      step: 1,
      steps: 5
    });
    expect(
      effortStep({
        model: "openai/gpt-5.6-sol",
        effort: "xhigh"
      })
    ).toEqual({
      step: 5,
      steps: 5
    });
  });

  it("falls back to the other ladder for a level only it has", () => {
    expect(
      effortStep({
        model: "gpt-5.6-sol",
        effort: "max"
      })
    ).toEqual({
      step: 5,
      steps: 5
    });
  });

  it("is null for a level no ladder knows", () => {
    expect(
      effortStep({
        model: "claude-opus-5",
        effort: "balanced"
      })
    ).toBeNull();
  });

  it("climbs the model's own catalog ladder when it has one", () => {
    expect(
      effortStep({
        model: "gpt-5.6-terra",
        effort: "max",
        ladder: ["low", "medium", "high", "xhigh", "max", "ultra"]
      })
    ).toEqual({
      step: 5,
      steps: 6
    });
  });
});

describe("effortHeat", () => {
  it("runs from cool at the lowest level to hot at the highest", () => {
    expect(
      effortHeat({
        step: 1,
        steps: 5
      })
    ).toBe(0);
    expect(
      effortHeat({
        step: 3,
        steps: 5
      })
    ).toBe(0.5);
    expect(
      effortHeat({
        step: 5,
        steps: 5
      })
    ).toBe(1);
  });

  it("draws a one-level ladder's only level hot", () => {
    expect(
      effortHeat({
        step: 1,
        steps: 1
      })
    ).toBe(1);
  });
});

describe("modelLabel", () => {
  it("names the effort when the model has one", () => {
    expect(
      modelLabel({
        name: "Opus 5.5",
        effort: "high"
      })
    ).toBe("Opus 5.5 · high effort");
  });

  it("is just the name without an effort", () => {
    expect(
      modelLabel({
        name: "Opus 5.5",
        effort: null
      })
    ).toBe("Opus 5.5");
  });
});
