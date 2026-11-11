import { isStoppingMidTurn, pacingNote } from "@/lib/stores/usage/usagePacing.svelte";
import { UsageWindowKind } from "@/lib/types";
import { describe, expect, it } from "vitest";

const NOW = new Date(2026, 9, 15, 12, 0, 0, 0).getTime();
const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;

describe("pacingNote", () => {
  const paused = {
    phase: "paused",
    windowKind: UsageWindowKind.enum.session,
    utilization: 72,
    pausedAt: NOW - 20 * MINUTE_MS,
    readyAt: NOW + HOUR_MS,
    restUntil: null,
    restMs: null
  } as const;
  const clock = new Intl.DateTimeFormat(undefined, { timeStyle: "short" });

  it("says when the agent was paused and that it continues on its own once there is room", () => {
    const note = pacingNote({
      agent: "Claude",
      state: paused,
      now: NOW
    });
    expect(note).toContain(`paused at ${clock.format(paused.pausedAt)}`);
    expect(note).toContain(`continues on its own around ${clock.format(paused.readyAt)}`);
  });

  it("says how long a slowed agent rests", () => {
    const restUntil = NOW + 15 * MINUTE_MS;
    expect(
      pacingNote({
        agent: "Claude",
        state: {
          ...paused,
          restUntil
        },
        now: NOW
      })
    ).toBe(`Claude is resting until ${clock.format(restUntil)} to slow its session usage (72%)`);
  });

  it("names only the earliest rest end while the agent still finishes its step, which can outlast the rest", () => {
    expect(
      pacingNote({
        agent: "Claude",
        state: {
          ...paused,
          phase: "pausing",
          restUntil: NOW - 5 * MINUTE_MS,
          restMs: 15 * MINUTE_MS
        },
        now: NOW
      })
    ).toBe(`Claude is finishing its step, then rests for 15 minutes (until ${clock.format(NOW + 15 * MINUTE_MS)} at the earliest) to slow its session usage (72%)`);
  });
});

describe("isStoppingMidTurn", () => {
  it("holds the stop while the session is still finishing, mid-turn", () => {
    expect(isStoppingMidTurn({ phase: "pausing" })).toBe(true);
  });

  it("lets go once it has stopped, once the user took it over, and for an unpaced session", () => {
    expect(isStoppingMidTurn({ phase: "paused" })).toBe(false);
    expect(isStoppingMidTurn({ phase: "overridden" })).toBe(false);
    expect(isStoppingMidTurn(undefined)).toBe(false);
  });
});
