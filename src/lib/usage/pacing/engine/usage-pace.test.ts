import { describe, expect, it } from "vitest";

import { PacedWindowKind } from "@/lib/types/pacing";
import { CreditsState, UsageWindowKind } from "@/lib/types/usage";
import type { UsageWindow } from "@/lib/types/usage";
import {
  assessPace,
  burnRate,
  hasRoomToSpare,
  isForecastOverPace,
  isNearlySpent,
  paceForecasts,
  projectedRate,
  reportedPacedKinds,
  RESET_GRACE_MS,
  slowRestUntil,
  workRhythm
} from "@/lib/usage/pacing/engine/usage-pace";
import type { UsageSample } from "@/lib/usage/pacing/engine/usage-pace";

const NOW = new Date(2026, 9, 15, 12, 0, 0, 0).getTime();
const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/** A burn rate in the formula's units, from percent per hour. */
function perHour(percentage: number) {
  return percentage / HOUR_MS;
}

function sessionWindow({ utilization, resetsIn }: { utilization: number; resetsIn: number }) {
  return {
    key: "five_hour",
    kind: UsageWindowKind.enum.session,
    label: "Session",
    utilization,
    resetsAt: new Date(NOW + resetsIn).toISOString()
  } satisfies UsageWindow;
}

function weeklyWindow({ utilization, resetsIn }: { utilization: number; resetsIn: number }) {
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
function readings({ minutes, rise, resetsAt = "reset" }: { minutes: number; rise: number; resetsAt?: string }) {
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

  it("counts readings whose reset time differs only by the vendor's jitter as one window", () => {
    const jittered = readings({
      minutes: 12,
      rise: 0.5,
      resetsAt: "2026-11-14T00:59:59.533870+00:00"
    }).map((sample, i) => ({
      ...sample,
      resetsAt: `2026-11-14T00:59:59.${String(500000 + i * 7919).padStart(6, "0")}+00:00`
    }));
    expect(percentPerHour(jittered)).toBeCloseTo(30);
  });

  it("still tells a new window from the old one by hours", () => {
    const old = readings({
      minutes: 10,
      rise: 0.5,
      resetsAt: "2026-11-14T00:59:59.533870+00:00"
    });
    const next = {
      at: (old.at(-1)?.at ?? NOW) + MINUTE_MS,
      utilization: 1,
      resetsAt: "2026-11-14T05:59:59.533870+00:00"
    };
    expect(percentPerHour([...old, next])).toBeCloseTo(30);
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

describe("reportedPacedKinds", () => {
  it("leaves out a session limit the plan doesn't report", () => {
    expect(
      reportedPacedKinds(
        account([
          weeklyWindow({
            utilization: 30,
            resetsIn: 3 * DAY_MS
          })
        ])
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

/** A week three and a half days from its reset, its even spend at 50%. */
const MID_WEEK_MS = 3.5 * DAY_MS;

describe("assessPace", () => {
  function assess({
    windows,
    rate,
    isPaused = false
  }: {
    windows: UsageWindow[];
    rate: number | null;
    isPaused?: boolean;
  }) {
    return assessPace({
      account: account(windows),
      now: NOW,
      isPaused,
      rateOf: () => rate
    });
  }

  function midWeek(utilization: number) {
    return [
      weeklyWindow({
        utilization,
        resetsIn: MID_WEEK_MS
      })
    ];
  }

  it("lets one session's burst through while the allowance covers it", () => {
    expect(
      assess({
        windows: midWeek(50),
        rate: perHour(1)
      })
    ).toBeNull();
  });

  it("slows several sessions bursting at once to the share the week can keep", () => {
    const pace = assess({
      windows: midWeek(50),
      rate: perHour(4)
    });
    // (50% over 83h 50m + the 10% allowance over 8 h) / 4% an hour
    expect(pace?.keep).toBeCloseTo(0.4616, 4);
    expect(pace?.kind).toBe(PacedWindowKind.enum.weekly);
  });

  it("has no usage level below which it looks away", () => {
    const pace = assess({
      windows: [
        weeklyWindow({
          utilization: 5,
          resetsIn: 6.5 * DAY_MS
        })
      ],
      rate: perHour(4)
    });
    expect(pace?.keep).toBeCloseTo(0.4649, 4);
  });

  it("spreads what is left over the time left once the allowance is spent, without stopping", () => {
    const pace = assess({
      windows: midWeek(60),
      rate: perHour(1)
    });
    expect(pace?.keep).toBeCloseTo(40 / (84 - 1 / 6), 4);
    expect(pace?.window.utilization).toBe(60);
  });

  it("stops only with next to nothing left in the window, however far over the pace", () => {
    function isNearlySpentAt(utilization: number) {
      return isNearlySpent(
        weeklyWindow({
          utilization,
          resetsIn: 3 * DAY_MS
        })
      );
    }

    expect(isNearlySpentAt(66)).toBe(false);
    expect(isNearlySpentAt(97)).toBe(false);
    expect(isNearlySpentAt(98)).toBe(true);
  });

  it("scales the allowance's payout to a session window", () => {
    const pace = assess({
      windows: [
        sessionWindow({
          utilization: 50,
          resetsIn: 2.5 * HOUR_MS
        })
      ],
      rate: perHour(90)
    });
    // (50% over 2h 20m + the 10% allowance over a quarter hour) / 90% an hour
    expect(pace?.keep).toBeCloseTo(0.7048, 4);
  });

  it("leaves a little slack under on pace, so whole-percent jitter slows nothing", () => {
    expect(
      assess({
        windows: midWeek(50),
        rate: perHour(1.9)
      })
    ).toBeNull();
  });

  it("holds a paused session until its keep share is back to 1", () => {
    const pace = assess({
      windows: midWeek(50),
      rate: perHour(1.9),
      isPaused: true
    });
    expect(pace?.keep).toBeCloseTo(0.9718, 4);
    // ready once 50% over the time left reaches 1.9 - 1.25 = 0.65% an hour
    expect(((pace?.readyAt ?? NOW) - NOW) / HOUR_MS).toBeCloseTo(84 - 1 / 6 - 50 / 0.65, 2);
  });

  it("lets a paused session of an overspent week go within hours, as its even spend catches up", () => {
    const pace = assess({
      windows: midWeek(60),
      rate: perHour(1),
      isPaused: true
    });
    const readyIn = (pace?.readyAt ?? NOW) - NOW;
    expect(readyIn).toBeGreaterThan(6 * HOUR_MS);
    expect(readyIn).toBeLessThan(7 * HOUR_MS);
  });

  it("reports the window that keeps the least", () => {
    const pace = assessPace({
      account: account([
        sessionWindow({
          utilization: 50,
          resetsIn: 2.5 * HOUR_MS
        }),
        ...midWeek(50)
      ]),
      now: NOW,
      isPaused: false,
      rateOf: window => (window.kind === UsageWindowKind.enum.session ? perHour(90) : perHour(4))
    });
    expect(pace?.kind).toBe(PacedWindowKind.enum.weekly);
    expect(pace?.continueAt).toBe(NOW + MID_WEEK_MS - RESET_GRACE_MS);
  });

  it("paces no one this close to the reset", () => {
    expect(
      assess({
        windows: [
          sessionWindow({
            utilization: 95,
            resetsIn: 9 * MINUTE_MS
          })
        ],
        rate: perHour(90)
      })
    ).toBeNull();
  });

  it("never paces before a pace is known", () => {
    expect(
      assess({
        windows: midWeek(90),
        rate: null
      })
    ).toBeNull();
  });

  it("still paces an account whose usage credits would carry it past the limit", () => {
    expect(
      assessPace({
        account: {
          ...account(midWeek(50)),
          credits: CreditsState.enum.available
        },
        now: NOW,
        isPaused: false,
        rateOf: () => perHour(4)
      })
    ).not.toBeNull();
  });

  it("never paces when the account can't be read", () => {
    expect(
      assessPace({
        account: null,
        now: NOW,
        isPaused: false,
        rateOf: () => perHour(60)
      })
    ).toBeNull();
  });
});

describe("hasRoomToSpare", () => {
  function hasRoom(rate: number | null) {
    return hasRoomToSpare({
      account: account([
        weeklyWindow({
          utilization: 50,
          resetsIn: MID_WEEK_MS
        })
      ]),
      now: NOW,
      rateOf: () => rate
    });
  }

  it("vouches only for a pace well under what the window lets it keep", () => {
    expect(hasRoom(perHour(1))).toBe(true);
    expect(hasRoom(perHour(1.6))).toBe(false);
  });

  it("can't vouch before a pace is measured", () => {
    expect(hasRoom(null)).toBe(false);
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
      rateOf: window => (window.kind === UsageWindowKind.enum.session ? perHour(30) : null)
    });
    expect(forecasts.session?.runOutAt).toBe(NOW + HOUR_MS);
    expect(forecasts.session?.keep).toBeCloseTo(30 / (3 - 1 / 6) / 30, 4);
    expect(forecasts.session !== undefined && isForecastOverPace(forecasts.session)).toBe(true);
    // 30% in four days lasts another 9.3 days at that pace - past the reset.
    expect(forecasts.weekly?.runOutAt).toBeGreaterThan(NOW + 3 * DAY_MS);
    expect(forecasts.weekly?.keep).toBeNull();
  });

  it("has no run-out time before a session's burn is measured", () => {
    expect(
      paceForecasts({
        account: account([
          sessionWindow({
            utilization: 70,
            resetsIn: 3 * HOUR_MS
          })
        ]),
        now: NOW,
        rateOf: () => null
      }).session?.runOutAt
    ).toBeNull();
  });
});

describe("projectedRate", () => {
  it("counts a session that just started on a dear model at once", () => {
    expect(
      projectedRate({
        measured: perHour(1),
        ratePerLoad: perHour(0.05),
        load: 40
      })
    ).toBeCloseTo(perHour(2));
  });

  it("keeps the measured burn when it is the higher", () => {
    expect(
      projectedRate({
        measured: perHour(4),
        ratePerLoad: perHour(0.05),
        load: 40
      })
    ).toBeCloseTo(perHour(4));
  });

  it("lifts the measured burn at most threefold, however far the load forecast runs", () => {
    expect(
      projectedRate({
        measured: perHour(1),
        ratePerLoad: perHour(10),
        load: 40
      })
    ).toBeCloseTo(perHour(3));
  });

  it("uses whichever is known, and nothing when neither is", () => {
    expect(
      projectedRate({
        measured: null,
        ratePerLoad: perHour(0.05),
        load: 40
      })
    ).toBeCloseTo(perHour(2));
    expect(
      projectedRate({
        measured: perHour(1),
        ratePerLoad: null,
        load: 40
      })
    ).toBeCloseTo(perHour(1));
    expect(
      projectedRate({
        measured: null,
        ratePerLoad: null,
        load: 40
      })
    ).toBeNull();
  });
});

describe("workRhythm", () => {
  it("works a quarter hour and rests what keeps the account to its keep share", () => {
    expect(workRhythm(0.5)).toEqual({
      stretch: 15 * MINUTE_MS,
      rest: 15 * MINUTE_MS
    });
    expect(workRhythm(0.25)).toEqual({
      stretch: 15 * MINUTE_MS,
      rest: 45 * MINUTE_MS
    });
  });

  it("does the same total work whatever the stretch: work over cycle length stays the keep share", () => {
    for (const keep of [0.12, 0.3, 0.6, 0.9]) {
      const { stretch, rest } = workRhythm(keep);
      expect(stretch / (stretch + rest)).toBeCloseTo(keep);
    }
  });

  it("rests two hours at most, so a slow keep share still measures the pace again", () => {
    expect(workRhythm(0.02)).toEqual({
      stretch: 15 * MINUTE_MS,
      rest: 120 * MINUTE_MS
    });
    expect(workRhythm(0).rest).toBe(120 * MINUTE_MS);
  });

  it("works longer, not rests less, when the keep share is high", () => {
    const { stretch, rest } = workRhythm(0.9);
    expect(rest).toBeCloseTo(5 * MINUTE_MS);
    expect(stretch).toBeCloseTo(45 * MINUTE_MS);
  });

  it("rests the minimum when the account keeps all its spend", () => {
    expect(workRhythm(1.4)).toEqual({
      stretch: 15 * MINUTE_MS,
      rest: 5 * MINUTE_MS
    });
  });
});

describe("slowRestUntil", () => {
  it("ends the rest where the rhythm says", () => {
    expect(
      slowRestUntil({
        keep: 0.5,
        readyAt: NOW + 12 * HOUR_MS,
        now: NOW
      })
    ).toBe(NOW + 15 * MINUTE_MS);
  });

  it("never rests past when the session would be back on pace anyway", () => {
    expect(
      slowRestUntil({
        keep: 0.5,
        readyAt: NOW + 8 * MINUTE_MS,
        now: NOW
      })
    ).toBe(NOW + 8 * MINUTE_MS);
  });
});
