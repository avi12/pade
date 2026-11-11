// Usage pacing's policy - one switch, and one built-in way of pacing. Pure, so
// the Config panel and the pacing store read one definition.
//
// The goal: get as much done as possible without either usage limit running
// out before it resets. While an account is on track to run out early, pacing
// slows its agents only as much as that needs, mildest first: turn the effort
// down, move to a cheaper model, give short rests between stretches of work,
// and - right at the edge - stop until there's room. Each step undoes itself
// as room comes back, a stopped agent continues on its own, and everything
// reverts as the window is about to reset.

import { PacedWindowKind, PacingStep } from "@/lib/types";
import type { Prefs, UsageWindowKind } from "@/lib/types";
import { isForecastOverPace, windowName } from "@/lib/usage/pacing/usage-pace";
import type { PaceForecast } from "@/lib/usage/pacing/usage-pace";
import { resetCountdown } from "@/lib/usage/usage-groups";
import { formatCount, formatPercent } from "@/lib/util/format";

/** Every step, in the order pacing climbs them. */
export const PACING_STEP_ORDER = [
  PacingStep.enum.effort,
  PacingStep.enum.model,
  PacingStep.enum.rest,
  PacingStep.enum.stop
] as const;

/** How a paced session's popover names each step it has taken. */
export const PACING_STEP_TEXT: Record<PacingStep, string> = {
  [PacingStep.enum.effort]: "Lower effort",
  [PacingStep.enum.model]: "Cheaper model",
  [PacingStep.enum.rest]: "Rests between work",
  [PacingStep.enum.stop]: "Stopped until there's room"
};

export const MILLISECONDS_PER_MINUTE = 60_000;
const MINUTES_PER_HOUR = 60;

/** How far ahead of pace a window may run before pacing acts: a little slack,
 *  so a short burst doesn't slow anything, sized to the window - minutes for
 *  the 5-hour window, hours for the week. Near the reset it shrinks to a share
 *  of the time left (lib/usage/pacing/usage-pace). */
const EARLY_MINUTES: Record<PacedWindowKind, number> = {
  [PacedWindowKind.enum.session]: 30,
  [PacedWindowKind.enum.weekly]: 6 * MINUTES_PER_HOUR
};

/** Whether pacing runs - on unless the user turned it off. */
export function isPacingOn(prefs: Pick<Prefs, "paceUsage">) {
  return prefs.paceUsage !== false;
}

/** How far ahead of pace each window may run, in milliseconds. */
export function pacingMargin(kind: PacedWindowKind) {
  return EARLY_MINUTES[kind] * MILLISECONDS_PER_MINUTE;
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

/** The Config panel's live line for one window (`paceForecasts`): how much is
 *  spent and when it resets, when it runs out at this pace, and what pacing is
 *  doing about it. A null forecast is a window not yet read. */
export function pacingPreview({ kind, forecast, slowedCount, now }: {
  kind: PacedWindowKind;
  forecast: PaceForecast | null;
  /** Sessions pacing is slowing right now. */
  slowedCount: number;
  now: number;
}) {
  if (forecast === null) {
    return null;
  }

  return {
    name: windowName(forecast.window.kind),
    utilization: forecast.window.utilization,
    resets: `resets ${spokenTime({
      time: forecast.resetAt,
      now
    })}`,
    pace: paceText({
      forecast,
      now
    }),
    action: actionText({
      kind,
      forecast,
      slowedCount,
      now
    })
  };
}

function paceText({ forecast, now }: {
  forecast: PaceForecast;
  now: number;
}) {
  if (!forecast.isMeasured) {
    return "Measuring the pace";
  }

  if (forecast.runOutAt === null || forecast.runOutAt >= forecast.resetAt) {
    return "At this pace: lasts until the reset";
  }

  return `At this pace: runs out ${spokenTime({
    time: forecast.runOutAt,
    now
  })}`;
}

function actionText({ kind, forecast, slowedCount, now }: {
  kind: PacedWindowKind;
  forecast: PaceForecast;
  slowedCount: number;
  now: number;
}) {
  if (slowedCount > 0) {
    const sessions = slowedCount === 1 ? "1 session" : `${formatCount(slowedCount)} sessions`;
    return `Slowing ${sessions} so it lasts`;
  }

  const isOverPace = forecast.isMeasured && isForecastOverPace({
    forecast,
    margin: pacingMargin(kind),
    now
  });
  return isOverPace ? "Slowing down the next working session" : "On pace - nothing to slow";
}
