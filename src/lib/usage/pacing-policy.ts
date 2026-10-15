// Usage pacing's policy - the steps it may take and the presets that pick them.
// Pure, so the Config panel and the pacing store read one definition.
//
// While an agent stays on track to run out of usage before its window resets,
// pacing climbs these steps one at a time, in this order: turn its effort down,
// move it to a cheaper model, give it short rests between stretches of work,
// and stop it until there's room. Each step undoes itself as room comes back,
// and all of it reverts as the window is about to reset. A preset is just a set
// of steps; any other set reads as Custom.

import { PacingStep, PacingStopThen } from "@/lib/types";
import type { PacingSettings, UsageWindowKind } from "@/lib/types";
import { resetCountdown } from "@/lib/usage/usage-groups";
import { isForecastOverPace, windowName } from "@/lib/usage/usage-pace";
import type { paceForecast } from "@/lib/usage/usage-pace";
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

/** How early a forecast run-out has to be before pacing acts, in minutes. */
export const PACING_EARLY_OPTIONS = [15, 30, 60, 120] as const;
const DEFAULT_EARLY_MINUTES = 30;

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

/** The settings pacing runs on until the user changes them: Balanced. */
export const DEFAULT_PACING: PacingSettings = {
  isOn: true,
  steps: [...PRESET_STEPS[PacingPreset.Balanced]],
  stopThen: PacingStopThen.enum.ask,
  earlyMinutes: DEFAULT_EARLY_MINUTES
};

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
    const runsOut = resetCountdown({
      iso: new Date(verdict.runOutAt).toISOString(),
      now
    });
    facts.push(`${windowName(verdict.windowKind)} window at ${formatPercent(verdict.utilization)} - on track to run out ${runsOut}, before its reset`);
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

/** The Config panel's live line for an account's forecast (`paceForecast`):
 *  the window, how much is spent, when it runs out and resets, and what pacing
 *  would do about it right now. A null forecast is an agent not yet watched. */
export function pacingPreview({ forecast, settings, now }: {
  forecast: ReturnType<typeof paceForecast>;
  settings: PacingSettings;
  now: number;
}) {
  if (forecast === null) {
    return {
      meter: null,
      wouldDo: "Measures the pace while the agent works"
    };
  }

  const countdown = (time: number) => resetCountdown({
    iso: new Date(time).toISOString(),
    now
  });
  const meter = {
    name: `${windowName(forecast.window.kind)} window`,
    utilization: forecast.window.utilization,
    detail: `${formatPercent(forecast.window.utilization)} · ${runOutText({
      forecast,
      countdown
    })} · resets ${countdown(forecast.resetAt)}`
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

function runOutText({ forecast, countdown }: {
  forecast: NonNullable<ReturnType<typeof paceForecast>>;
  countdown: (time: number) => string;
}) {
  if (!forecast.isMeasured) {
    return "pace not measured yet";
  }

  return forecast.runOutAt === null ? "nothing spent lately" : `runs out ${countdown(forecast.runOutAt)}`;
}

function wouldDoText({ forecast, settings, now }: {
  forecast: NonNullable<ReturnType<typeof paceForecast>>;
  settings: PacingSettings;
  now: number;
}) {
  if (!forecast.isMeasured) {
    return "Measuring the pace - it can act after about ten minutes of work";
  }

  const isOverPace = isForecastOverPace({
    forecast,
    earlyMs: settings.earlyMinutes * MILLISECONDS_PER_MINUTE,
    now
  });
  const firstStep = allowedSteps(settings)[0];
  return isOverPace && firstStep !== undefined
    ? `Would now: ${PACING_STEP_TEXT[firstStep].label.toLowerCase()}`
    : "On pace - nothing to do";
}
