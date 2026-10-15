import {
  assessPace,
  PACE_FLOOR_PERCENTAGE,
  paceLine,
  pauseRequest,
  RESET_GRACE_MS
} from "@/lib/stores/usage/usagePacing.svelte";
import { CreditsState, UsageWindowKind } from "@/lib/types";
import type { UsageWindow } from "@/lib/types";
import { describe, expect, it } from "vitest";

const NOW = new Date(2026, 9, 15, 12, 0, 0, 0).getTime();
const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

function sessionWindow({ utilization, resetsIn }: {
  utilization: number;
  resetsIn: number;
}) {
  return {
    key: "five_hour",
    kind: UsageWindowKind.enum.session,
    label: "Session",
    utilization,
    resetsAt: new Date(NOW + resetsIn).toISOString()
  } satisfies UsageWindow;
}

function weeklyWindow({ utilization, resetsIn }: {
  utilization: number;
  resetsIn: number;
}) {
  return {
    key: "seven_day",
    kind: UsageWindowKind.enum.weekly,
    label: "Weekly",
    utilization,
    resetsAt: new Date(NOW + resetsIn).toISOString()
  } satisfies UsageWindow;
}

function account(windows: UsageWindow[]) {
  return {
    windows,
    plan: "Max",
    source: "test",
    credits: CreditsState.enum.off
  };
}

describe("paceLine", () => {
  it("starts at the floor when the window opens and climbs toward its reset", () => {
    expect(
      paceLine({
        window: sessionWindow({
          utilization: 0,
          resetsIn: 5 * HOUR_MS
        }),
        now: NOW
      })
    ).toBe(PACE_FLOOR_PERCENTAGE);
    expect(
      paceLine({
        window: sessionWindow({
          utilization: 0,
          resetsIn: 2.5 * HOUR_MS
        }),
        now: NOW
      })
    ).toBe(80);
  });

  it("paces no one within the grace before a reset", () => {
    expect(
      paceLine({
        window: sessionWindow({
          utilization: 95,
          resetsIn: RESET_GRACE_MS
        }),
        now: NOW
      })
    ).toBeNull();
  });

  it("leaves per-model caps and unknown resets alone", () => {
    expect(
      paceLine({
        window: {
          kind: UsageWindowKind.enum.model,
          resetsAt: new Date(NOW + HOUR_MS).toISOString()
        },
        now: NOW
      })
    ).toBeNull();
    expect(
      paceLine({
        window: {
          kind: UsageWindowKind.enum.session,
          resetsAt: null
        },
        now: NOW
      })
    ).toBeNull();
  });
});

describe("assessPace", () => {
  it("pauses a session far ahead of its line with hours to go", () => {
    const pace = assessPace({
      account: account([sessionWindow({
        utilization: 65,
        resetsIn: 4.5 * HOUR_MS
      })]),
      now: NOW,
      isPaused: false
    });
    expect(pace?.window.kind).toBe(UsageWindowKind.enum.session);
    // Half an hour in the line stands at 64%; it reaches 65% plus the resume
    // margin 0.175 of the way through the window, 52.5 minutes in.
    expect(pace?.resumeAt).toBe(NOW + 22.5 * MINUTE_MS);
  });

  it("lets a session run on at 95% when the window resets in ten minutes", () => {
    expect(
      assessPace({
        account: account([sessionWindow({
          utilization: 95,
          resetsIn: 9 * MINUTE_MS
        })]),
        now: NOW,
        isPaused: false
      })
    ).toBeNull();
  });

  it("still pauses near a session reset when the weekly window is ahead of its line", () => {
    const pace = assessPace({
      account: account([
        sessionWindow({
          utilization: 95,
          resetsIn: 5 * MINUTE_MS
        }),
        weeklyWindow({
          utilization: 90,
          resetsIn: 3 * DAY_MS
        })
      ]),
      now: NOW,
      isPaused: false
    });
    expect(pace?.window.kind).toBe(UsageWindowKind.enum.weekly);
  });

  it("keeps a paused session paused until the line clears the resume margin", () => {
    const window = sessionWindow({
      utilization: 79,
      resetsIn: 2.5 * HOUR_MS
    });
    expect(
      assessPace({
        account: account([window]),
        now: NOW,
        isPaused: false
      })
    ).toBeNull();
    expect(
      assessPace({
        account: account([window]),
        now: NOW,
        isPaused: true
      })
    ).not.toBeNull();
  });

  it("never paces an account whose usage credits carry the work", () => {
    expect(
      assessPace({
        account: {
          ...account([sessionWindow({
            utilization: 90,
            resetsIn: 4 * HOUR_MS
          })]),
          credits: CreditsState.enum.available
        },
        now: NOW,
        isPaused: false
      })
    ).toBeNull();
  });

  it("never paces when the account can't be read", () => {
    expect(
      assessPace({
        account: null,
        now: NOW,
        isPaused: false
      })
    ).toBeNull();
  });
});

describe("pauseRequest", () => {
  it("names the window, its spend and the time to its reset", () => {
    const text = pauseRequest({
      window: sessionWindow({
        utilization: 72,
        resetsIn: 3 * HOUR_MS
      }),
      now: NOW
    });
    expect(text).toContain("72%");
    expect(text).toContain("5-hour");
    expect(text).toContain("in 3h 0m");
  });
});
