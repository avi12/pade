import { ContextLevel, contextLevel, DEFAULT_CONTEXT_HANDOFF_PERCENTAGE } from "@/lib/usage/context-level";
import { describe, expect, it } from "vitest";

// Severity is relative to whatever threshold the user configured, so the tests
// exercise the ramp at a mid-range threshold (90) and at the low default.
describe("contextLevel", () => {
  it("stays ok well below the handoff threshold", () => {
    expect(
      contextLevel({
        percentage: 0,
        threshold: 90
      })
    ).toBe(ContextLevel.Ok);
    expect(
      contextLevel({
        percentage: 44,
        threshold: 90
      })
    ).toBe(ContextLevel.Ok); // 44/90 = 0.489 < 0.5
  });

  it("warns from 50% of the way to the handoff", () => {
    expect(
      contextLevel({
        percentage: 45,
        threshold: 90
      })
    ).toBe(ContextLevel.Warning); // 45/90 = 0.5
    expect(
      contextLevel({
        percentage: 67,
        threshold: 90
      })
    ).toBe(ContextLevel.Warning); // 67/90 = 0.744 < 0.75
  });

  it("turns critical from 75% of the way to the handoff", () => {
    expect(
      contextLevel({
        percentage: 68,
        threshold: 90
      })
    ).toBe(ContextLevel.Critical); // 68/90 = 0.756
    expect(
      contextLevel({
        percentage: 100,
        threshold: 90
      })
    ).toBe(ContextLevel.Critical); // clamped past the ceiling
  });

  it("scales the whole ramp down with a low threshold", () => {
    expect(
      contextLevel({
        percentage: 14,
        threshold: 30
      })
    ).toBe(ContextLevel.Ok); // 14/30 = 0.467
    expect(
      contextLevel({
        percentage: 15,
        threshold: 30
      })
    ).toBe(ContextLevel.Warning); // 15/30 = 0.5
    expect(
      contextLevel({
        percentage: 23,
        threshold: 30
      })
    ).toBe(ContextLevel.Critical); // 23/30 = 0.767
  });

  it("reads a 31% fill as critical against a 40% handoff", () => {
    expect(
      contextLevel({
        percentage: 31,
        threshold: 40
      })
    ).toBe(ContextLevel.Critical); // 31/40 = 0.775
  });

  it("defaults the handoff threshold to 30% of context", () => {
    expect(DEFAULT_CONTEXT_HANDOFF_PERCENTAGE).toBe(30);
  });
});
