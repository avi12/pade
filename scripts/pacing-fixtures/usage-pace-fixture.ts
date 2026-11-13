// The usage-pace fixture: a grid and a seeded scatter of windows, forecasts, rest
// and weekly-window cases swept through `usage-pace` and `pacing-policy`.

import { PacedWindowKind } from "@/lib/types/pacing";
import { UsageWindowKind } from "@/lib/types/usage";
import type { AccountUsage, UsageWindow } from "@/lib/types/usage";
import { pacingMargin } from "@/lib/usage/pacing/engine/pacing-policy";
import {
  assessPace,
  burnRate,
  evenSpendReachedAt,
  hasRoomToSpare,
  isForecastOverPace,
  isRestEnough,
  paceForecasts,
  slowRestUntil,
  workingRunway,
  workRhythm
} from "@/lib/usage/pacing/engine/usage-pace";
import type { PaceForecast, UsageSample } from "@/lib/usage/pacing/engine/usage-pace";

import { between, DAY, HOUR, MINUTE, NOW, perHour, pick, random } from "./fixture-primitives";

export function windowOf({
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

/** What one pace case feeds the formula: the account's windows, the burn the
 *  agents measure per paced kind, the user's margin per paced kind. */
interface PaceInputs {
  windows: UsageWindow[];
  rates: PerKind;
  margins: PerKind;
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
    marginOf: (kind: PacedWindowKind) => inputs.margins[kind],
    rateOf: (window: UsageWindow) =>
      lookup({
        window,
        perKind: inputs.rates
      })
  };
}

const NO_PACING: PerKind = {
  [PacedWindowKind.enum.session]: null,
  [PacedWindowKind.enum.weekly]: null
};

/** One account with a single paced window, the other kind switched off. */
function singleWindowCase({
  kind,
  utilization,
  resetsIn,
  rate,
  margin,
  isPaused
}: {
  kind: PacedWindowKind;
  utilization: number;
  resetsIn: number;
  rate: number | null;
  margin: number;
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
      ...NO_PACING,
      [kind]: rate
    },
    margins: {
      ...NO_PACING,
      [kind]: margin
    },
    isPaused
  } satisfies PaceInputs;
}

const SESSION_RESETS_IN = [9 * MINUTE, 30 * MINUTE, 90 * MINUTE];

const WEEKLY_RESETS_IN = [30 * MINUTE, 2 * DAY, 3.5 * DAY, 6 * DAY];

const SESSION_RATES = [null, perHour(30), perHour(90)];

const BOTH_WAYS = [false, true];

const SESSION_UTILIZATIONS = [59, 75, 95];

const WEEKLY_UTILIZATIONS = [60, 75, 95];

const SESSION_GRID_MARGINS = [15 * MINUTE, 30 * MINUTE];

const SPARE_BOUNDARY_UTILIZATION = 70;

const SPARE_BOUNDARY_RESETS_IN = 2 * HOUR + 10 * MINUTE;

const SPARE_BOUNDARY_MARGIN = 30 * MINUTE;

const WEEKLY_BOUNDARY_CASE_COUNT = 21;

const WEEKLY_BOUNDARY_BASE_UTILIZATION = 50;

const WEEKLY_BOUNDARY_RESETS_IN = 3 * DAY;

const REVERSED_WINDOWS_THRESHOLD = 0.5;

const SCATTERED_CASE_COUNT = 70;

const SCATTERED_SESSION_RATES = [null, perHour(10), perHour(30), perHour(60)];

const SCATTERED_WEEKLY_RATES = [null, perHour(1), perHour(4)];

const SCATTERED_SESSION_MARGINS = [null, 15 * MINUTE, 30 * MINUTE];

const SCATTERED_WEEKLY_MARGINS = [null, HOUR, 6 * HOUR, 12 * HOUR];

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

function sessionRateCases({ utilization, resetsIn }: { utilization: number; resetsIn: number }) {
  return SESSION_RATES.flatMap(rate =>
    SESSION_GRID_MARGINS.flatMap(margin =>
      bothPauseStates({
        kind: PacedWindowKind.enum.session,
        utilization,
        resetsIn,
        rate,
        margin
      })
    )
  );
}

function sessionGrid() {
  return SESSION_UTILIZATIONS.flatMap(utilization =>
    SESSION_RESETS_IN.flatMap(resetsIn =>
      sessionRateCases({
        utilization,
        resetsIn
      })
    )
  );
}

function weeklyGrid() {
  const margins = [HOUR, 12 * HOUR];
  return WEEKLY_UTILIZATIONS.flatMap(utilization =>
    WEEKLY_RESETS_IN.flatMap(resetsIn =>
      margins.flatMap(margin =>
        bothPauseStates({
          kind: PacedWindowKind.enum.weekly,
          utilization,
          resetsIn,
          rate: perHour(2),
          margin
        })
      )
    )
  );
}

/** The session rates around the one whose runway reaches 1.5 times the time
 *  left, where `hasRoomToSpare` flips. */
function spareBoundaryCases() {
  const ratesPerHour = Array.from({ length: 17 }, (_, step) => 8 + step * 0.25);
  return ratesPerHour.map(ratePerHour =>
    singleWindowCase({
      kind: PacedWindowKind.enum.session,
      utilization: SPARE_BOUNDARY_UTILIZATION,
      resetsIn: SPARE_BOUNDARY_RESETS_IN,
      rate: perHour(ratePerHour),
      margin: SPARE_BOUNDARY_MARGIN,
      isPaused: false
    })
  );
}

/** The weekly spends around an even spend three days from the reset, where the
 *  margin decides. */
function weeklyBoundaryCases() {
  return Array.from({ length: WEEKLY_BOUNDARY_CASE_COUNT }, (_, step) =>
    singleWindowCase({
      kind: PacedWindowKind.enum.weekly,
      utilization: WEEKLY_BOUNDARY_BASE_UTILIZATION + step,
      resetsIn: WEEKLY_BOUNDARY_RESETS_IN,
      rate: null,
      margin: HOUR,
      isPaused: false
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
      low: 40,
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
      low: 20,
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
        margins: {
          [PacedWindowKind.enum.session]: pick(SCATTERED_SESSION_MARGINS),
          [PacedWindowKind.enum.weekly]: pick(SCATTERED_WEEKLY_MARGINS)
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
    readyAt: binding.readyAt,
    continueAt: binding.continueAt
  };
}

function forecastsOf(inputs: PaceInputs) {
  const { account, now, rateOf } = paceParameters(inputs);
  return paceForecasts({
    account,
    now,
    rateOf
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
  const cases = grid.slice(-FORECAST_CASE_COUNT).map(inputs => ({
    index: grid.indexOf(inputs),
    forecasts: forecastsOf(inputs)
  }));
  const forecasts = cases
    .slice(0, OVER_PACE_SOURCE_COUNT)
    .flatMap(({ forecasts: byKind }) => [byKind.session, byKind.weekly])
    .filter((forecast): forecast is PaceForecast => forecast !== undefined);
  const overPace = forecasts.flatMap(forecast =>
    [15 * MINUTE, 6 * HOUR].map(margin => ({
      forecast,
      margin,
      expected: isForecastOverPace({
        forecast,
        margin,
        now: NOW
      })
    }))
  );
  return {
    cases,
    overPace
  };
}

function restFixtures() {
  const runways = [0, 6 * MINUTE, HOUR, 3 * HOUR, 5 * HOUR, 9 * HOUR];
  const rhythms = runways.flatMap(runway =>
    [0, 2 * HOUR, 4 * HOUR, 10 * HOUR].map(timeLeft => ({
      runway,
      timeLeft,
      expected: workRhythm({
        runway,
        timeLeft
      })
    }))
  );
  const continueAt = NOW + 4 * HOUR;
  const rests = runways.flatMap(runway =>
    [8 * MINUTE, 12 * HOUR].map(readyOffset => ({
      runway,
      readyAt: NOW + readyOffset,
      continueAt,
      now: NOW,
      expectedRestUntil: slowRestUntil({
        runway,
        readyAt: NOW + readyOffset,
        continueAt,
        now: NOW
      }),
      expectedIsRestEnough: isRestEnough({
        runway,
        continueAt,
        now: NOW
      })
    }))
  );
  return {
    rhythms,
    rests
  };
}

function weeklyWindowFixtures() {
  const windows = [0, 30, 76, 100].flatMap(utilization =>
    [HOUR, 2 * DAY, 6 * DAY].map(resetsIn =>
      windowOf({
        kind: UsageWindowKind.enum.weekly,
        utilization,
        resetsIn
      })
    )
  );
  const readyAt = NOW + 14 * HOUR;
  const continueAt = NOW + 2 * DAY;
  return {
    evenSpend: windows.map(window => ({
      window,
      expected: evenSpendReachedAt(window)
    })),
    runways: windows.flatMap(window =>
      [0, perHour(1), perHour(2)].map(rate => ({
        window,
        rate,
        readyAt,
        continueAt,
        expected: workingRunway({
          window,
          rate,
          readyAt,
          continueAt
        })
      }))
    )
  };
}

export function usagePaceFixture() {
  const grid = [
    ...sessionGrid(),
    ...spareBoundaryCases(),
    ...weeklyBoundaryCases(),
    ...weeklyGrid(),
    ...scatteredPaceInputs()
  ];
  const forecasts = forecastFixtures(grid);
  const rest = restFixtures();
  const weekly = weeklyWindowFixtures();
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
    evenSpendReachedAt: weekly.evenSpend,
    workingRunway: weekly.runways,
    workRhythm: rest.rhythms,
    slowRest: rest.rests,
    pacingMargin: Object.values(PacedWindowKind.enum).map(kind => ({
      kind,
      expected: pacingMargin(kind)
    }))
  };
}
