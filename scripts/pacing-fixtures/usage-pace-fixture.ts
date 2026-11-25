// The usage-pace fixture: a grid and a seeded scatter of windows, forecasts, rest
// and projected-pace cases swept through `usage-pace`.

import { PacedWindowKind } from "@/lib/types/pacing";
import { UsageWindowKind } from "@/lib/types/usage";
import type { AccountUsage, UsageWindow } from "@/lib/types/usage";
import {
  assessPace,
  burnRate,
  hasRoomToSpare,
  isForecastOverPace,
  paceForecasts,
  projectedRate,
  slowRestUntil,
  workRhythm
} from "@/lib/usage/pacing/engine/usage-pace";
import type { PaceForecast, UsageSample } from "@/lib/usage/pacing/engine/usage-pace";

import { between, DAY, HOUR, MINUTE, NOW, perHour, pick, random } from "./fixture-primitives";

function windowOf({
  kind,
  utilization,
  resetsIn
}: {
  kind: UsageWindow["kind"];
  utilization: number;
  resetsIn: number;
}) {
  return {
    key: kind,
    kind,
    label: kind,
    utilization,
    resetsAt: new Date(NOW + resetsIn).toISOString()
  } satisfies UsageWindow;
}

function accountOf(windows: UsageWindow[]) {
  return {
    windows,
    plan: "Max",
    source: "fixture"
  } satisfies AccountUsage;
}

type PerKind = Record<PacedWindowKind, number | null>;

/** What one pace case feeds the formula: the account's windows and the pace
 *  its agents are measured at per paced kind. */
interface PaceInputs {
  windows: UsageWindow[];
  rates: PerKind;
  isPaused: boolean;
}

function lookup({ window, perKind }: { window: UsageWindow; perKind: PerKind }) {
  const kind = PacedWindowKind.safeParse(window.kind);
  return kind.success ? perKind[kind.data] : null;
}

function paceParameters(inputs: PaceInputs) {
  return {
    account: accountOf(inputs.windows),
    now: NOW,
    rateOf: (window: UsageWindow) =>
      lookup({
        window,
        perKind: inputs.rates
      })
  };
}

const UNMEASURED: PerKind = {
  [PacedWindowKind.enum.session]: null,
  [PacedWindowKind.enum.weekly]: null
};

/** One account with a single paced window. */
function singleWindowCase({
  kind,
  utilization,
  resetsIn,
  rate,
  isPaused
}: {
  kind: PacedWindowKind;
  utilization: number;
  resetsIn: number;
  rate: number | null;
  isPaused: boolean;
}) {
  return {
    windows: [
      windowOf({
        kind,
        utilization,
        resetsIn
      })
    ],
    rates: {
      ...UNMEASURED,
      [kind]: rate
    },
    isPaused
  } satisfies PaceInputs;
}

const SESSION_RESETS_IN = [9 * MINUTE, 30 * MINUTE, 90 * MINUTE, 4 * HOUR];

const WEEKLY_RESETS_IN = [30 * MINUTE, 2 * DAY, 3.5 * DAY, 6 * DAY];

const SESSION_RATES = [null, perHour(10), perHour(30), perHour(90)];

const WEEKLY_RATES = [null, perHour(0.5), perHour(2), perHour(6)];

const BOTH_WAYS = [false, true];

const UTILIZATIONS = [5, 30, 59, 75, 95];

/** Where the keep share sits near its thresholds: three and a half days from a
 *  weekly reset, at an even spend of 50%. */
const KEEP_BOUNDARY_UTILIZATION = 55;

const KEEP_BOUNDARY_RESETS_IN = 3.5 * DAY;

const KEEP_BOUNDARY_CASE_COUNT = 33;

/** The boundary sweep's slowest pace and its step, in percent per hour. */
const KEEP_BOUNDARY_FIRST_RATE = 0.4;

const KEEP_BOUNDARY_RATE_STEP = 0.05;

const REVERSED_WINDOWS_THRESHOLD = 0.5;

const SCATTERED_CASE_COUNT = 70;

const SCATTERED_SESSION_RATES = [null, perHour(10), perHour(30), perHour(60)];

const SCATTERED_WEEKLY_RATES = [null, perHour(0.5), perHour(1), perHour(4)];

const PAUSED_THRESHOLD = 0.5;

const BURN_START_LOW = 50;

const BURN_START_HIGH = 80;

const BURN_START_STEP = 0.5;

const BURN_DELTA_LOW = -0.5;

const BURN_DELTA_STEP = 0.25;

const BURN_GAPS = [30_000, MINUTE, MINUTE, 2 * MINUTE, 6 * MINUTE, 0];

const BURN_RATE_CASE_COUNT = 30;

function bothPauseStates(base: Omit<Parameters<typeof singleWindowCase>[0], "isPaused">) {
  return BOTH_WAYS.map(isPaused =>
    singleWindowCase({
      ...base,
      isPaused
    })
  );
}

function gridOf({ kind, resetsIn, rates }: { kind: PacedWindowKind; resetsIn: number[]; rates: (number | null)[] }) {
  return UTILIZATIONS.flatMap(utilization =>
    resetsIn.flatMap(resets =>
      rates.flatMap(rate =>
        bothPauseStates({
          kind,
          utilization,
          resetsIn: resets,
          rate
        })
      )
    )
  );
}

/** The weekly paces around the keep share's thresholds - slow down, ready and
 *  room to spare. */
function keepBoundaryCases() {
  return Array.from({ length: KEEP_BOUNDARY_CASE_COUNT }, (_, step) =>
    singleWindowCase({
      kind: PacedWindowKind.enum.weekly,
      utilization: KEEP_BOUNDARY_UTILIZATION,
      resetsIn: KEEP_BOUNDARY_RESETS_IN,
      rate: perHour(KEEP_BOUNDARY_FIRST_RATE + step * KEEP_BOUNDARY_RATE_STEP),
      isPaused: step % 2 === 1
    })
  );
}

function scatteredWindow({
  kind,
  utilization,
  resetsIn
}: {
  kind: UsageWindow["kind"];
  utilization: {
    low: number;
    high: number;
    step: number;
  };
  resetsIn: {
    low: number;
    high: number;
    step: number;
  };
}) {
  return windowOf({
    kind,
    utilization: between(utilization),
    resetsIn: between(resetsIn)
  });
}

/** The windows an account may report, how likely each is to be missing
 *  (`absent`), and the range its spend and reset are drawn from. */
const SCATTERED_WINDOWS = [
  {
    kind: UsageWindowKind.enum.session,
    absent: 0.15,
    utilization: {
      low: 0,
      high: 100,
      step: 0.5
    },
    resetsIn: {
      low: 2 * MINUTE,
      high: 5 * HOUR,
      step: MINUTE
    }
  },
  {
    kind: UsageWindowKind.enum.weekly,
    absent: 0.15,
    utilization: {
      low: 0,
      high: 100,
      step: 0.5
    },
    resetsIn: {
      low: HOUR,
      high: 7 * DAY,
      step: 10 * MINUTE
    }
  },
  {
    kind: UsageWindowKind.enum.model,
    absent: 0.7,
    utilization: {
      low: 0,
      high: 100,
      step: 1
    },
    resetsIn: {
      low: DAY,
      high: DAY,
      step: DAY
    }
  }
];

function scatteredWindows() {
  const windows = SCATTERED_WINDOWS.filter(draw => random() > draw.absent).map(draw => scatteredWindow(draw));
  return random() > REVERSED_WINDOWS_THRESHOLD ? windows.toReversed() : windows;
}

/** An account with both paced windows (in either order, now and then one of them
 *  missing, now and then a per-model window beside them) drawn at random. */
function scatteredPaceInputs() {
  return Array.from(
    { length: SCATTERED_CASE_COUNT },
    () =>
      ({
        windows: scatteredWindows(),
        rates: {
          [PacedWindowKind.enum.session]: pick(SCATTERED_SESSION_RATES),
          [PacedWindowKind.enum.weekly]: pick(SCATTERED_WEEKLY_RATES)
        },
        isPaused: random() > PAUSED_THRESHOLD
      }) satisfies PaceInputs
  );
}

function assessmentOf(inputs: PaceInputs) {
  const binding = assessPace({
    ...paceParameters(inputs),
    isPaused: inputs.isPaused
  });
  if (binding === null) {
    return null;
  }

  return {
    kind: binding.kind,
    rate: binding.rate,
    keep: binding.keep,
    readyAt: binding.readyAt,
    continueAt: binding.continueAt
  };
}

/** Every other forecast case has a day of readings behind its weekly window. */
const DAILY_RATE = perHour(0.41);

function dailyRateOfCase(index: number) {
  return index % 2 === 0 ? null : DAILY_RATE;
}

function forecastsOf({ inputs, dailyRate }: { inputs: PaceInputs; dailyRate: number | null }) {
  return paceForecasts({
    ...paceParameters(inputs),
    dailyRateOf: () => dailyRate
  });
}

const BURN_RESETS = [
  "2026-11-14T00:59:59.533870+00:00",
  "2026-11-14T00:59:59.601234+00:00",
  "2026-11-14T05:59:59.533870+00:00",
  "reset",
  null
];

/** A run of readings: now and then idle gaps, falling readings, a new window. */
function burnSamples(drawn: number) {
  const resets = drawn % 3 === 0 ? BURN_RESETS : BURN_RESETS.slice(0, 2);
  const length = 2 + Math.floor(random() * 18);
  const samples: UsageSample[] = [];
  let takenAt = NOW;
  let utilization = between({
    low: BURN_START_LOW,
    high: BURN_START_HIGH,
    step: BURN_START_STEP
  });
  for (let i = 0; i < length; i += 1) {
    samples.push({
      at: takenAt,
      utilization,
      resetsAt: pick(resets)
    });
    takenAt += pick(BURN_GAPS);
    utilization += between({
      low: BURN_DELTA_LOW,
      high: 2,
      step: BURN_DELTA_STEP
    });
  }

  return samples;
}

const FORECAST_CASE_COUNT = 40;

const OVER_PACE_SOURCE_COUNT = 12;

function forecastFixtures(grid: PaceInputs[]) {
  const cases = grid.slice(-FORECAST_CASE_COUNT).map(inputs => {
    const index = grid.indexOf(inputs);
    const dailyRate = dailyRateOfCase(index);
    return {
      index,
      dailyRate,
      forecasts: forecastsOf({
        inputs,
        dailyRate
      })
    };
  });
  const overPace = cases
    .slice(0, OVER_PACE_SOURCE_COUNT)
    .flatMap(({ forecasts: byKind }) => [byKind.session, byKind.weekly])
    .filter((forecast): forecast is PaceForecast => forecast !== undefined)
    .map(forecast => ({
      forecast,
      expected: isForecastOverPace(forecast)
    }));
  return {
    cases,
    overPace
  };
}

function restFixtures() {
  const keeps = [-0.2, 0, 0.04, 0.1, 0.25, 0.5, 0.8, 0.95, 1, 1.4];
  const rhythms = keeps.map(duty => ({
    duty,
    expected: workRhythm(duty)
  }));
  const rests = keeps.flatMap(keep =>
    [8 * MINUTE, 12 * HOUR].map(readyOffset => ({
      keep,
      readyAt: NOW + readyOffset,
      now: NOW,
      expected: slowRestUntil({
        keep,
        readyAt: NOW + readyOffset,
        now: NOW
      })
    }))
  );
  return {
    rhythms,
    rests
  };
}

function projectedRateFixtures() {
  const measuredRates = [null, perHour(1), perHour(4)];
  const ratesPerLoad = [null, perHour(0.01), perHour(0.05)];
  const loads = [0, 40, 160];
  return measuredRates.flatMap(measured =>
    ratesPerLoad.flatMap(ratePerLoad =>
      loads.map(load => ({
        measured,
        ratePerLoad,
        load,
        expected: projectedRate({
          measured,
          ratePerLoad,
          load
        })
      }))
    )
  );
}

export function usagePaceFixture() {
  const grid = [
    ...gridOf({
      kind: PacedWindowKind.enum.session,
      resetsIn: SESSION_RESETS_IN,
      rates: SESSION_RATES
    }),
    ...gridOf({
      kind: PacedWindowKind.enum.weekly,
      resetsIn: WEEKLY_RESETS_IN,
      rates: WEEKLY_RATES
    }),
    ...keepBoundaryCases(),
    ...scatteredPaceInputs()
  ];
  const forecasts = forecastFixtures(grid);
  const rest = restFixtures();
  return {
    now: NOW,
    paceCases: grid.map(inputs => ({
      inputs,
      assessPace: assessmentOf(inputs),
      hasRoomToSpare: hasRoomToSpare(paceParameters(inputs))
    })),
    paceForecasts: forecasts.cases,
    isForecastOverPace: forecasts.overPace,
    burnRate: Array.from({ length: BURN_RATE_CASE_COUNT }, (_, drawn) => burnSamples(drawn)).map(samples => ({
      samples,
      expected: burnRate(samples)
    })),
    workRhythm: rest.rhythms,
    slowRest: rest.rests,
    projectedRate: projectedRateFixtures()
  };
}
