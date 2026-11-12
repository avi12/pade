import { PacedWindowKind } from "@/lib/types/pacing";
import { CreditsState, UsageWindowKind } from "@/lib/types/usage";
import {
  isPacingOn,
  pacingFacts,
  pacingFactText,
  pacingPreview,
  pacingResume,
  pacingWhen,
  spokenTime
} from "@/lib/usage/pacing/pacing-policy";
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
    expect(isPacingOn({ paceUsage: true })).toBe(true);
    expect(isPacingOn({ paceUsage: false })).toBe(false);
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
  it("lists only what pacing knows, in order, each time kept apart", () => {
    const resume = pacingResume({
      isStopped: false,
      resumesAt: NOW + DAY_MS,
      original: "Opus 5.5"
    });
    expect(
      pacingFacts({
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
        resume
      })
    ).toEqual([
      {
        text: "session window at 85% - runs out before its reset",
        time: NOW + HOUR_MS
      },
      {
        text: "Runs Sonnet 5.5 - normally Opus 5.5",
        time: null
      },
      {
        text: "Back to Opus 5.5",
        time: NOW + DAY_MS
      }
    ]);
  });

  it("says a weekly window is ahead of an even spend, and when it is back on pace", () => {
    const [verdict] = pacingFacts({
      verdict: {
        windowKind: UsageWindowKind.enum.weekly,
        utilization: 80,
        runOutAt: NOW,
        readyAt: NOW + DAY_MS
      },
      choice: null,
      resume: pacingResume({
        isStopped: true,
        resumesAt: null,
        original: null
      })
    });
    expect(verdict).toEqual({
      text: "weekly window at 80% - ahead of an even spend, back on pace",
      time: NOW + DAY_MS
    });
  });
});

describe("pacingWhen", () => {
  it("reads a time as a clock time or as a countdown", () => {
    const time = NOW + HOUR_MS;
    expect(
      pacingWhen({
        time,
        now: NOW,
        isCountdown: false
      })
    ).toBe(
      `at ${spokenTime({
        time,
        now: NOW
      })}`
    );
    expect(
      pacingWhen({
        time,
        now: NOW,
        isCountdown: true
      })
    ).toBe("in 1h 0m");
  });

  it("falls back to the clock for a time already past", () => {
    expect(
      pacingWhen({
        time: NOW - HOUR_MS,
        now: NOW,
        isCountdown: true
      })
    ).toMatch(/^at /v);
  });
});

describe("pacingResume", () => {
  it("says when a paused session continues, as the badge's hint reads it", () => {
    const fact = pacingResume({
      isStopped: true,
      resumesAt: NOW + HOUR_MS,
      original: "Opus 5.5"
    });
    expect(
      pacingFactText({
        fact,
        now: NOW
      })
    ).toBe(
      `Continues at ${spokenTime({
        time: NOW + HOUR_MS,
        now: NOW
      })}`
    );
  });

  it("names the model and effort a slowed session goes back to", () => {
    expect(
      pacingResume({
        isStopped: false,
        resumesAt: NOW + HOUR_MS,
        original: "Opus 5.5 · high effort"
      }).text
    ).toBe("Back to Opus 5.5 · high effort");
  });

  it("falls back to the pace when no time is known yet", () => {
    expect(
      pacingResume({
        isStopped: true,
        resumesAt: null,
        original: null
      })
    ).toEqual({
      text: "Continues once usage is back on pace",
      time: null
    });
  });
});
