import { pacingNote } from "@/lib/stores/usage/usagePacing.svelte";
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
    continueAt: NOW + 2 * HOUR_MS,
    restUntil: null,
    restMs: null
  } as const;
  const clock = new Intl.DateTimeFormat(undefined, { timeStyle: "short" });

  it("says when the agent was paused and that it continues just before the reset", () => {
    const note = pacingNote({
      agent: "Claude",
      state: paused,
      isContinuingWhenReady: false,
      now: NOW
    });
    expect(note).toContain(`paused at ${clock.format(paused.pausedAt)}`);
    expect(note).toContain(`continues on its own around ${clock.format(paused.continueAt)}`);
  });

  it("names the earlier time once the session continues as soon as it has room", () => {
    expect(
      pacingNote({
        agent: "Claude",
        state: paused,
        isContinuingWhenReady: true,
        now: NOW
      })
    ).toContain(`continues on its own around ${clock.format(paused.readyAt)}`);
  });

  it("tells a ready session apart from one still waiting for room", () => {
    expect(
      pacingNote({
        agent: "Claude",
        state: {
          ...paused,
          phase: "ready"
        },
        isContinuingWhenReady: false,
        now: NOW
      })
    ).toContain("has room in its session usage (72%) again");
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
        isContinuingWhenReady: true,
        now: NOW
      })
    ).toBe(`Claude is resting until ${clock.format(restUntil)} to slow its session usage (72%)`);
  });

  it("names no rest end while the agent still finishes its step, which can outlast the rest", () => {
    expect(
      pacingNote({
        agent: "Claude",
        state: {
          ...paused,
          phase: "pausing",
          restUntil: NOW - 5 * MINUTE_MS,
          restMs: 15 * MINUTE_MS
        },
        isContinuingWhenReady: true,
        now: NOW
      })
    ).toBe("Claude is finishing its step, then rests for 15 minutes to slow its session usage (72%)");
  });
});
