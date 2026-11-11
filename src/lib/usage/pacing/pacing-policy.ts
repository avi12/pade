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

/** One line pacing says about a session, with the time it ends on kept apart,
 *  so the popover can read that time either way (`pacingWhen`). */
interface PacingFact {
  text: string;
  time: number | null;
}

/** A time as a fact ends on: "at 2:49 PM", or as a countdown, "in 6h 59m". */
export function pacingWhen({ time, now, isCountdown }: {
  time: number;
  now: number;
  isCountdown: boolean;
}) {
  const countdown = resetCountdown({
    iso: new Date(time).toISOString(),
    now
  });
  if (isCountdown && countdown !== "") {
    return countdown;
  }

  return `at ${spokenTime({
    time,
    now
  })}`;
}

/** A fact as one plain line, its time as a clock time - for the tab badge's hint. */
export function pacingFactText({ fact, now }: {
  fact: PacingFact;
  now: number;
}) {
  if (fact.time === null) {
    return fact.text;
  }

  return `${fact.text} ${pacingWhen({
    time: fact.time,
    now,
    isCountdown: false
  })}`;
}

/** When a held session is back to normal - its tab badge's hint and its
 *  popover both say it. A paused session continues; a slowed one goes back to
 *  its own model and effort. */
export function pacingResume({ isStopped, resumesAt, original }: {
  isStopped: boolean;
  resumesAt: number | null;
  original: string | null;
}) {
  const resume = isStopped ? "Continues" : `Back to ${original ?? "its usual model and effort"}`;
  if (resumesAt === null) {
    return {
      text: `${resume} once usage is back on pace`,
      time: null
    };
  }

  return {
    text: resume,
    time: resumesAt
  };
}

/** The facts a paced session's popover lists: why it is held (its window and
 *  when it would run out), what it runs instead of its own choice, and when it
 *  is back to normal (`pacingResume`). */
export function pacingFacts({ verdict, choice, resume }: {
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
  resume: PacingFact;
}) {
  const facts: PacingFact[] = [];
  if (verdict !== null) {
    facts.push(verdictFact(verdict));
  }

  if (choice !== null) {
    facts.push({
      text: `Runs ${choice.current} - normally ${choice.original}`,
      time: null
    });
  }

  facts.push(resume);
  return facts;
}

/** Why a window holds a session: a session window runs out before its reset;
 *  a weekly one spends ahead of an even week. */
function verdictFact(verdict: NonNullable<Parameters<typeof pacingFacts>[0]["verdict"]>) {
  const spent = `${windowName(verdict.windowKind)} window at ${formatPercent(verdict.utilization)}`;
  if (verdict.windowKind === PacedWindowKind.enum.weekly) {
    return {
      text: `${spent} - ahead of an even spend, back on pace`,
      time: verdict.readyAt
    };
  }

  return {
    text: `${spent} - runs out before its reset`,
    time: verdict.runOutAt
  };
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
