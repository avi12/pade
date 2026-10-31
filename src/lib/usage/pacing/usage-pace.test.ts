import { CreditsState, PacedWindowKind, UsageWindowKind } from "@/lib/types";
import type { UsageWindow } from "@/lib/types";
import {
  assessPace,
  burnRate,
  hasRoomToSpare,
  isRestEnough,
  PACE_FLOOR_PERCENTAGE,
  paceForecasts,
  pauseRequest,
  reportedPacedKinds,
  RESET_GRACE_MS,
  resumeRequest,
  slowRestUntil,
  WEEKLY_WINDOW_MS
} from "@/lib/usage/pacing/usage-pace";
import type { UsageSample } from "@/lib/usage/pacing/usage-pace";
import { describe, expect, it } from "vitest";

const NOW = new Date(2026, 9, 15, 12, 0, 0, 0).getTime();
const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;
/** The margin pacing acts on unless a test says otherwise - the default's. */
const EARLY_MS = 30 * MINUTE_MS;

/** A burn rate in the formula's units, from percent per hour. */
function perHour(percentage: number) {
  return percentage / HOUR_MS;
}

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

/** One reading a minute, rising `rise` percent each. */
function readings({ minutes, rise, resetsAt = "reset" }: {
  minutes: number;
  rise: number;
  resetsAt?: string;
}) {
  return Array.from({ length: minutes + 1 }, (_, i) => ({
    at: NOW + i * MINUTE_MS,
    utilization: 60 + i * rise,
    resetsAt
  })) satisfies UsageSample[];
}

describe("burnRate", () => {
  function percentPerHour(samples: UsageSample[]) {
    const rate = burnRate(samples);
    return rate === null ? null : rate * HOUR_MS;
  }

  it("measures how fast a window rises while its agents work", () => {
    expect(
      percentPerHour(
        readings({
          minutes: 12,
          rise: 0.5
        })
      )
    ).toBeCloseTo(30);
  });

  it("waits for ten minutes of working time before naming a rate", () => {
    expect(
      burnRate(
        readings({
          minutes: 9,
          rise: 0.5
        })
      )
    ).toBeNull();
  });

  it("leaves idle gaps and a new window out of the measure", () => {
    const working = readings({
      minutes: 10,
      rise: 0.5
    });
    const afterIdleHour = working.map(sample => ({
      ...sample,
      at: sample.at + HOUR_MS,
      utilization: sample.utilization + 5
    }));
    const lastWorking = afterIdleHour.at(-1) ?? working[0];
    const nextWindow = {
      at: lastWorking.at + MINUTE_MS,
      utilization: 1,
      resetsAt: "next"
    };
    expect(percentPerHour([...working, ...afterIdleHour, nextWindow])).toBeCloseTo(30);
  });
});

describe("assessPace", () => {
  function assess({ windows, rate, isPaused = false, earlyMs = EARLY_MS }: {
    windows: UsageWindow[];
    rate: number | null;
    isPaused?: boolean;
    earlyMs?: number;
  }) {
    return assessPace({
      account: account(windows),
      now: NOW,
      isPaused,
      marginOf: () => earlyMs,
      rateOf: () => rate
    });
  }

  it("pauses a session that would run out hours before its reset", () => {
    const pace = assess({
      windows: [sessionWindow({
        utilization: 70,
        resetsIn: 4 * HOUR_MS
      })],
      rate: perHour(30)
    });
    // 30% left at 30% an hour lasts one hour; the window runs free in 3h 50m.
    expect(pace?.window.kind).toBe(UsageWindowKind.enum.session);
    expect(pace?.readyAt).toBe(NOW + 4 * HOUR_MS - RESET_GRACE_MS - HOUR_MS);
    expect(pace?.continueAt).toBe(NOW + 4 * HOUR_MS - RESET_GRACE_MS);
  });

  it("lets a session run on while it would run out no earlier than the chosen margin", () => {
    // 30% left at 30% an hour lasts one hour; the window runs free in 1h 20m -
    // 20 minutes early, inside a 30-minute margin but past a 15-minute one.
    const windows = [sessionWindow({
      utilization: 70,
      resetsIn: 90 * MINUTE_MS
    })];
    expect(
      assess({
        windows,
        rate: perHour(30)
      })
    ).toBeNull();
    expect(
      assess({
        windows,
        rate: perHour(30),
        earlyMs: 15 * MINUTE_MS
      })
    ).not.toBeNull();
  });

  it("needs no window length - the same burn with the reset an hour off runs on", () => {
    expect(
      assess({
        windows: [sessionWindow({
          utilization: 70,
          resetsIn: 70 * MINUTE_MS
        })],
        rate: perHour(30)
      })
    ).toBeNull();
  });

  it("lets a session run on at 95% when the window resets in ten minutes", () => {
    expect(
      assess({
        windows: [sessionWindow({
          utilization: 95,
          resetsIn: 9 * MINUTE_MS
        })],
        rate: perHour(60)
      })
    ).toBeNull();
  });

  it("stops a session about to run dry half an hour out, until it can finish the stretch", () => {
    const pace = assess({
      windows: [sessionWindow({
        utilization: 95,
        resetsIn: 30 * MINUTE_MS
      })],
      rate: perHour(30),
      earlyMs: 5 * MINUTE_MS
    });
    // 5% at 30% an hour lasts ten minutes - enough once ten minutes stand
    // between it and the grace period.
    expect(pace?.readyAt).toBe(NOW + 10 * MINUTE_MS);
  });

  it("never pauses below the floor, however fast it burns", () => {
    expect(
      assess({
        windows: [sessionWindow({
          utilization: PACE_FLOOR_PERCENTAGE - 1,
          resetsIn: 4 * HOUR_MS
        })],
        rate: perHour(90)
      })
    ).toBeNull();
  });

  it("never pauses before a burn rate is known", () => {
    expect(
      assess({
        windows: [sessionWindow({
          utilization: 90,
          resetsIn: 4 * HOUR_MS
        })],
        rate: null
      })
    ).toBeNull();
  });

  it("keeps a paused session paused until its runway reaches the reset", () => {
    const windows = [sessionWindow({
      utilization: 70,
      resetsIn: HOUR_MS + RESET_GRACE_MS
    })];
    // Runway 54 minutes against an hour: inside the margin to start, short to finish.
    const rate = 30 / (54 * MINUTE_MS);
    expect(
      assess({
        windows,
        rate
      })
    ).toBeNull();
    expect(
      assess({
        windows,
        rate,
        isPaused: true
      })
    ).not.toBeNull();
  });

  it("still pauses near a session reset when the weekly window would run dry first", () => {
    const pace = assess({
      windows: [
        sessionWindow({
          utilization: 95,
          resetsIn: 5 * MINUTE_MS
        }),
        weeklyWindow({
          utilization: 90,
          resetsIn: 3 * DAY_MS
        })
      ],
      rate: perHour(2)
    });
    expect(pace?.window.kind).toBe(UsageWindowKind.enum.weekly);
  });

  it("still paces an account whose usage credits would carry it past the limit", () => {
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
        isPaused: false,
        marginOf: () => EARLY_MS,
        rateOf: () => perHour(60)
      })
    ).not.toBeNull();
  });

  it("never paces when the account can't be read", () => {
    expect(
      assessPace({
        account: null,
        now: NOW,
        isPaused: false,
        marginOf: () => EARLY_MS,
        rateOf: () => perHour(60)
      })
    ).toBeNull();
  });

  it("leaves a window alone while its pacing is off", () => {
    const windows = [weeklyWindow({
      utilization: 90,
      resetsIn: 3 * DAY_MS
    })];
    expect(
      assessPace({
        account: account(windows),
        now: NOW,
        isPaused: false,
        marginOf: kind => kind === PacedWindowKind.enum.weekly ? null : EARLY_MS,
        rateOf: () => perHour(2)
      })
    ).toBeNull();
  });
});

describe("assessPace, weekly", () => {
  const TWELVE_HOURS_MS = 12 * HOUR_MS;

  function assessWeekly({ utilization, resetsIn, isPaused = false }: {
    utilization: number;
    resetsIn: number;
    isPaused?: boolean;
  }) {
    return assessPace({
      account: account([weeklyWindow({
        utilization,
        resetsIn
      })]),
      now: NOW,
      isPaused,
      marginOf: () => TWELVE_HOURS_MS,
      rateOf: () => null
    });
  }

  it("needs no burn rate - it is judged against an even spend over the week", () => {
    // Four days in: an even spend reaches 90% in 2.3 more days.
    const pace = assessWeekly({
      utilization: 90,
      resetsIn: 3 * DAY_MS
    });
    expect(pace?.kind).toBe(PacedWindowKind.enum.weekly);
    expect(pace?.readyAt).toBe(NOW - 4 * DAY_MS + 0.9 * WEEKLY_WINDOW_MS);
  });

  it("lets a week run on within the margin of an even spend, however fast it burned today", () => {
    // Five days in, an even spend is at 71%; 75% is about seven hours ahead.
    expect(
      assessWeekly({
        utilization: 75,
        resetsIn: 2 * DAY_MS
      })
    ).toBeNull();
  });

  it("holds a paused session until the even spend catches up", () => {
    expect(
      assessWeekly({
        utilization: 75,
        resetsIn: 2 * DAY_MS,
        isPaused: true
      })
    ).not.toBeNull();
  });
});

describe("paceForecasts", () => {
  it("forecasts each paced window by its own measure", () => {
    const forecasts = paceForecasts({
      account: account([
        sessionWindow({
          utilization: 70,
          resetsIn: 3 * HOUR_MS
        }),
        weeklyWindow({
          utilization: 30,
          resetsIn: 3 * DAY_MS
        })
      ]),
      now: NOW,
      rateOf: window => window.kind === UsageWindowKind.enum.session ? perHour(30) : null
    });
    expect(forecasts.session?.runOutAt).toBe(NOW + HOUR_MS);
    // 30% in four days lasts another 9.3 days at that pace - past the reset.
    expect(forecasts.weekly?.runOutAt).toBeGreaterThan(NOW + 3 * DAY_MS);
    expect(forecasts.weekly?.evenPercentage).toBeCloseTo(400 / 7);
  });

  it("has no run-out time before a session's burn is measured", () => {
    expect(
      paceForecasts({
        account: account([sessionWindow({
          utilization: 70,
          resetsIn: 3 * HOUR_MS
        })]),
        now: NOW,
        rateOf: () => null
      }).session?.runOutAt
    ).toBeNull();
  });
});

describe("reportedPacedKinds", () => {
  it("leaves out a session limit the plan doesn't report", () => {
    expect(
      reportedPacedKinds(
        account([weeklyWindow({
          utilization: 30,
          resetsIn: 3 * DAY_MS
        })])
      )
    ).toEqual([PacedWindowKind.enum.weekly]);
  });

  it("keeps both in their order when both are reported", () => {
    expect(
      reportedPacedKinds(
        account([
          weeklyWindow({
            utilization: 30,
            resetsIn: 3 * DAY_MS
          }),
          sessionWindow({
            utilization: 70,
            resetsIn: 3 * HOUR_MS
          })
        ])
      )
    ).toEqual(PacedWindowKind.options);
  });

  it("offers every paced window while there is no reading", () => {
    expect(reportedPacedKinds(null)).toEqual(PacedWindowKind.options);
  });
});

describe("isRestEnough", () => {
  it("rests while no more rest than work is needed, and stops past that", () => {
    expect(
      isRestEnough({
        readyAt: NOW + HOUR_MS,
        continueAt: NOW + 2 * HOUR_MS,
        now: NOW
      })
    ).toBe(true);
    expect(
      isRestEnough({
        readyAt: NOW + 90 * MINUTE_MS,
        continueAt: NOW + 2 * HOUR_MS,
        now: NOW
      })
    ).toBe(false);
  });
});

describe("hasRoomToSpare", () => {
  function spare({ rate, utilization = 70 }: {
    rate: number | null;
    utilization?: number;
  }) {
    return hasRoomToSpare({
      account: account([sessionWindow({
        utilization,
        resetsIn: 2 * HOUR_MS + RESET_GRACE_MS
      })]),
      now: NOW,
      marginOf: () => EARLY_MS,
      rateOf: () => rate
    });
  }

  it("vouches only for a runway reaching well past the reset", () => {
    // 30% left lasts 3h50m at 8% an hour - past 1.5 times the 2h left.
    expect(spare({ rate: perHour(8) })).toBe(true);
    expect(spare({ rate: perHour(12) })).toBe(false);
  });

  it("can't vouch before a burn rate is measured, unless the window is under the floor", () => {
    expect(spare({ rate: null })).toBe(false);
    expect(
      spare({
        rate: null,
        utilization: PACE_FLOOR_PERCENTAGE - 1
      })
    ).toBe(true);
  });
});

describe("slowRestUntil", () => {
  it("rests long enough that work and rest average out to a lasting pace", () => {
    // Runway 1h against 2h left: rest as long as it just worked.
    expect(
      slowRestUntil({
        readyAt: NOW + HOUR_MS,
        continueAt: NOW + 2 * HOUR_MS,
        stretch: 20 * MINUTE_MS,
        now: NOW
      })
    ).toBe(NOW + 20 * MINUTE_MS);
  });

  it("rests at least five minutes, and never past where a full stop would end", () => {
    expect(
      slowRestUntil({
        readyAt: NOW + 30 * MINUTE_MS,
        continueAt: NOW + 2 * HOUR_MS,
        stretch: MINUTE_MS,
        now: NOW
      })
    ).toBe(NOW + 5 * MINUTE_MS);
    expect(
      slowRestUntil({
        readyAt: NOW + 30 * MINUTE_MS,
        continueAt: NOW + HOUR_MS,
        stretch: 2 * HOUR_MS,
        now: NOW
      })
    ).toBe(NOW + 30 * MINUTE_MS);
  });
});

describe("pauseRequest", () => {
  const window = sessionWindow({
    utilization: 72,
    resetsIn: 3 * HOUR_MS
  });

  it("names the window, its spend and the time to its reset", () => {
    const text = pauseRequest({
      window,
      now: NOW,
      isSlowing: false
    });
    expect(text).toContain("72%");
    expect(text).toContain("session usage");
    expect(text).toContain("in 3h 0m");
  });

  it("asks a slowed agent for a short break", () => {
    expect(
      pauseRequest({
        window,
        now: NOW,
        isSlowing: true
      })
    ).toContain("short break");
  });

  it("pauses the agent's delegated background work too", () => {
    const text = pauseRequest({
      window,
      now: NOW,
      isSlowing: true
    });
    expect(text).toContain("stop every background agent, workflow and teammate");
    expect(text).toContain("launch nothing new");
  });
});

describe("resumeRequest", () => {
  it("says it comes from pacing, not the user", () => {
    expect(resumeRequest()).toContain("not the user");
  });
});
