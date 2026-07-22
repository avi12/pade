import { ContextLevel, contextLevel, DEFAULT_CONTEXT_HANDOFF_PCT } from "@/lib/context-level";
import { describe, expect, it } from "vitest";

// Severity is relative to whatever threshold the user configured, so the tests
// exercise the ramp at a mid-range threshold (90) and at the low default.
describe("contextLevel", () => {
  it("stays ok well below the handoff threshold", () => {
    expect(
      contextLevel({
        pct: 0,
        threshold: 90
      })
    ).toBe(ContextLevel.ok);
    expect(
      contextLevel({
        pct: 53,
        threshold: 90
      })
    ).toBe(ContextLevel.ok); // 53/90 = 0.588 < 0.6
  });

  it("warns from 60% of the way to the handoff", () => {
    expect(
      contextLevel({
        pct: 54,
        threshold: 90
      })
    ).toBe(ContextLevel.warning); // 54/90 = 0.6
    expect(
      contextLevel({
        pct: 80,
        threshold: 90
      })
    ).toBe(ContextLevel.warning); // 80/90 = 0.889 < 0.9
  });

  it("turns critical from 90% of the way to the handoff", () => {
    expect(
      contextLevel({
        pct: 81,
        threshold: 90
      })
    ).toBe(ContextLevel.critical); // 81/90 = 0.9
    expect(
      contextLevel({
        pct: 100,
        threshold: 90
      })
    ).toBe(ContextLevel.critical); // clamped past the ceiling
  });

  it("scales the whole ramp down with a low threshold", () => {
    expect(
      contextLevel({
        pct: 17,
        threshold: 30
      })
    ).toBe(ContextLevel.ok); // 17/30 = 0.567
    expect(
      contextLevel({
        pct: 18,
        threshold: 30
      })
    ).toBe(ContextLevel.warning); // 18/30 = 0.6
    expect(
      contextLevel({
        pct: 27,
        threshold: 30
      })
    ).toBe(ContextLevel.critical); // 27/30 = 0.9
  });

  it("defaults the handoff threshold to 30% of context", () => {
    expect(DEFAULT_CONTEXT_HANDOFF_PCT).toBe(30);
  });
});
