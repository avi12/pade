import { modelLabel, modelName } from "@/lib/model-label";
import { describe, expect, it } from "vitest";

describe("modelName", () => {
  it("reads a Claude id as family and dotted version", () => {
    expect(modelName("claude-opus-5-5")).toBe("Opus 5.5");
    expect(modelName("claude-fable-5-1")).toBe("Fable 5.1");
  });

  it("drops the release date", () => {
    expect(modelName("claude-haiku-4-5-20251001")).toBe("Haiku 4.5");
  });

  it("shows any other id exactly as logged", () => {
    expect(modelName("gpt-5.6-sol")).toBe("gpt-5.6-sol");
    expect(modelName("claude-opus-5-5-preview")).toBe("claude-opus-5-5-preview");
  });
});

describe("modelLabel", () => {
  it("adds the effort when the model has one", () => {
    expect(
      modelLabel({
        model: "claude-opus-5-5",
        effort: "high"
      })
    ).toBe("Opus 5.5 · high");
  });

  it("is just the name without an effort", () => {
    expect(
      modelLabel({
        model: "claude-opus-5-5",
        effort: null
      })
    ).toBe("Opus 5.5");
  });
});
