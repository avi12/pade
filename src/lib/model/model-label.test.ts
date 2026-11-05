import {
  bareModel,
  effortBars,
  effortStep,
  effortTone,
  lowerEffort,
  modelLabel,
  modelName,
  shortModelName
} from "@/lib/model/model-label";
import { describe, expect, it } from "vitest";

describe("lowerEffort", () => {
  it("steps one level down the model's own ladder", () => {
    expect(
      lowerEffort({
        model: "claude-opus-5-5",
        effort: "high",
        ladder: ["low", "medium", "high"]
      })
    ).toBe("medium");
  });

  it("falls back to the family's ladder", () => {
    expect(
      lowerEffort({
        model: "gpt-5.6-sol",
        effort: "low"
      })
    ).toBe("minimal");
  });

  it("has nowhere lower to go at the bottom or for an unknown level", () => {
    expect(
      lowerEffort({
        model: "claude-opus-5-5",
        effort: "low"
      })
    ).toBeNull();
    expect(
      lowerEffort({
        model: "claude-opus-5-5",
        effort: "turbo"
      })
    ).toBeNull();
  });
});

describe("bareModel", () => {
  it("drops the provider prefix and the context variant", () => {
    expect(bareModel("anthropic/claude-opus-5")).toBe("claude-opus-5");
    expect(bareModel("claude-opus-5-5[1m]")).toBe("claude-opus-5-5");
  });
});

describe("modelName", () => {
  it("reads a Claude id as family and dotted version", () => {
    expect(modelName("claude-opus-5-5")).toBe("Opus 5.5");
    expect(modelName("claude-fable-5-1")).toBe("Fable 5.1");
  });

  it("names the context variant", () => {
    expect(modelName("claude-opus-5-5[1m]")).toBe("Opus 5.5 1M");
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

describe("shortModelName", () => {
  it("reads a Claude id as its family alone", () => {
    expect(shortModelName("claude-opus-5-5")).toBe("Opus");
    expect(shortModelName("anthropic/claude-sonnet-5-5[1m]")).toBe("Sonnet");
    expect(shortModelName("claude-haiku-4-5-20251001")).toBe("Haiku");
  });

  it("keeps an OpenAI tier word lowercase, as logged", () => {
    expect(shortModelName("gpt-5.6-codex")).toBe("codex");
    expect(shortModelName("openai/gpt-5.6-codex-mini")).toBe("mini");
  });

  it("capitalizes another vendor's tier word", () => {
    expect(shortModelName("gemini-2.5-pro")).toBe("Pro");
    expect(shortModelName("gemini-2.5-flash")).toBe("Flash");
  });

  it("capitalizes a lone word", () => {
    expect(shortModelName("auto")).toBe("Auto");
  });

  it("shows an id with no tier word as logged", () => {
    expect(shortModelName("gpt-5.6")).toBe("gpt-5.6");
    expect(shortModelName("composer-1")).toBe("composer-1");
  });
});

describe("effortBars", () => {
  it("lights the bars up to the level in use, climbing in height", () => {
    expect(
      effortBars({
        model: "claude-opus-5-5",
        effort: "xhigh"
      })
    ).toEqual([
      {
        level: "low",
        heightPercent: 35,
        isLit: true
      },
      {
        level: "medium",
        heightPercent: 51,
        isLit: true
      },
      {
        level: "high",
        heightPercent: 68,
        isLit: true
      },
      {
        level: "xhigh",
        heightPercent: 84,
        isLit: true
      },
      {
        level: "max",
        heightPercent: 100,
        isLit: false
      }
    ]);
  });

  it("draws one bar per level of the model's own ladder", () => {
    const bars = effortBars({
      model: "gpt-5.6-terra",
      effort: "low",
      ladder: ["low", "high", "ultra"]
    });
    expect(bars.map(bar => bar.heightPercent)).toEqual([35, 68, 100]);
    expect(bars.map(bar => bar.isLit)).toEqual([true, false, false]);
  });

  it("draws a one-level ladder as one full, lit bar", () => {
    expect(
      effortBars({
        model: "gpt-5.6-terra",
        effort: "auto",
        ladder: ["auto"]
      })
    ).toEqual([
      {
        level: "auto",
        heightPercent: 100,
        isLit: true
      }
    ]);
  });

  it("is empty for a level no ladder knows", () => {
    expect(
      effortBars({
        model: "claude-opus-5-5",
        effort: "turbo"
      })
    ).toEqual([]);
  });
});

describe("effortTone", () => {
  it("gives each level of a five-level ladder its own tone", () => {
    const tones = [1, 2, 3, 4, 5].map(step => effortTone({
      step,
      steps: 5
    }));
    expect(tones).toEqual([1, 2, 3, 4, 5]);
  });

  it("spans the coolest to the hottest tone on a shorter ladder", () => {
    const tones = [1, 2, 3, 4].map(step => effortTone({
      step,
      steps: 4
    }));
    expect(tones).toEqual([1, 2, 4, 5]);
  });

  it("draws a one-level ladder's only level hottest", () => {
    expect(
      effortTone({
        step: 1,
        steps: 1
      })
    ).toBe(5);
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
