// The usage-pacing formula - pure, DOM-free and rune-free, so every rule is
// tested on its own (lib/stores/usage/usagePacing applies it to live sessions).
//
// No window length is assumed - a vendor can shorten its session window any
// day - so each paced window (the session, the weekly cap) is judged from two
// things only: when it resets, and how fast it is being spent while its agents
// work (the burn rate). What is left at that pace is the runway:
//
//     runway    = (100% - used) / burn rate
//     time left = reset - now - RESET_GRACE_MS
//
// Past PACE_FLOOR_PERCENTAGE used, a runway shorter than PAUSE_RUNWAY_FRACTION
// of the time left is over pace; a paused session is ready once its runway
// covers all of the time left; a stepped-down one can afford its dearer choice
// again once the runway reaches SPARE_RUNWAY_FRACTION of it. Near a reset a
// window paces no one - it is about to come back anyway. Usage credits that
// would carry an agent past a spent window don't change any of this: going over
// costs real money, which is exactly what pacing is there to save.

import { UsageWindowKind } from "@/lib/types";
import type { AccountUsage, UsageWindow } from "@/lib/types";
import { resetCountdown, usageResetTime } from "@/lib/usage/usage-groups";
import { formatPercent } from "@/lib/util/format";

/** Below this much of a window spent, no one is paced however fast it burns. */
export const PACE_FLOOR_PERCENTAGE = 60;
const FULL_PERCENTAGE = 100;
/** This close to its reset a window paces no one, and a session it paused
 *  continues on its own. */
export const RESET_GRACE_MS = 10 * 60_000;
/** A working session is over pace once its runway covers less than this share
 *  of the time left; a paused one is ready once it covers all of it. The gap
 *  keeps a continued session from pausing again a minute later. */
const PAUSE_RUNWAY_FRACTION = 0.8;
/** A stepped-down session steps back up only once its runway reaches this far
 *  past the reset - the gap to PAUSE_RUNWAY_FRACTION keeps it from see-sawing. */
const SPARE_RUNWAY_FRACTION = 1.5;
/** Two readings further apart than this span idle time, not burn - skipped. */
const MAXIMUM_SAMPLE_GAP_MS = 5 * 60_000;
/** A burn rate needs at least this much working time behind it… */
export const MINIMUM_RATE_SPAN_MS = 10 * 60_000;
/** …and only this much recent working time counts toward it. */
export const RATE_HISTORY_MS = 45 * 60_000;
/** The shortest rest a slowed session takes - a rest of seconds is all
 *  interruption and no saving. */
const MINIMUM_REST_MS = 5 * 60_000;

/** The windows paced, with what a person calls each. A window of another kind
 *  (a per-model cap, extra usage) is never paced. */
const PACED_WINDOWS: Partial<Record<UsageWindowKind, string>> = {
  [UsageWindowKind.enum.session]: "session",
  [UsageWindowKind.enum.weekly]: "weekly"
};

export function windowName(kind: UsageWindowKind) {
  return PACED_WINDOWS[kind] ?? kind;
}

/** One reading of a window's usage, taken while one of its agents was working. */
export interface UsageSample {
  at: number;
  utilization: number;
  resetsAt: string | null | undefined;
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

/** When a window runs free: RESET_GRACE_MS before its reset, or null when it
 *  isn't paced now - an unpaced kind, an unknown or near reset, or usage under
 *  the floor. */
function freeTime({ window, now }: {
  window: UsageWindow;
  now: number;
}) {
  const freeAt = usageResetTime(window.resetsAt) - RESET_GRACE_MS;
  const isPacedNow = PACED_WINDOWS[window.kind] !== undefined
    && Number.isFinite(freeAt)
    && freeAt > now
    && window.utilization >= PACE_FLOOR_PERCENTAGE;
  return isPacedNow ? freeAt : null;
}

function runwayOf({ window, rate }: {
  window: UsageWindow;
  rate: number;
}) {
  return rate > 0 ? (FULL_PERCENTAGE - window.utilization) / rate : Number.POSITIVE_INFINITY;
}

/** Judge an account's pace from each window's measured burn (`rateOf`). The
 *  window that holds the session longest is the one reported: when it may go on
 *  (`readyAt`, once its runway reaches the time left) and when its reset is
 *  close enough to continue on its own (`continueAt`). `isPaused` holds a
 *  paused session until its runway covers all of the time left. Null when no
 *  window holds it - and always before a burn rate is known. */
export function assessPace({ account, now, isPaused, rateOf }: {
  account: AccountUsage | null;
  now: number;
  isPaused: boolean;
  rateOf: (window: UsageWindow) => number | null;
}) {
  if (!account) {
    return null;
  }

  let binding: {
    window: UsageWindow;
    rate: number;
    readyAt: number;
    continueAt: number;
  } | null = null;
  for (const window of account.windows) {
    const freeAt = freeTime({
      window,
      now
    });
    const rate = rateOf(window);
    if (freeAt === null || rate === null) {
      continue;
    }

    const runway = runwayOf({
      window,
      rate
    });
    const needed = (freeAt - now) * (isPaused ? 1 : PAUSE_RUNWAY_FRACTION);
    const readyAt = Math.max(now, freeAt - runway);
    const isHolding = runway < needed && (binding === null || readyAt > binding.readyAt);
    if (isHolding) {
      binding = {
        window,
        rate,
        readyAt,
        continueAt: freeAt
      };
    }
  }

  return binding;
}

/** Whether an account could afford a dearer effort or model again: every paced
 *  window has a measured runway reaching well past its reset. A window with no
 *  measured burn yet can't vouch for it. */
export function hasRoomToSpare({ account, now, rateOf }: {
  account: AccountUsage | null;
  now: number;
  rateOf: (window: UsageWindow) => number | null;
}) {
  if (!account) {
    return false;
  }

  for (const window of account.windows) {
    const freeAt = freeTime({
      window,
      now
    });
    if (freeAt === null) {
      continue;
    }

    const rate = rateOf(window);
    const isShort = rate === null || runwayOf({
      window,
      rate
    }) < (freeAt - now) * SPARE_RUNWAY_FRACTION;
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
  return `PADE usage pacing: ${formatPercent(window.utilization)} of this account's ${windowName(window.kind)} usage is spent and it resets ${countdown}, so ${outcome}. Finish the step you're on, leave the work easy to pick up, then stop and wait - you'll be told to continue.`;
}
