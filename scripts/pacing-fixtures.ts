// Golden fixtures for the Rust port of the usage-pacing math: it sweeps a grid of
// inputs (and a seeded random scatter) through the TypeScript functions and
// returns what they answer, so `src-tauri/src/pacing/golden_fixtures.rs` can hold
// the Rust functions to the same numbers. `pnpm pacing:fixtures` (from the repo
// root) writes them to `src-tauri/src/pacing/fixtures/`; run it whenever a pacing
// rule changes and commit the JSON it rewrites.
//
// Everything here is deterministic: a fixed clock and a seeded generator, so a
// rerun with the TypeScript unchanged rewrites byte-identical files.

import { PacedWindowKind, SubagentStepKind, UsageWindowKind } from "@/lib/types";
import type { AccountUsage, SubagentStep, UsageWindow } from "@/lib/types";
import { planRound } from "@/lib/usage/pacing/account-round";
import type { SessionReport, SubagentSteps, WindowReport } from "@/lib/usage/pacing/account-round";
import { costWeight, planShedding, shedFraction } from "@/lib/usage/pacing/load-shedding";
import type { ShedCandidate } from "@/lib/usage/pacing/load-shedding";
import { pacingMargin } from "@/lib/usage/pacing/pacing-policy";
import { easedSubagentStep, holdForSaving, nextSubagentStep } from "@/lib/usage/pacing/subagent-steps";
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
} from "@/lib/usage/pacing/usage-pace";
import type { PaceForecast, UsageSample } from "@/lib/usage/pacing/usage-pace";

const NOW = Date.UTC(2026, 9, 15, 12);
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const SEED = 0x9ace_2026;

/** A burn rate in the formula's units, from percent per hour. */
function perHour(percentage: number) {
  return percentage / HOUR;
}

/** A small seeded generator (mulberry32), so a rerun draws the same scatter. */
function seededRandom(seed: number) {
  let state = seed;
  return () => {
    state = (state + 0x6d2b_79f5) | 0;
    let mixed = Math.imul(state ^ (state >>> 15), 1 | state);
    mixed = (mixed + Math.imul(mixed ^ (mixed >>> 7), 61 | mixed)) ^ mixed;
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4_294_967_296;
  };
}

const random = seededRandom(SEED);

function pick<Item>(items: readonly Item[]) {
  const [item] = items.slice(Math.floor(random() * items.length));
  if (item === undefined) {
    throw new Error("Nothing to pick from");
  }

  return item;
}

/** A uniform draw rounded to a step, so the JSON stays short. */
function between({ low, high, step }: {
  low: number;
  high: number;
  step: number;
}) {
  return Math.round((low + random() * (high - low)) / step) * step;
}

function windowOf({ kind, utilization, resetsIn }: {
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

function lookup({ window, perKind }: {
  window: UsageWindow;
  perKind: PerKind;
}) {
  const kind = PacedWindowKind.safeParse(window.kind);
  return kind.success ? perKind[kind.data] : null;
}

function paceParameters(inputs: PaceInputs) {
  return {
    account: accountOf(inputs.windows),
    now: NOW,
    marginOf: (kind: PacedWindowKind) => inputs.margins[kind],
    rateOf: (window: UsageWindow) => lookup({
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
function singleWindowCase({ kind, utilization, resetsIn, rate, margin, isPaused }: {
  kind: PacedWindowKind;
  utilization: number;
  resetsIn: number;
  rate: number | null;
  margin: number;
  isPaused: boolean;
}) {
  return {
    windows: [windowOf({
      kind,
      utilization,
      resetsIn
    })],
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

function sessionGrid() {
  const margins = [15 * MINUTE, 30 * MINUTE];
  return [59, 75, 95].flatMap(
    utilization => SESSION_RESETS_IN.flatMap(
      resetsIn => SESSION_RATES.flatMap(
        rate => margins.flatMap(
          margin => BOTH_WAYS.map(isPaused => singleWindowCase({
            kind: PacedWindowKind.enum.session,
            utilization,
            resetsIn,
            rate,
            margin,
            isPaused
          }))
        )
      )
    )
  );
}

function weeklyGrid() {
  const margins = [HOUR, 12 * HOUR];
  return [60, 75, 95].flatMap(
    utilization => WEEKLY_RESETS_IN.flatMap(
      resetsIn => margins.flatMap(
        margin => BOTH_WAYS.map(isPaused => singleWindowCase({
          kind: PacedWindowKind.enum.weekly,
          utilization,
          resetsIn,
          rate: perHour(2),
          margin,
          isPaused
        }))
      )
    )
  );
}

/** The session rates around the one whose runway reaches 1.5 times the time
 *  left, where `hasRoomToSpare` flips. */
function spareBoundaryCases() {
  const ratesPerHour = Array.from({ length: 17 }, (_, step) => 8 + step * 0.25);
  return ratesPerHour.map(ratePerHour => singleWindowCase({
    kind: PacedWindowKind.enum.session,
    utilization: 70,
    resetsIn: 2 * HOUR + 10 * MINUTE,
    rate: perHour(ratePerHour),
    margin: 30 * MINUTE,
    isPaused: false
  }));
}

/** The weekly spends around an even spend three days from the reset, where the
 *  margin decides. */
function weeklyBoundaryCases() {
  return Array.from({ length: 21 }, (_, step) => singleWindowCase({
    kind: PacedWindowKind.enum.weekly,
    utilization: 50 + step,
    resetsIn: 3 * DAY,
    rate: null,
    margin: HOUR,
    isPaused: false
  }));
}

function scatteredWindow({ kind, utilization, resetsIn }: {
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
  const windows = SCATTERED_WINDOWS
    .filter(draw => random() > draw.absent)
    .map(draw => scatteredWindow(draw));
  return random() > 0.5 ? windows.toReversed() : windows;
}

/** An account with both paced windows (in either order, now and then one of them
 *  missing, now and then a per-model window beside them) drawn at random. */
function scatteredPaceInputs() {
  return Array.from({ length: 70 }, () => ({
    windows: scatteredWindows(),
    rates: {
      [PacedWindowKind.enum.session]: pick([null, perHour(10), perHour(30), perHour(60)]),
      [PacedWindowKind.enum.weekly]: pick([null, perHour(1), perHour(4)])
    },
    margins: {
      [PacedWindowKind.enum.session]: pick([null, 15 * MINUTE, 30 * MINUTE]),
      [PacedWindowKind.enum.weekly]: pick([null, HOUR, 6 * HOUR, 12 * HOUR])
    },
    isPaused: random() > 0.5
  } satisfies PaceInputs));
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
    low: 50,
    high: 80,
    step: 0.5
  });
  for (let i = 0; i < length; i += 1) {
    samples.push({
      at: takenAt,
      utilization,
      resetsAt: pick(resets)
    });
    takenAt += pick([30_000, MINUTE, MINUTE, 2 * MINUTE, 6 * MINUTE, 0]);
    utilization += between({
      low: -0.5,
      high: 2,
      step: 0.25
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
  const overPace = forecasts.flatMap(forecast => [15 * MINUTE, 6 * HOUR].map(margin => ({
    forecast,
    margin,
    expected: isForecastOverPace({
      forecast,
      margin,
      now: NOW
    })
  })));
  return {
    cases,
    overPace
  };
}

function restFixtures() {
  const runways = [0, 6 * MINUTE, HOUR, 3 * HOUR, 5 * HOUR, 9 * HOUR];
  const rhythms = runways.flatMap(runway => [0, 2 * HOUR, 4 * HOUR, 10 * HOUR].map(timeLeft => ({
    runway,
    timeLeft,
    expected: workRhythm({
      runway,
      timeLeft
    })
  })));
  const continueAt = NOW + 4 * HOUR;
  const rests = runways.flatMap(runway => [8 * MINUTE, 12 * HOUR].map(readyOffset => ({
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
  })));
  return {
    rhythms,
    rests
  };
}

function weeklyWindowFixtures() {
  const windows = [0, 30, 76, 100].flatMap(utilization => [HOUR, 2 * DAY, 6 * DAY].map(resetsIn => windowOf({
    kind: UsageWindowKind.enum.weekly,
    utilization,
    resetsIn
  })));
  const readyAt = NOW + 14 * HOUR;
  const continueAt = NOW + 2 * DAY;
  return {
    evenSpend: windows.map(window => ({
      window,
      expected: evenSpendReachedAt(window)
    })),
    runways: windows.flatMap(window => [0, perHour(1), perHour(2)].map(rate => ({
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
    })))
  };
}

function usagePaceFixture() {
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
    burnRate: Array.from({ length: 30 }, (_, drawn) => burnSamples(drawn)).map(samples => ({
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

const MODELS = [
  "claude-haiku-5-5",
  "claude-sonnet-5-5",
  "claude-opus-5-5[1m]",
  "claude-fable-5-1",
  "anthropic/claude-opus-5",
  "gpt-5.6-sol",
  "gpt-5-mini",
  "mystery-model"
];
const EFFORTS = [undefined, "minimal", "low", "medium", "high", "xhigh", "max", "HIGH", "turbo"];
const LADDERS: string[][] = [[], ["low", "high"]];

function costWeightCases() {
  return MODELS.flatMap(model => EFFORTS.flatMap(effort => LADDERS.map(ladder => ({
    model,
    effort: effort ?? null,
    ladder,
    expected: costWeight({
      model,
      effort,
      ladder
    })
  }))));
}

function shedFractionCases() {
  const rates = [null, 0, perHour(1), perHour(10), perHour(60)];
  const timesLeft = [10 * MINUTE, 100 * MINUTE, 5 * HOUR];
  return [60, 80, 95].flatMap(utilization => timesLeft.flatMap(timeLeft => rates.map(rate => ({
    utilization,
    timeLeft,
    rate,
    expected: shedFraction({
      window: windowOf({
        kind: UsageWindowKind.enum.session,
        utilization,
        resetsIn: timeLeft
      }),
      rate,
      freeAt: NOW + timeLeft,
      now: NOW
    })
  }))));
}

function planSheddingCases() {
  return Array.from({ length: 30 }, () => {
    const candidates: ShedCandidate[] = Array.from({ length: 1 + Math.floor(random() * 5) }, (_, i) => ({
      id: `candidate-${i}`,
      load: between({
        low: 1,
        high: 60,
        step: 0.5
      }),
      saving: pick([0, 0.2, 0.4, 0.5, 1])
    }));
    const totalLoad = candidates.reduce((sum, candidate) => sum + candidate.load, 0) + between({
      low: 0,
      high: 30,
      step: 1
    });
    const fraction = pick([null, 0.1, 0.35, 0.6, 0.95]);
    return {
      candidates,
      totalLoad,
      fraction,
      expected: planShedding({
        candidates,
        totalLoad,
        fraction
      })
    };
  });
}

function loadSheddingFixture() {
  return {
    costWeight: costWeightCases(),
    shedFraction: shedFractionCases(),
    planShedding: planSheddingCases()
  };
}

function gate(holdSeconds: number) {
  return {
    step: SubagentStepKind.enum.gate,
    holdSeconds
  } satisfies SubagentStep;
}

const WRAP_UP = { step: SubagentStepKind.enum.wrapUp } satisfies SubagentStep;

function subagentStepsFixture() {
  const holds = [5, 10, 20, 25, 40, 44, 45];
  const steps: SubagentStep[] = [...holds.map(gate), WRAP_UP];
  const currents = [null, ...steps];
  return {
    holdForSaving: Array.from({ length: 101 }, (_, i) => i / 100).map(fraction => ({
      fraction,
      expected: holdForSaving(fraction)
    })),
    nextSubagentStep: currents.flatMap(current => [null, 0, 0.1, 0.5, 0.99, 1].map(shed => ({
      current,
      shed,
      expected: nextSubagentStep({
        current: current ?? undefined,
        shed
      })
    }))),
    easedSubagentStep: steps.map(current => ({
      current,
      expected: easedSubagentStep(current)
    }))
  };
}

const ACCOUNTS = ["avi@example.com", "someone@example.com"];
const WINDOW_LABELS = ["main", "window-1", "window-2"];

function sessionReportOf(id: string): SessionReport {
  const isOverPace = random() > 0.45;
  return {
    id,
    name: id,
    accountKey: pick(ACCOUNTS),
    command: "claude",
    conversationId: pick([null, `${id}-conversation`]),
    reportedAt: NOW - pick([0, 0, 30_000, 120_000]),
    load: between({
      low: 0,
      high: 60,
      step: 0.5
    }),
    overPace: isOverPace ? {
      shed: pick([null, 0.1, 0.25, 0.3, 0.6, 0.9]),
      saving: pick([0, 0.2, 0.5, 1])
    } : null,
    canStepUp: random() > 0.6,
    hasRoom: random() > 0.5,
    subagents: Array.from({ length: Math.floor(random() * 4) }, (_, i) => ({
      id: `${id}-agent-${i}`,
      description: pick([null, "Explore the repo", "Run the tests"]),
      load: between({
        low: 0,
        high: 40,
        step: 0.5
      })
    }))
  };
}

function windowReportsOf() {
  const windowCount = 1 + Math.floor(random() * 3);
  return Array.from({ length: windowCount }, (_, windowIndex): WindowReport => ({
    label: pick(WINDOW_LABELS) + (windowIndex === 0 ? "" : `-${windowIndex}`),
    at: NOW,
    sessions: Array.from({ length: 1 + Math.floor(random() * 3) }, (_, sessionIndex) => sessionReportOf(`w${windowIndex}s${sessionIndex}`))
  }));
}

/** What an earlier round left behind: some subagents already stepped, some
 *  sessions stepped a moment ago. */
function priorRound(reports: WindowReport[]) {
  const steps: SubagentSteps = {};
  const steppedAt: Record<string, number> = {};
  for (const session of reports.flatMap(report => report.sessions)) {
    if (random() > 0.7) {
      steppedAt[session.id] = NOW - pick([10_000, 60_000]);
    }

    for (const subagent of session.subagents.filter(() => random() > 0.5)) {
      steps[`${session.id}/${subagent.id}`] = pick([gate(pick([5, 10, 20, 45])), WRAP_UP]);
    }
  }

  return {
    steps,
    steppedAt
  };
}

function roundScenarios() {
  return Array.from({ length: 24 }, () => {
    const reports = windowReportsOf();
    const { steps, steppedAt } = priorRound(reports);
    return {
      reports,
      steps,
      steppedAt,
      leader: "main",
      now: NOW,
      expected: planRound({
        reports,
        steps,
        steppedAt: new Map(Object.entries(steppedAt)),
        leader: "main",
        now: NOW
      })
    };
  });
}

/** Every fixture file's contents, by the name it is written under
 *  (`src-tauri/src/pacing/fixtures/<name>.json`, by `pacing-fixtures.mjs`). */
export const pacingFixtures = {
  usage_pace: usagePaceFixture(),
  load_shedding: loadSheddingFixture(),
  subagent_steps: subagentStepsFixture(),
  account_round: {
    scenarios: roundScenarios()
  }
};
