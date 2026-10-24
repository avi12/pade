import { CreditsState, PacingStep, UsageWindowKind } from "@/lib/types";
import {
  allowedSteps,
  DEFAULT_PACING,
  marginLabel,
  PACING_EARLY_OPTIONS,
  pacingFacts,
  PacingPreset,
  pacingPreset,
  pacingPreview,
  withPreset,
  withStep
} from "@/lib/usage/pacing/pacing-policy";
import { paceForecasts, RESET_GRACE_MS } from "@/lib/usage/pacing/usage-pace";
import type { PaceForecast } from "@/lib/usage/pacing/usage-pace";
import { describe, expect, it } from "vitest";

const NOW = new Date(2026, 9, 15, 12, 0, 0, 0).getTime();
const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;
const SESSION_PACING = DEFAULT_PACING.session;

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

describe("pacingPreview, weekly", () => {
  it("shows where an even spend would be and acts once ahead of it by the margin", () => {
    const resetsIn = 3 * DAY_MS;
    // Four days in, an even spend is at 57%; 90% is reached by an even spend
    // in 2.3 days - far past a 12-hour margin.
    const preview = pacingPreview({
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
      settings: DEFAULT_PACING.weekly,
      now: NOW
    });
    expect(preview.meter?.detail).toContain("even spend 57%");
    expect(preview.wouldDo).toBe("Would now: lower effort");
  });
});

describe("marginLabel", () => {
  it("names minutes, hours and days", () => {
    expect(PACING_EARLY_OPTIONS.session.map(minutes => marginLabel(minutes))).toEqual(["15 min", "30 min", "1 h", "2 h"]);
    expect(PACING_EARLY_OPTIONS.weekly.map(minutes => marginLabel(minutes))).toEqual(["6 h", "12 h", "1 d", "2 d"]);
  });
});

describe("pacingPreview", () => {
  it("still acts close to the reset when the chosen margin is longer than the time left", () => {
    // 2 h early can't happen with 16 minutes to go; the margin shrinks to half
    // the 6 minutes before the grace, and running dry in 2 minutes is past it.
    const preview = pacingPreview({
      forecast: forecast({
        utilization: 99,
        runsOutIn: 2 * MINUTE_MS,
        resetsIn: 16 * MINUTE_MS
      }),
      settings: {
        ...SESSION_PACING,
        earlyMinutes: 120
      },
      now: NOW
    });
    expect(preview.wouldDo).toBe("Would now: lower effort");
  });

  it("tells a pace still being measured from one that spends nothing", () => {
    const measuring = pacingPreview({
      forecast: {
        ...forecast({
          utilization: 99,
          runsOutIn: null,
          resetsIn: HOUR_MS
        }),
        isMeasured: false
      },
      settings: SESSION_PACING,
      now: NOW
    });
    expect(measuring.meter?.detail).toContain("pace not measured yet");
    expect(measuring.wouldDo).toContain("Measuring the pace");
    expect(
      pacingPreview({
        forecast: forecast({
          utilization: 99,
          runsOutIn: null,
          resetsIn: HOUR_MS
        }),
        settings: SESSION_PACING,
        now: NOW
      }).meter?.detail
    ).toContain("nothing spent lately");
  });

  it("says what pacing would do to an agent running out well before its reset", () => {
    const preview = pacingPreview({
      forecast: forecast({
        utilization: 80,
        runsOutIn: HOUR_MS,
        resetsIn: 3 * HOUR_MS
      }),
      settings: SESSION_PACING,
      now: NOW
    });
    expect(preview.meter?.detail).toBe("80% · runs out in 1h 0m · resets in 3h 0m");
    expect(preview.wouldDo).toBe("Would now: lower effort");
  });

  it("reads on pace inside the chosen margin, and waits for a measured pace", () => {
    expect(
      pacingPreview({
        forecast: forecast({
          utilization: 80,
          runsOutIn: 3 * HOUR_MS - 20 * MINUTE_MS,
          resetsIn: 3 * HOUR_MS
        }),
        settings: SESSION_PACING,
        now: NOW
      }).wouldDo
    ).toBe("On pace - nothing to do");
    expect(
      pacingPreview({
        forecast: null,
        settings: SESSION_PACING,
        now: NOW
      })
    ).toEqual({
      meter: null,
      wouldDo: "Measures the pace while the agent works"
    });
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

describe("pacingPreset", () => {
  it("reads the defaults as Balanced", () => {
    expect(pacingPreset(SESSION_PACING)).toBe(PacingPreset.Balanced);
    expect(pacingPreset(DEFAULT_PACING.weekly)).toBe(PacingPreset.Balanced);
  });

  it("reads switched-off pacing as Off, whatever its steps", () => {
    expect(
      pacingPreset({
        ...SESSION_PACING,
        isOn: false
      })
    ).toBe(PacingPreset.Off);
  });

  it("names a step set no preset takes Custom, and ignores step order", () => {
    expect(
      pacingPreset({
        ...SESSION_PACING,
        steps: [PacingStep.enum.stop, PacingStep.enum.effort]
      })
    ).toBe(PacingPreset.Custom);
    expect(
      pacingPreset({
        ...SESSION_PACING,
        steps: [PacingStep.enum.rest, PacingStep.enum.effort, PacingStep.enum.model]
      })
    ).toBe(PacingPreset.Balanced);
  });
});

describe("withPreset", () => {
  it("picks a preset's steps and turns pacing on", () => {
    const strict = withPreset({
      settings: {
        ...SESSION_PACING,
        isOn: false
      },
      preset: PacingPreset.Strict
    });
    expect(strict.isOn).toBe(true);
    expect(allowedSteps(strict)).toEqual(["effort", "model", "rest", "stop"]);
  });

  it("keeps the steps when turning pacing off", () => {
    const off = withPreset({
      settings: SESSION_PACING,
      preset: PacingPreset.Off
    });
    expect(off.isOn).toBe(false);
    expect(off.steps).toEqual(SESSION_PACING.steps);
  });
});

describe("withStep", () => {
  it("adds and removes a step, keeping climbing order", () => {
    const withStop = withStep({
      settings: SESSION_PACING,
      step: PacingStep.enum.stop,
      isAllowed: true
    });
    expect(withStop.steps).toEqual(["effort", "model", "rest", "stop"]);
    expect(
      withStep({
        settings: withStop,
        step: PacingStep.enum.model,
        isAllowed: false
      }).steps
    ).toEqual(["effort", "rest", "stop"]);
  });
});
