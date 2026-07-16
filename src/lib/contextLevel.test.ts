import { CONTEXT_HANDOFF_PCT, ContextLevel, contextLevel } from "@/lib/contextLevel";
import { describe, expect, it } from "vitest";

describe("contextLevel", () => {
  it("stays ok well below the handoff threshold", () => {
    expect(contextLevel(0)).toBe(ContextLevel.ok);
    expect(contextLevel(53)).toBe(ContextLevel.ok); // 53/90 = 0.588 < 0.6
  });

  it("warns from 60% of the way to the handoff (54% of context)", () => {
    expect(contextLevel(54)).toBe(ContextLevel.warning); // 54/90 = 0.6
    expect(contextLevel(80)).toBe(ContextLevel.warning); // 80/90 = 0.889 < 0.9
  });

  it("turns critical from 90% of the way to the handoff (81% of context)", () => {
    expect(contextLevel(81)).toBe(ContextLevel.critical); // 81/90 = 0.9
    expect(contextLevel(100)).toBe(ContextLevel.critical); // clamped past the ceiling
  });

  it("pins the handoff threshold at 90% of context", () => {
    expect(CONTEXT_HANDOFF_PCT).toBe(90);
  });
});
