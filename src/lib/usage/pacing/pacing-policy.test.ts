import { CreditsState, PacedWindowKind, UsageWindowKind } from "@/lib/types";
import { isPacingOn, pacingFacts, pacingPreview } from "@/lib/usage/pacing/pacing-policy";
import { paceForecasts, RESET_GRACE_MS } from "@/lib/usage/pacing/usage-pace";
import type { PaceForecast } from "@/lib/usage/pacing/usage-pace";
import { describe, expect, it } from "vitest";

const NOW = new Date(2026, 9, 15, 12, 0, 0, 0).getTime();
const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

function forecast({ utilization, runsOutIn, resetsIn }: {
  utilization: number;
  runsOutIn: number | null;
  resetsIn: number;
}) {
  return {
    window: {
      key: "five_hour",
      kind: UsageWindowKind.enum.session,
      label: "Session",
      utilization,
      resetsAt: new Date(NOW + resetsIn).toISOString()
    },
    resetAt: NOW + resetsIn,
    runOutAt: runsOutIn === null ? null : NOW + runsOutIn,
    isMeasured: true,
    onPaceAt: runsOutIn === null ? null : NOW + resetsIn - RESET_GRACE_MS - runsOutIn,
    evenPercentage: null
  } satisfies PaceForecast;
}

function sessionPreview({ slowedCount = 0, ...shape }: Parameters<typeof forecast>[0] & {
  slowedCount?: number;
}) {
  return pacingPreview({
    kind: PacedWindowKind.enum.session,
    forecast: forecast(shape),
    slowedCount,
    now: NOW
  });
}

describe("isPacingOn", () => {
  it("is on unless turned off", () => {
    expect(isPacingOn({})).toBe(true);
    expect(isPacingOn({ usagePacing: true })).toBe(true);
    expect(isPacingOn({ usagePacing: false })).toBe(false);
  });
});

describe("pacingPreview", () => {
  it("slows the week once it spends well ahead of an even week", () => {
    const resetsIn = 3 * DAY_MS;
    const preview = pacingPreview({
      kind: PacedWindowKind.enum.weekly,
      forecast: paceForecasts({
        account: {
          windows: [{
            key: "seven_day",
            kind: UsageWindowKind.enum.weekly,
            label: "Weekly",
            utilization: 90,
            resetsAt: new Date(NOW + resetsIn).toISOString()
          }],
          plan: "Max",
          source: "test",
          credits: CreditsState.enum.off
        },
        now: NOW,
        rateOf: () => null
      }).weekly ?? null,
      slowedCount: 0,
      now: NOW
    });
    expect(preview?.utilization).toBe(90);
    expect(preview?.action).toBe("Slowing down the next working session");
  });

  it("says when it runs out at this pace, and how many sessions it slows", () => {
    const preview = sessionPreview({
      utilization: 80,
      runsOutIn: HOUR_MS,
      resetsIn: 3 * HOUR_MS,
      slowedCount: 2
    });
    expect(preview?.pace).toMatch(/^At this pace: runs out /v);
    expect(preview?.action).toBe("Slowing 2 sessions so it lasts");
  });

  it("still acts close to the reset, when the built-in slack is longer than the time left", () => {
    expect(
      sessionPreview({
        utilization: 99,
        runsOutIn: 2 * MINUTE_MS,
        resetsIn: 16 * MINUTE_MS
      })?.action
    ).toBe("Slowing down the next working session");
  });

  it("leaves a pace inside the slack alone", () => {
    const preview = sessionPreview({
      utilization: 80,
      runsOutIn: 3 * HOUR_MS - 20 * MINUTE_MS,
      resetsIn: 3 * HOUR_MS
    });
    expect(preview?.action).toBe("On pace - nothing to slow");
  });

  it("tells a pace still being measured from one that lasts", () => {
    const measuring = pacingPreview({
      kind: PacedWindowKind.enum.session,
      forecast: {
        ...forecast({
          utilization: 50,
          runsOutIn: null,
          resetsIn: HOUR_MS
        }),
        isMeasured: false
      },
      slowedCount: 0,
      now: NOW
    });
    expect(measuring?.pace).toBe("Measuring the pace");
    expect(
      sessionPreview({
        utilization: 50,
        runsOutIn: null,
        resetsIn: HOUR_MS
      })?.pace
    ).toBe("At this pace: lasts until the reset");
  });

  it("shows nothing for a limit not read yet", () => {
    expect(
      pacingPreview({
        kind: PacedWindowKind.enum.session,
        forecast: null,
        slowedCount: 0,
        now: NOW
      })
    ).toBeNull();
  });
});

describe("pacingFacts", () => {
  it("lists only what pacing knows, in order", () => {
    const facts = pacingFacts({
      verdict: {
        windowKind: UsageWindowKind.enum.session,
        utilization: 85,
        runOutAt: NOW + HOUR_MS,
        readyAt: NOW + HOUR_MS
      },
      choice: {
        current: "Sonnet 5.5",
        original: "Opus 5.5"
      },
      restUntil: null,
      revertsAt: null,
      now: NOW
    });
    expect(facts).toEqual([
      "session window at 85% - on track to run out in 1h 0m, before its reset",
      "Runs Sonnet 5.5 - normally Opus 5.5"
    ]);
  });

  it("says a weekly window is ahead of an even spend, and when it is back on pace", () => {
    expect(
      pacingFacts({
        verdict: {
          windowKind: UsageWindowKind.enum.weekly,
          utilization: 80,
          runOutAt: NOW,
          readyAt: NOW + DAY_MS
        },
        choice: null,
        restUntil: null,
        revertsAt: null,
        now: NOW
      })
    ).toEqual(["weekly window at 80% - ahead of an even spend, back on pace in 1d 0h"]);
  });
});
