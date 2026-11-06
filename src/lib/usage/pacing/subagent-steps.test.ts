import { SubagentStepKind } from "@/lib/types";
import {
  easedSubagentStep,
  holdForSaving,
  holdSaving,
  isBlockingSpawns,
  MAXIMUM_HOLD_SECONDS,
  MINIMUM_HOLD_SECONDS,
  nextSubagentStep,
  NO_SPAWN_SHED
} from "@/lib/usage/pacing/subagent-steps";
import { describe, expect, it } from "vitest";

const WRAP_UP = {
  step: SubagentStepKind.enum.wrapUp
} as const;

function gate(holdSeconds: number) {
  return {
    step: SubagentStepKind.enum.gate,
    holdSeconds
  } as const;
}

describe("the gate length", () => {
  it("saves the share it was sized for", () => {
    expect(holdSaving(holdForSaving(0.5))).toBeCloseTo(0.5, 1);
    expect(holdSaving(holdForSaving(0.6))).toBeCloseTo(0.6, 1);
  });

  it("grows with the shed and stays within its bounds", () => {
    expect(holdForSaving(0.2)).toBeLessThan(holdForSaving(0.5));
    expect(holdForSaving(0)).toBe(MINIMUM_HOLD_SECONDS);
    expect(holdForSaving(0.99)).toBe(MAXIMUM_HOLD_SECONDS);
    expect(holdForSaving(1)).toBe(MAXIMUM_HOLD_SECONDS);
  });
});

describe("nextSubagentStep", () => {
  it("gates first, sized to the shed, and the shortest hold while the shed is unknown", () => {
    expect(
      nextSubagentStep({
        current: undefined,
        shed: 0.5
      })?.step
    ).toEqual(gate(holdForSaving(0.5)));
    expect(
      nextSubagentStep({
        current: undefined,
        shed: null
      })?.step
    ).toEqual(gate(MINIMUM_HOLD_SECONDS));
  });

  it("doubles the hold up to the longest, saving what the longer hold adds", () => {
    const next = nextSubagentStep({
      current: gate(10),
      shed: 0.4
    });
    expect(next?.step).toEqual(gate(20));
    expect(next?.saving).toBeCloseTo(1 - (20 + 10) / (20 + 20));
    expect(
      nextSubagentStep({
        current: gate(40),
        shed: 0.4
      })?.step
    ).toEqual(gate(MAXIMUM_HOLD_SECONDS));
  });

  it("wraps up at the longest hold, saving all it spends", () => {
    expect(
      nextSubagentStep({
        current: gate(MAXIMUM_HOLD_SECONDS),
        shed: 0.4
      })
    ).toEqual({
      step: WRAP_UP,
      saving: 1
    });
  });

  it("has nothing further once it is wrapping up", () => {
    expect(
      nextSubagentStep({
        current: WRAP_UP,
        shed: 0.9
      })
    ).toBeNull();
  });
});

describe("easedSubagentStep", () => {
  it("halves a hold and lifts it under the shortest", () => {
    expect(easedSubagentStep(gate(40))).toEqual(gate(20));
    expect(easedSubagentStep(gate(MINIMUM_HOLD_SECONDS))).toBeNull();
  });

  it("lets a subagent told to wrap up finish", () => {
    expect(easedSubagentStep(WRAP_UP)).toEqual(WRAP_UP);
  });
});

describe("isBlockingSpawns", () => {
  it("blocks new subagents only past the threshold, never on an unknown shed", () => {
    expect(isBlockingSpawns(NO_SPAWN_SHED)).toBe(true);
    expect(isBlockingSpawns(NO_SPAWN_SHED - 0.01)).toBe(false);
    expect(isBlockingSpawns(null)).toBe(false);
  });
});
