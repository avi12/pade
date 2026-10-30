// The usage-pacing formula - pure, DOM-free and rune-free, so every rule is
// tested on its own (lib/stores/usage/usagePacing applies it to live sessions).
//
// Each paced window is judged by one question: if its agents stopped now, when
// would it be back on pace (`onPaceAt`)? How far ahead of now that lies is how
// far ahead of pace the window is spending. The two windows answer it their own
// way, because they are spent differently:
//
//   session - worked through in one sitting, so it is judged by its burn rate
//             while its agents work, with no window length assumed (a vendor
//             can shorten it any day). What is left at that pace is the runway:
//
//                 runway   = (100% - used) / burn rate
//                 onPaceAt = reset - RESET_GRACE_MS - runway
//
//   weekly  - spread over days with nights and breaks between, so a working
//             burn rate would always look too fast. It is judged against an
//             even spend from the window's start to its reset instead:
//
//                 onPaceAt = start + used × WEEKLY_WINDOW_MS
//
// Past PACE_FLOOR_PERCENTAGE used, a window more than the user's chosen margin
// ahead of pace is over pace; a paused session is ready once it is back on
// pace; a stepped-down one can afford its dearer choice again once it is well
// behind. Near a reset a window paces no one - it is about to come back anyway.
// Usage credits that would carry an agent past a spent window don't change any
// of this: going over costs real money, which is exactly what pacing is there
// to save.

import { PacedWindowKind } from "@/lib/types";
import type { AccountUsage, UsageWindow, UsageWindowKind } from "@/lib/types";
import { resetCountdown, usageResetTime } from "@/lib/usage/usage-groups";
import { formatPercent } from "@/lib/util/format";

/** Below this much of a window spent, no one is paced however fast it burns. */
export const PACE_FLOOR_PERCENTAGE = 60;
const FULL_PERCENTAGE = 100;
/** This close to its reset a window paces no one, and a session it paused
 *  continues on its own. */
export const RESET_GRACE_MS = 10 * 60_000;
/** A stepped-down session steps back up only once its runway reaches this far
 *  past the reset - well clear of where it stepped down, so it doesn't see-saw. */
const SPARE_RUNWAY_FRACTION = 1.5;
/** The chosen margin never asks for more than this share of the time left. */
const MAXIMUM_MARGIN_SHARE = 0.5;
/** Two readings further apart than this span idle time, not burn - skipped. */
const MAXIMUM_SAMPLE_GAP_MS = 5 * 60_000;
/** A burn rate needs at least this much working time behind it… */
export const MINIMUM_RATE_SPAN_MS = 10 * 60_000;
/** …and only this much recent working time counts toward it. */
export const RATE_HISTORY_MS = 45 * 60_000;
/** The shortest rest a slowed session takes - a rest of seconds is all
 *  interruption and no saving. */
const MINIMUM_REST_MS = 5 * 60_000;
/** How long a weekly window runs, start to reset - the length its even spend
 *  is drawn over. */
export const WEEKLY_WINDOW_MS = 7 * 24 * 60 * 60_000;

/** What a person calls each paced window. A window of another kind (a
 *  per-model cap, extra usage) is never paced. */
const PACED_WINDOW_NAMES: Record<PacedWindowKind, string> = {
  [PacedWindowKind.enum.session]: "session",
  [PacedWindowKind.enum.weekly]: "weekly"
};

/** The paced kind a window is, or null for a window pacing never watches. */
function pacedKind(kind: UsageWindowKind) {
  const parsed = PacedWindowKind.safeParse(kind);
  return parsed.success ? parsed.data : null;
}

export function windowName(kind: UsageWindowKind) {
  const paced = pacedKind(kind);
  return paced === null ? kind : PACED_WINDOW_NAMES[paced];
}

/** One reading of a window's usage, taken while one of its agents was working. */
export interface UsageSample {
  at: number;
  utilization: number;
  resetsAt?: string | null;
}

/** How fast a window is being spent while its agents work, in percent per
 *  millisecond: the rise across consecutive readings of the same window, over
 *  the working time between them. Null until enough working time is behind it. */
export function burnRate(samples: readonly UsageSample[]) {
  let spent = 0;
  let span = 0;
  for (let i = 1; i < samples.length; i += 1) {
    const previous = samples[i - 1];
    const current = samples[i];
    const gap = current.at - previous.at;
    const isSameWindow = current.resetsAt === previous.resetsAt;
    const isWorkingStretch = gap > 0 && gap <= MAXIMUM_SAMPLE_GAP_MS;
    const rise = current.utilization - previous.utilization;
    if (!isSameWindow || !isWorkingStretch || rise < 0) {
      continue;
    }

    spent += rise;
    span += gap;
  }

  return span >= MINIMUM_RATE_SPAN_MS ? spent / span : null;
}

/** How far ahead of pace each paced window may run before pacing acts, in
 *  milliseconds - null for a window whose pacing is off. */
export type MarginOf = (kind: PacedWindowKind) => number | null;

/** A paced window being watched now: its kind, when it runs free
 *  (RESET_GRACE_MS before its reset) and the user's margin for it. Null when it
 *  isn't paced now - an unpaced or switched-off kind, an unknown or near reset,
 *  or usage under the floor. */
function watchedWindow({ window, now, marginOf }: {
  window: UsageWindow;
  now: number;
  marginOf: MarginOf;
}) {
  const kind = pacedKind(window.kind);
  const margin = kind === null ? null : marginOf(kind);
  const freeAt = usageResetTime(window.resetsAt) - RESET_GRACE_MS;
  const isPacedNow = kind !== null
    && margin !== null
    && Number.isFinite(freeAt)
    && freeAt > now
    && window.utilization >= PACE_FLOOR_PERCENTAGE;
  return isPacedNow ? {
    kind,
    freeAt,
    margin
  } : null;
}

function runwayOf({ window, rate }: {
  window: UsageWindow;
  rate: number;
}) {
  return rate > 0 ? (FULL_PERCENTAGE - window.utilization) / rate : Number.POSITIVE_INFINITY;
}

/** Where the window would stand with an even spend from its start to its
 *  reset, and when that even spend reaches what is used now. */
function evenSpend({ window, now }: {
  window: UsageWindow;
  now: number;
}) {
  const startAt = usageResetTime(window.resetsAt) - WEEKLY_WINDOW_MS;
  return {
    percentage: (now - startAt) / WEEKLY_WINDOW_MS * FULL_PERCENTAGE,
    reachedAt: startAt + window.utilization / FULL_PERCENTAGE * WEEKLY_WINDOW_MS
  };
}

/** When a window would be back on pace if its agents stopped now, by its own
 *  measure (see the top of this file) - null while a session window's burn
 *  rate isn't measured yet. */
function onPaceTime({ kind, window, freeAt, rate, now }: {
  kind: PacedWindowKind;
  window: UsageWindow;
  freeAt: number;
  rate: number | null;
  now: number;
}) {
  if (kind === PacedWindowKind.enum.weekly) {
    return evenSpend({
      window,
      now
    }).reachedAt;
  }

  return rate === null ? null : freeAt - runwayOf({
    window,
    rate
  });
}

/** The trigger: a window more than `margin` ahead of pace. */
function isTooFarAhead({ onPaceAt, freeAt, margin, now }: {
  onPaceAt: number;
  freeAt: number;
  margin: number;
  now: number;
}) {
  // A margin longer than the time left can never be met - "2 h early" with 16
  // minutes to go would leave the window unpaced right when it runs dry - so it
  // shrinks to at most a share of the time left.
  const reachable = Math.min(margin, (freeAt - now) * MAXIMUM_MARGIN_SHARE);
  return onPaceAt - now > reachable;
}

/** How far behind pace a window has to fall before a stepped-down session may
 *  climb back: a session window, well clear of where it stepped down so it
 *  doesn't see-saw; a weekly window, back on its even spend - the margin it
 *  stepped down at is band enough over days. */
function spareMs({ kind, freeAt, now }: {
  kind: PacedWindowKind;
  freeAt: number;
  now: number;
}) {
  return kind === PacedWindowKind.enum.weekly ? 0 : (freeAt - now) * (SPARE_RUNWAY_FRACTION - 1);
}

/** One paced window's live line for the Config panel (`paceForecasts`). */
export interface PaceForecast {
  window: UsageWindow;
  resetAt: number;
  /** When it runs out at its pace, or null while that isn't measured (or
   *  nothing is being spent). */
  runOutAt: number | null;
  /** Whether enough working time is behind the pace to judge it. */
  isMeasured: boolean;
  /** When it would be back on pace if its agents stopped now. */
  onPaceAt: number | null;
  /** A weekly window's even spend by now, in percent - null for a session. */
  evenPercentage: number | null;
}

/** Whether a forecast is over pace now - what the Config panel's live line
 *  asks, by the same rule the pacing loop acts on. */
export function isForecastOverPace({ forecast, margin, now }: {
  forecast: PaceForecast;
  margin: number;
  now: number;
}) {
  const freeAt = forecast.resetAt - RESET_GRACE_MS;
  const isPacedNow = forecast.window.utilization >= PACE_FLOOR_PERCENTAGE && freeAt > now;
  return isPacedNow && forecast.onPaceAt !== null && isTooFarAhead({
    onPaceAt: forecast.onPaceAt,
    freeAt,
    margin,
    now
  });
}

/** Judge an account's pace, each window by its own measure (`rateOf` gives a
 *  session window's measured burn). A window is over pace once it runs more
 *  than its margin ahead (`marginOf`). The window that holds the session
 *  longest is the one reported: when it may go on (`readyAt`, once it is back
 *  on pace) and when its reset is close enough to continue on its own
 *  (`continueAt`). `isPaused` holds a paused session until it is back on pace.
 *  Null when no window holds it. */
export function assessPace({ account, now, isPaused, marginOf, rateOf }: {
  account: AccountUsage | null;
  now: number;
  isPaused: boolean;
  marginOf: MarginOf;
  rateOf: (window: UsageWindow) => number | null;
}) {
  if (!account) {
    return null;
  }

  let binding: {
    kind: PacedWindowKind;
    window: UsageWindow;
    rate: number;
    readyAt: number;
    continueAt: number;
  } | null = null;
  for (const window of account.windows) {
    const watched = watchedWindow({
      window,
      now,
      marginOf
    });
    if (watched === null) {
      continue;
    }

    const rate = rateOf(window);
    const onPaceAt = onPaceTime({
      ...watched,
      window,
      rate,
      now
    });
    if (onPaceAt === null) {
      continue;
    }

    const isOverPace = isTooFarAhead({
      onPaceAt,
      freeAt: watched.freeAt,
      margin: isPaused ? 0 : watched.margin,
      now
    });
    const readyAt = Math.max(now, onPaceAt);
    const isHolding = isOverPace && (binding === null || readyAt > binding.readyAt);
    if (isHolding) {
      binding = {
        kind: watched.kind,
        window,
        rate: rate ?? 0,
        readyAt,
        continueAt: watched.freeAt
      };
    }
  }

  return binding;
}

/** Every paced window's forecast, by kind - for the Config panel's live
 *  lines. A session window's comes from its measured burn; a weekly window's
 *  from its average spend since it started. */
export function paceForecasts({ account, now, rateOf }: {
  account: AccountUsage | null;
  now: number;
  rateOf: (window: UsageWindow) => number | null;
}) {
  const forecasts: Partial<Record<PacedWindowKind, PaceForecast>> = {};
  for (const window of account?.windows ?? []) {
    const kind = pacedKind(window.kind);
    const resetAt = usageResetTime(window.resetsAt);
    const isForecastable = kind !== null
      && forecasts[kind] === undefined
      && Number.isFinite(resetAt)
      && resetAt > now;
    if (!isForecastable) {
      continue;
    }

    const freeAt = resetAt - RESET_GRACE_MS;
    const even = kind === PacedWindowKind.enum.weekly ? evenSpend({
      window,
      now
    }) : null;
    const elapsed = Math.max(MINIMUM_RATE_SPAN_MS, now - (resetAt - WEEKLY_WINDOW_MS));
    const averageRate = even === null ? null : window.utilization / elapsed;
    const rate = averageRate ?? rateOf(window);
    forecasts[kind] = {
      window,
      resetAt,
      runOutAt: rate === null || rate <= 0 ? null : now + runwayOf({
        window,
        rate
      }),
      isMeasured: rate !== null,
      onPaceAt: onPaceTime({
        kind,
        window,
        freeAt,
        rate,
        now
      }),
      evenPercentage: even?.percentage ?? null
    };
  }

  return forecasts;
}

/** Whether resting can bring a session back on pace - it needs no more rest
 *  than work. Past that, a stop does the job better than rests longer than the
 *  work between them. */
export function isRestEnough({ readyAt, continueAt, now }: {
  readyAt: number;
  continueAt: number;
  now: number;
}) {
  const runway = continueAt - readyAt;
  const timeLeft = continueAt - now;
  return timeLeft <= 2 * runway;
}

/** Whether an account could afford a dearer effort or model again: every
 *  watched window is far enough behind pace. A session window with no measured
 *  burn yet can't vouch for it. */
export function hasRoomToSpare({ account, now, marginOf, rateOf }: {
  account: AccountUsage | null;
  now: number;
  marginOf: MarginOf;
  rateOf: (window: UsageWindow) => number | null;
}) {
  if (!account) {
    return false;
  }

  for (const window of account.windows) {
    const watched = watchedWindow({
      window,
      now,
      marginOf
    });
    if (watched === null) {
      continue;
    }

    const onPaceAt = onPaceTime({
      ...watched,
      window,
      rate: rateOf(window),
      now
    });
    const isShort = onPaceAt === null || now - onPaceAt < spareMs({
      ...watched,
      now
    });
    if (isShort) {
      return false;
    }
  }

  return true;
}

/** When a slowed session's rest ends. Working a stretch at its burn rate, then
 *  resting, averages out to the pace that lasts until the reset when
 *
 *     rest = stretch × (time left / runway - 1)
 *
 *  with time left and runway both measured to the grace period. Never shorter
 *  than MINIMUM_REST_MS, and never past the moment a full stop would end. */
export function slowRestUntil({ readyAt, continueAt, stretch, now }: {
  readyAt: number;
  continueAt: number;
  /** How long the session has just worked. */
  stretch: number;
  now: number;
}) {
  const runway = continueAt - readyAt;
  const timeLeft = continueAt - now;
  const rest = runway > 0 ? stretch * (timeLeft / runway - 1) : timeLeft;
  return Math.min(readyAt, now + Math.max(MINIMUM_REST_MS, rest));
}

/** The request that stops a working agent at its next safe point - for good,
 *  or for a short rest when it is being slowed down. */
export function pauseRequest({ window, now, isSlowing }: {
  window: UsageWindow;
  now: number;
  isSlowing: boolean;
}) {
  const countdown = resetCountdown({
    iso: window.resetsAt,
    now
  });
  const outcome = isSlowing ? "work takes a short break here to slow the pace" : "work pauses here to spread the rest out";
  return `PADE usage pacing: ${formatPercent(window.utilization)} of this account's ${windowName(window.kind)} usage is spent and it resets ${countdown}, so ${outcome}. The pause covers work you delegated too: launch no new background agents, workflows or teammates, and stop every one still running, noting where each got to so it can be relaunched. Finish the step you're on, leave the work easy to pick up, then stop and wait - you'll be told to continue.`;
}

/** The message that picks a paused agent back up - marked as pacing's own, so
 *  the agent never mistakes it for the user's answer to a question it asked. */
export function resumeRequest() {
  return "PADE usage pacing: the pause is over - pick up where you left off, relaunching any background work you stopped for it. This message comes from pacing, not the user: it answers nothing you asked, so whatever waits on the user's decision keeps waiting.";
}

/** What the user's own "Continue anyway" types: the user is speaking, so it
 *  needs none of pacing's explaining. */
export const USER_CONTINUE_REQUEST = "continue";
