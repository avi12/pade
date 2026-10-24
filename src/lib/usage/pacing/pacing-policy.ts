// Usage pacing's policy - the steps it may take and the presets that pick them.
// Pure, so the Config panel and the pacing store read one definition.
//
// While an agent stays on track to run out of usage before its window resets,
// pacing climbs these steps one at a time, in this order: turn its effort down,
// move it to a cheaper model, give it short rests between stretches of work,
// and stop it until there's room. Each step undoes itself as room comes back,
// and all of it reverts as the window is about to reset. A preset is just a set
// of steps; any other set reads as Custom.

import { PacedWindowKind, PacingStep, PacingStopThen } from "@/lib/types";
import type { PacingByWindow, PacingSettings, UsageWindowKind } from "@/lib/types";
import { isForecastOverPace, windowName } from "@/lib/usage/pacing/usage-pace";
import type { MarginOf, PaceForecast } from "@/lib/usage/pacing/usage-pace";
import { resetCountdown } from "@/lib/usage/usage-groups";
import { formatPercent } from "@/lib/util/format";

/** Every step, in the order pacing climbs them. */
export const PACING_STEP_ORDER = [
  PacingStep.enum.effort,
  PacingStep.enum.model,
  PacingStep.enum.rest,
  PacingStep.enum.stop
] as const;

/** How the Config panel names each step and says what it does. */
export const PACING_STEP_TEXT: Record<PacingStep, {
  label: string;
  hint: string;
}> = {
  [PacingStep.enum.effort]: {
    label: "Lower effort",
    hint: "one level at a time"
  },
  [PacingStep.enum.model]: {
    label: "Cheaper model",
    hint: "e.g. opus → sonnet"
  },
  [PacingStep.enum.rest]: {
    label: "Rest between work",
    hint: "short breaks sized to the pace"
  },
  [PacingStep.enum.stop]: {
    label: "Stop until there's room",
    hint: "it always continues just before the reset"
  }
};

const MINUTES_PER_HOUR = 60;
const MINUTES_PER_DAY = 24 * MINUTES_PER_HOUR;

/** How far ahead of pace a window may run before pacing acts, in minutes, per
 *  window - a session is judged in minutes, a week in hours and days. */
export const PACING_EARLY_OPTIONS: Record<PacedWindowKind, readonly number[]> = {
  [PacedWindowKind.enum.session]: [15, 30, 60, 120],
  [PacedWindowKind.enum.weekly]: [6 * MINUTES_PER_HOUR, 12 * MINUTES_PER_HOUR, MINUTES_PER_DAY, 2 * MINUTES_PER_DAY]
};

/** A margin as the Config panel names it - "30 min", "6 h", "1 d". */
export function marginLabel(minutes: number) {
  if (minutes < MINUTES_PER_HOUR) {
    return `${minutes} min`;
  }

  return minutes < MINUTES_PER_DAY ? `${minutes / MINUTES_PER_HOUR} h` : `${minutes / MINUTES_PER_DAY} d`;
}

/** How the Config panel heads each window, and how its trigger reads. */
export const PACED_WINDOW_TEXT: Record<PacedWindowKind, {
  title: string;
  hint: string;
  trigger: string;
  triggerEnd: string;
}> = {
  [PacedWindowKind.enum.session]: {
    title: "Session window",
    hint: "Judged by how fast it burns while agents work",
    trigger: "Act when on track to run out",
    triggerEnd: "early"
  },
  [PacedWindowKind.enum.weekly]: {
    title: "Weekly window",
    hint: "Judged against an even spend over the week",
    trigger: "Act when ahead of an even spend by",
    triggerEnd: ""
  }
};

export const PacingPreset = {
  Off: "off",
  Gentle: "gentle",
  Balanced: "balanced",
  Strict: "strict",
  Custom: "custom"
} as const;
export type PacingPreset = (typeof PacingPreset)[keyof typeof PacingPreset];

/** The presets that name a set of steps. */
const NAMED_PRESETS = [PacingPreset.Gentle, PacingPreset.Balanced, PacingPreset.Strict] as const;

/** The steps each named preset takes. */
const PRESET_STEPS: Record<(typeof NAMED_PRESETS)[number], readonly PacingStep[]> = {
  [PacingPreset.Gentle]: [PacingStep.enum.effort],
  [PacingPreset.Balanced]: [PacingStep.enum.effort, PacingStep.enum.model, PacingStep.enum.rest],
  [PacingPreset.Strict]: PACING_STEP_ORDER
};

/** The settings each window is paced on until the user changes them: Balanced,
 *  with a margin that suits the window. */
export const DEFAULT_PACING: PacingByWindow = {
  [PacedWindowKind.enum.session]: {
    isOn: true,
    steps: [...PRESET_STEPS[PacingPreset.Balanced]],
    stopThen: PacingStopThen.enum.ask,
    earlyMinutes: 30
  },
  [PacedWindowKind.enum.weekly]: {
    isOn: true,
    steps: [...PRESET_STEPS[PacingPreset.Balanced]],
    stopThen: PacingStopThen.enum.ask,
    earlyMinutes: 12 * MINUTES_PER_HOUR
  }
};

/** Whether pacing watches any window at all. */
export function isPacingOn(pacing: PacingByWindow) {
  return PacedWindowKind.options.some(kind => pacing[kind].isOn);
}

/** Every preset with what it does, for the Config panel's strength row. */
export const PACING_PRESETS = [
  {
    value: PacingPreset.Off,
    label: "Off",
    hint: "Never paces an agent"
  },
  {
    value: PacingPreset.Gentle,
    label: "Gentle",
    hint: "Turns the effort down, nothing more"
  },
  {
    value: PacingPreset.Balanced,
    label: "Balanced",
    hint: "Turns the effort down, then the model, then adds rests"
  },
  {
    value: PacingPreset.Strict,
    label: "Strict",
    hint: "Every step, ending in a stop"
  }
] as const;

/** A step set in climbing order, without repeats. */
function ordered(steps: readonly PacingStep[]) {
  return PACING_STEP_ORDER.filter(step => steps.includes(step));
}

/** Which preset the settings amount to - Custom when the steps match none. */
export function pacingPreset(settings: PacingSettings) {
  if (!settings.isOn) {
    return PacingPreset.Off;
  }

  const chosen = ordered(settings.steps).join();
  const named = NAMED_PRESETS.find(preset => ordered(PRESET_STEPS[preset]).join() === chosen);
  return named ?? PacingPreset.Custom;
}

/** The settings a preset picks, keeping what it doesn't decide. */
export function withPreset({ settings, preset }: {
  settings: PacingSettings;
  preset: PacingPreset;
}) {
  if (preset === PacingPreset.Off) {
    return {
      ...settings,
      isOn: false
    };
  }

  if (preset === PacingPreset.Custom) {
    return {
      ...settings,
      isOn: true
    };
  }

  return {
    ...settings,
    isOn: true,
    steps: [...PRESET_STEPS[preset]],
    stopThen: preset === PacingPreset.Strict ? PacingStopThen.enum.ask : settings.stopThen
  };
}

/** The settings with one step turned on or off. */
export function withStep({ settings, step, isAllowed }: {
  settings: PacingSettings;
  step: PacingStep;
  isAllowed: boolean;
}) {
  const others = settings.steps.filter(candidate => candidate !== step);
  return {
    ...settings,
    steps: ordered(isAllowed ? [...others, step] : others)
  };
}

/** The allowed steps, in climbing order. */
export function allowedSteps(settings: PacingSettings) {
  return ordered(settings.steps);
}

const MILLISECONDS_PER_MINUTE = 60_000;

/** How far ahead of pace each window may run, from its settings - null for a
 *  window whose pacing is off. */
export function pacingMargins(pacing: PacingByWindow): MarginOf {
  return kind => pacing[kind].isOn ? pacing[kind].earlyMinutes * MILLISECONDS_PER_MINUTE : null;
}

const CLOCK = new Intl.DateTimeFormat(undefined, { timeStyle: "short" });
const DAY = new Intl.DateTimeFormat(undefined, { dateStyle: "short" });
const DAY_AND_CLOCK = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  hour: "numeric",
  minute: "2-digit"
});

/** A time as pacing says it - the clock alone today, with the weekday after. */
export function spokenTime({ time, now }: {
  time: number;
  now: number;
}) {
  const isToday = DAY.format(time) === DAY.format(now);
  return isToday ? CLOCK.format(time) : DAY_AND_CLOCK.format(time);
}

/** The facts a paced session's popover lists: why it is held (its window and
 *  when it would run out), what it runs instead of its own choice, how long it
 *  rests, and when it all reverts. */
export function pacingFacts({ verdict, choice, restUntil, revertsAt, now }: {
  verdict: {
    windowKind: UsageWindowKind;
    utilization: number;
    runOutAt: number;
    readyAt: number;
  } | null;
  choice: {
    current: string;
    original: string;
  } | null;
  restUntil: number | null;
  revertsAt: number | null;
  now: number;
}) {
  const facts: string[] = [];
  if (verdict !== null) {
    facts.push(
      verdictText({
        verdict,
        now
      })
    );
  }

  if (choice !== null) {
    facts.push(`Runs ${choice.current} - normally ${choice.original}`);
  }

  if (restUntil !== null) {
    facts.push(
      `Resting until ${spokenTime({
        time: restUntil,
        now
      })}`
    );
  }

  if (revertsAt !== null) {
    facts.push(
      `Reverts at ${spokenTime({
        time: revertsAt,
        now
      })}`
    );
  }

  return facts;
}

/** Why a window holds a session: a session window runs out before its reset;
 *  a weekly one spends ahead of an even week. */
function verdictText({ verdict, now }: {
  verdict: NonNullable<Parameters<typeof pacingFacts>[0]["verdict"]>;
  now: number;
}) {
  const spent = `${windowName(verdict.windowKind)} window at ${formatPercent(verdict.utilization)}`;
  if (verdict.windowKind === PacedWindowKind.enum.weekly) {
    return `${spent} - ahead of an even spend, back on pace ${countdownTo({
      time: verdict.readyAt,
      now
    })}`;
  }

  return `${spent} - on track to run out ${countdownTo({
    time: verdict.runOutAt,
    now
  })}, before its reset`;
}

/** How long until `time`, as the usage meter says it - "in 2h 5m". */
function countdownTo({ time, now }: {
  time: number;
  now: number;
}) {
  return resetCountdown({
    iso: new Date(time).toISOString(),
    now
  });
}

/** The Config panel's live line for one window's forecast (`paceForecasts`):
 *  the window, how much is spent (and, weekly, where an even spend would be),
 *  when it runs out and resets, and what pacing would do about it right now. A
 *  null forecast is a window not yet watched. */
export function pacingPreview({ forecast, settings, now }: {
  forecast: PaceForecast | null;
  settings: PacingSettings;
  now: number;
}) {
  if (forecast === null) {
    return {
      meter: null,
      wouldDo: "Measures the pace while the agent works"
    };
  }

  const parts = [formatPercent(forecast.window.utilization)];
  if (forecast.evenPercentage !== null) {
    parts.push(`even spend ${formatPercent(forecast.evenPercentage)}`);
  }

  parts.push(
    runOutText({
      forecast,
      now
    }), `resets ${countdownTo({
      time: forecast.resetAt,
      now
    })}`
  );
  const meter = {
    name: `${windowName(forecast.window.kind)} window`,
    utilization: forecast.window.utilization,
    evenPercentage: forecast.evenPercentage,
    detail: parts.join(" · ")
  };
  return {
    meter,
    wouldDo: wouldDoText({
      forecast,
      settings,
      now
    })
  };
}

function runOutText({ forecast, now }: {
  forecast: PaceForecast;
  now: number;
}) {
  if (!forecast.isMeasured) {
    return "pace not measured yet";
  }

  if (forecast.runOutAt === null) {
    return "nothing spent lately";
  }

  return `runs out ${countdownTo({
    time: forecast.runOutAt,
    now
  })}`;
}

function wouldDoText({ forecast, settings, now }: {
  forecast: PaceForecast;
  settings: PacingSettings;
  now: number;
}) {
  if (!forecast.isMeasured) {
    return "Measuring the pace - it can act after about ten minutes of work";
  }

  const isOverPace = isForecastOverPace({
    forecast,
    margin: settings.earlyMinutes * MILLISECONDS_PER_MINUTE,
    now
  });
  const firstStep = allowedSteps(settings)[0];
  return isOverPace && firstStep !== undefined
    ? `Would now: ${PACING_STEP_TEXT[firstStep].label.toLowerCase()}`
    : "On pace - nothing to do";
}
