// The usage-pacing formula - pure, DOM-free and rune-free, so every rule is
// tested on its own (lib/stores/usage/usagePacing applies it to live sessions).
//
// Pacing spreads what is left of a window over the time left until it resets,
// and slows an account only as much as that takes, from the first percent on -
// there is no usage level below which it looks away. Each paced window answers
// one question: what share of its current spend can the account keep?
//
//     affordable = (100% - used) / time left
//     allowance  = ALLOWANCE - (used - even spend so far), within 0..ALLOWANCE
//     may spend  = affordable + allowance / payout time
//     keep       = may spend / pace
//
// The allowance is the room for bursts. An even spend climbs through nights and
// breaks while nothing is spent, so idle hours refill it (up to ALLOWANCE), and
// every rise in usage - from any session on the account - drains it. One
// session's half-day burst fits in it and slows nothing; several sessions
// bursting at once drain it within hours, and the keep share slides down toward
// what lasts to the reset. A window already spent past its allowance just
// spreads what is left over the time left, so an overspend is paid back a
// little at a time across the rest of the window, never by a long stop.
//
// The pace is the account's measured burn while its agents work, its usage
// deltas over its recent working time (`burnRate`), raised to what the agents
// are running right now when that is more (`projectedRate`): a session that
// starts on, or switches to, a dear model such as Fable counts the moment it
// does, not ten minutes of readings later.
//
// Below SLOW_DOWN_KEEP a window is over pace; a paused session is ready once its
// keep share reaches 1; a stepped-down one can afford its dearer choice again
// once the keep share is well above it (STEP_UP_KEEP). Near a reset a window
// paces no one - it is about to come back anyway. Usage credits that would carry
// an agent past a spent window don't change any of this: going over costs real
// money, which is exactly what pacing is there to save.

import { PacedWindowKind } from "@/lib/types/pacing";
import type { AccountUsage, UsageWindow, UsageWindowKind } from "@/lib/types/usage";
import { usageResetTime } from "@/lib/usage/groups/reset-time";

const FULL_PERCENTAGE = 100;
/** This close to its reset a window paces no one, and a session it paused
 *  continues on its own. */
export const RESET_GRACE_MS = 10 * 60_000;
/** Two readings further apart than this span idle time, not burn - skipped. */
const MAXIMUM_SAMPLE_GAP_MS = 5 * 60_000;
/** A burn rate needs at least this much working time behind it… */
export const MINIMUM_RATE_SPAN_MS = 10 * 60_000;
/** …and only this much recent working time counts toward it - long enough to
 *  hold two of a window's whole-percent ticks at a slow, paced burn. */
export const RATE_HISTORY_MS = 3 * 60 * 60_000;
/** A window's usage is read in whole percents: the burn shows only when it
 *  crosses one. */
const UTILIZATION_STEP = 1;
/** This much working time without a single tick says the burn is slower than
 *  the readings can show; less says nothing yet - a whole percent can take
 *  over an hour to cross at a paced burn. */
const ZERO_EVIDENCE_SPAN_MS = 2 * 60 * 60_000;
/** The slowest burn the readings can tell from none: one tick in that span. */
const SLOWEST_MEASURABLE_RATE = UTILIZATION_STEP / ZERO_EVIDENCE_SPAN_MS;
/** The shortest rest a slowed session takes - a rest of seconds is all
 *  interruption and no saving. */
const MINIMUM_REST_MS = 5 * 60_000;
/** The shortest stretch a slowed session works between two rests - long enough
 *  to get something done, so rests come as short, regular breaks. */
const MINIMUM_WORK_STRETCH_MS = 15 * 60_000;
/** A window's daily readings reach back this far… */
const DAILY_SPAN_MS = 24 * 60 * 60_000;
/** …one kept per this much time, the newest always current… */
const DAILY_READING_SPACING_MS = 60 * 60_000;
/** …and a daily rate needs this much time behind it, or it says more about
 *  the last hour than the day. */
const MINIMUM_DAILY_SPAN_MS = 6 * 60 * 60_000;
/** How long a weekly window runs, start to reset. */
const WEEKLY_WINDOW_MS = 7 * DAILY_SPAN_MS;
/** How long a session window runs, start to reset. */
const SESSION_WINDOW_MS = 5 * 60 * 60_000;
const WINDOW_MS: Record<PacedWindowKind, number> = {
  [PacedWindowKind.enum.session]: SESSION_WINDOW_MS,
  [PacedWindowKind.enum.weekly]: WEEKLY_WINDOW_MS
};
/** The burst allowance: how far ahead of an even spend a window may run, in
 *  percent of the window, before the overspend has to be paid back. */
const ALLOWANCE_PERCENTAGE = 10;
/** The allowance is paid out over this share of the window - eight hours of a
 *  week, a quarter of an hour of a session window. */
const PAYOUT_SHARE = 8 / (7 * 24);
/** Under this keep share a window is over pace - a little slack, so the jitter
 *  of whole-percent readings doesn't slow anything. */
const SLOW_DOWN_KEEP = 0.95;
/** A stepped-down session steps back up only once its account could keep this
 *  much more than it spends - clear of what one step saves, so it doesn't
 *  see-saw. */
const STEP_UP_KEEP = 1.3;
/** With this little of a window left there is next to nothing to spend, so a
 *  session stops until there is room instead of resting. A pace far over -
 *  however far - only ever rests: a stop judged on a forecast once held
 *  sessions from Tuesday to Saturday's reset. */
const STOP_REMAINING_PERCENTAGE = 2;
/** The longest a slowed session rests before it works a stretch again, so the
 *  pace is measured afresh rather than trusted for days - the slower the keep
 *  share, the more of each cycle is rest, up to this. */
const MAXIMUM_REST_MS = 2 * 60 * 60_000;
/** Halvings that find when a paused session is back on pace - a week down to
 *  well under a second. */
const READY_SEARCH_STEPS = 40;

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

/** The paced windows an account reports, in the enum's order - a plan without
 *  a session limit (Claude Ultra) reports only its weekly one. Every paced
 *  window while there is no reading, since nothing says which the plan has. */
export function reportedPacedKinds(account: AccountUsage | null) {
  if (account === null) {
    return PacedWindowKind.options;
  }

  const reported = new Set(account.windows.map(window => pacedKind(window.kind)));
  return PacedWindowKind.options.filter(kind => reported.has(kind));
}

/** When the last of an account's paced windows resets - how long "Skip this
 *  window" holds when no verdict names the window. Null with none known. */
export function lastPacedReset(account: AccountUsage | null) {
  let last: number | null = null;
  for (const window of account?.windows ?? []) {
    const resetAt = usageResetTime(window.resetsAt);
    const isLaterPacedReset =
      pacedKind(window.kind) !== null && Number.isFinite(resetAt) && (last === null || resetAt > last);
    if (isLaterPacedReset) {
      last = resetAt;
    }
  }

  return last;
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

/** A vendor re-stamps a window's reset a little differently on each read
 *  (sub-second jitter in the same window); a new window moves it by hours. */
const RESET_JITTER_MS = 60_000;

/** Whether two readings are of the same window - by its reset, within the
 *  jitter, never by the strings, which differ read to read. */
function isSameReset({ previous, current }: { previous: UsageSample; current: UsageSample }) {
  const difference = Math.abs(usageResetTime(current.resetsAt) - usageResetTime(previous.resetsAt));
  if (Number.isNaN(difference)) {
    return current.resetsAt === previous.resetsAt;
  }

  return difference < RESET_JITTER_MS;
}

/** How fast a window is being spent while its agents work, in percent per
 *  millisecond, from consecutive readings of the same window over the working
 *  time between them. The readings move in whole percents, so a short span
 *  mostly reads a flat line however fast the burn: it is measured tick to tick
 *  (the rise after the first tick, over the working time to the last one),
 *  never faster than the flat stretch since allows. Null until enough working
 *  time is behind it, and while no tick has shown in less than
 *  `ZERO_EVIDENCE_SPAN_MS` - a flat line that short is no evidence of no burn. */
export function burnRate(samples: readonly UsageSample[]) {
  let spent = 0;
  let span = 0;
  let firstTickAt: number | null = null;
  let lastTickAt = 0;
  let spentAfterFirstTick = 0;
  for (let i = 1; i < samples.length; i += 1) {
    const previous = samples[i - 1];
    const current = samples[i];
    const gap = current.at - previous.at;
    const isSameWindow = isSameReset({
      previous,
      current
    });
    const isWorkingStretch = gap > 0 && gap <= MAXIMUM_SAMPLE_GAP_MS;
    const rise = current.utilization - previous.utilization;
    const isUnusableSample = !isSameWindow || !isWorkingStretch || rise < 0;
    if (isUnusableSample) {
      continue;
    }

    spent += rise;
    span += gap;
    const isTick = rise > 0;
    if (!isTick) {
      continue;
    }

    if (firstTickAt === null) {
      firstTickAt = span;
    } else {
      spentAfterFirstTick += rise;
      lastTickAt = span;
    }
  }

  if (span < MINIMUM_RATE_SPAN_MS) {
    return null;
  }

  if (firstTickAt === null) {
    return span >= ZERO_EVIDENCE_SPAN_MS ? 0 : null;
  }

  const tickSpan = lastTickAt - firstTickAt;
  const isTickToTick = spentAfterFirstTick > 0 && tickSpan >= MINIMUM_RATE_SPAN_MS;
  const estimate = isTickToTick ? spentAfterFirstTick / tickSpan : spent / span;
  // Since the first tick the window has crossed no more than what it shows,
  // and not yet the next whole percent.
  const sinceFirstTick = span - firstTickAt;
  const ceiling = sinceFirstTick > 0 ? (spentAfterFirstTick + UTILIZATION_STEP) / sinceFirstTick : estimate;
  return Math.min(estimate, ceiling);
}

/** A window's daily readings with a new one added: one an hour over the last
 *  day of the same window, the newest replaced by the latest. Unlike the burn
 *  readings, these are never restarted by a pacing step. */
export function withDailyReading({ readings, reading }: { readings: readonly UsageSample[]; reading: UsageSample }) {
  const kept = readings.filter(
    each =>
      each.at > reading.at - DAILY_SPAN_MS &&
      isSameReset({
        previous: each,
        current: reading
      })
  );
  const lastKept = kept.at(-2);
  const isNewestReplaced = lastKept !== undefined && reading.at - lastKept.at < DAILY_READING_SPACING_MS;
  return [...(isNewestReplaced ? kept.slice(0, -1) : kept), reading];
}

/** How fast a window was spent over about the last day, nights and breaks
 *  included, in percent per millisecond: the rise from its oldest daily
 *  reading to its newest, over the time between. Null with less than
 *  `MINIMUM_DAILY_SPAN_MS` behind it. */
export function dailyRate(readings: readonly UsageSample[]) {
  const oldest = readings.at(0);
  const newest = readings.at(-1);
  if (oldest === undefined || newest === undefined) {
    return null;
  }

  const span = newest.at - oldest.at;
  return span >= MINIMUM_DAILY_SPAN_MS ? Math.max(0, newest.utilization - oldest.utilization) / span : null;
}

/** How far the load forecast may lift a measured burn. Load and burn are only
 *  loosely tied - a scan can catch the load near zero, or count a burst the
 *  readings never show - and an unbounded forecast once read 1,363% an hour,
 *  which stopped sessions until the weekly reset. A dear model joining still
 *  counts at once, up to this many times what the account measures. */
const MAXIMUM_PROJECTED_RISE = 3;

/** An account's pace as its agents run now: the measured burn, raised to what
 *  the sessions working now would spend at the burn per unit of load measured
 *  last (`ratePerLoad`), at most `MAXIMUM_PROJECTED_RISE` times the measured
 *  burn - so a session starting on, or switching to, a dear model counts at
 *  once. A measured zero only says the burn is below what whole-percent
 *  readings show, so the cap counts it as that slowest measurable burn rather
 *  than silencing the forecast. Either alone when the other is unknown; null
 *  when both are. */
export function projectedRate({
  measured,
  ratePerLoad,
  load
}: {
  measured: number | null;
  ratePerLoad: number | null;
  /** The account's working load now (lib/usage/pacing/engine/load-shedding). */
  load: number;
}) {
  const forecast = ratePerLoad === null ? null : ratePerLoad * load;
  if (measured === null) {
    return forecast;
  }

  if (forecast === null) {
    return measured;
  }

  const ceiling = Math.max(measured, SLOWEST_MEASURABLE_RATE) * MAXIMUM_PROJECTED_RISE;
  return Math.min(Math.max(measured, forecast), ceiling);
}

/** The pace each window's account is measured at, in percent per millisecond -
 *  null while it isn't measured yet. */
export type RateOf = (window: UsageWindow) => number | null;

/** A paced window while it still paces: its kind, its reset and when it runs
 *  free (RESET_GRACE_MS before it). Null for an unpaced kind or an unknown or
 *  near reset. */
function pacedWindow({ window, now }: { window: UsageWindow; now: number }) {
  const kind = pacedKind(window.kind);
  const resetAt = usageResetTime(window.resetsAt);
  const freeAt = resetAt - RESET_GRACE_MS;
  const isPacedNow = kind !== null && Number.isFinite(freeAt) && freeAt > now;
  return isPacedNow
    ? {
        kind,
        resetAt,
        freeAt
      }
    : null;
}

type PacedWindow = NonNullable<ReturnType<typeof pacedWindow>>;

/** What a window lets its account spend at `at`, in percent per millisecond,
 *  with `used` spent by then (see the top of this file). */
function maySpendAt({ paced, used, at }: { paced: PacedWindow; used: number; at: number }) {
  const length = WINDOW_MS[paced.kind];
  const elapsed = Math.min(length, Math.max(0, at - (paced.resetAt - length)));
  const even = (elapsed / length) * FULL_PERCENTAGE;
  const allowance = Math.min(ALLOWANCE_PERCENTAGE, Math.max(0, ALLOWANCE_PERCENTAGE - (used - even)));
  const affordable = (FULL_PERCENTAGE - used) / (paced.freeAt - at);
  return Math.max(0, affordable + allowance / (PAYOUT_SHARE * length));
}

/** The share of its spend at `rate` a window lets its account keep now: 1 is
 *  exactly on pace, under it the account must shed the rest, over it there is
 *  room to spare. Infinite while nothing is spent. */
function keepAt({ paced, window, rate, now }: { paced: PacedWindow; window: UsageWindow; rate: number; now: number }) {
  const maySpend = maySpendAt({
    paced,
    used: window.utilization,
    at: now
  });
  return rate > 0 ? maySpend / rate : Number.POSITIVE_INFINITY;
}

/** When a window would let its account spend at `rate` again if its agents
 *  stopped now - its even spend climbing back toward what is used. Its free
 *  time when that never comes first. */
function readyTime({
  paced,
  window,
  rate,
  now
}: {
  paced: PacedWindow;
  window: UsageWindow;
  rate: number;
  now: number;
}) {
  function isReadyAt(time: number) {
    return (
      maySpendAt({
        paced,
        used: window.utilization,
        at: time
      }) >= rate
    );
  }

  if (isReadyAt(now)) {
    return now;
  }

  let early = now;
  let late = paced.freeAt;
  for (let i = 0; i < READY_SEARCH_STEPS; i += 1) {
    const middle = (early + late) / 2;
    if (isReadyAt(middle)) {
      late = middle;
    } else {
      early = middle;
    }
  }

  return late;
}

/** Judge an account's pace, each paced window by its keep share at its pace
 *  (`rateOf`). A window is over pace under SLOW_DOWN_KEEP - or, for a paused
 *  session (`isPaused`), until it is back to 1. The window that keeps the
 *  least is the one reported: its keep share, when the session may go on
 *  (`readyAt`) and when the reset is close enough to continue on its own
 *  (`continueAt`). Null when no window holds it - or none is measured yet. */
export function assessPace({
  account,
  now,
  isPaused,
  rateOf
}: {
  account: AccountUsage | null;
  now: number;
  isPaused: boolean;
  rateOf: RateOf;
}) {
  if (!account) {
    return null;
  }

  const threshold = isPaused ? 1 : SLOW_DOWN_KEEP;
  let binding: {
    kind: PacedWindowKind;
    window: UsageWindow;
    rate: number;
    keep: number;
    readyAt: number;
    continueAt: number;
  } | null = null;
  for (const window of account.windows) {
    const paced = pacedWindow({
      window,
      now
    });
    const rate = rateOf(window);
    if (paced === null || rate === null) {
      continue;
    }

    const keep = keepAt({
      paced,
      window,
      rate,
      now
    });
    const isHolding = keep < threshold && (binding === null || keep < binding.keep);
    if (isHolding) {
      binding = {
        kind: paced.kind,
        window,
        rate,
        keep,
        readyAt: readyTime({
          paced,
          window,
          rate,
          now
        }),
        continueAt: paced.freeAt
      };
    }
  }

  return binding;
}

/** Whether an account could afford a dearer effort or model again: every
 *  paced window keeps well above its spend (STEP_UP_KEEP). A window with no
 *  measured pace yet can't vouch for it. */
export function hasRoomToSpare({
  account,
  now,
  rateOf
}: {
  account: AccountUsage | null;
  now: number;
  rateOf: RateOf;
}) {
  if (!account) {
    return false;
  }

  for (const window of account.windows) {
    const paced = pacedWindow({
      window,
      now
    });
    if (paced === null) {
      continue;
    }

    const rate = rateOf(window);
    const isShort =
      rate === null ||
      keepAt({
        paced,
        window,
        rate,
        now
      }) < STEP_UP_KEEP;
    if (isShort) {
      return false;
    }
  }

  return true;
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
  /** The share of its spend the account may keep (see the top of this file),
   *  null while its pace isn't measured or the window is about to reset. */
  keep: number | null;
}

/** Whether a forecast is over pace now - what the Config panel's live line
 *  asks, by the same rule the pacing loop acts on. */
export function isForecastOverPace(forecast: PaceForecast) {
  return forecast.keep !== null && forecast.keep < SLOW_DOWN_KEEP;
}

/** Every paced window's forecast, by kind - for the Config panel's live
 *  lines. A session window runs out at its measured burn; a weekly window at
 *  its spend over the last day (`dailyRateOf`), nights and breaks included -
 *  so a burst early in the week, since slowed by pacing, no longer forecasts
 *  a run-out it won't reach - or, with less than a day of readings, its
 *  average spend since it started. */
export function paceForecasts({
  account,
  now,
  rateOf,
  dailyRateOf
}: {
  account: AccountUsage | null;
  now: number;
  rateOf: RateOf;
  dailyRateOf: RateOf;
}) {
  const forecasts: Partial<Record<PacedWindowKind, PaceForecast>> = {};
  for (const window of account?.windows ?? []) {
    const kind = pacedKind(window.kind);
    const resetAt = usageResetTime(window.resetsAt);
    const isForecastable = kind !== null && forecasts[kind] === undefined && Number.isFinite(resetAt) && resetAt > now;
    if (!isForecastable) {
      continue;
    }

    const rate = rateOf(window);
    const elapsed = Math.max(MINIMUM_RATE_SPAN_MS, now - (resetAt - WEEKLY_WINDOW_MS));
    const runOutRate =
      kind === PacedWindowKind.enum.weekly ? (dailyRateOf(window) ?? window.utilization / elapsed) : rate;
    const isNoRunOut = runOutRate === null || runOutRate <= 0;
    const paced = pacedWindow({
      window,
      now
    });
    const isKeepKnown = paced !== null && rate !== null;
    forecasts[kind] = {
      window,
      resetAt,
      runOutAt: isNoRunOut ? null : now + (FULL_PERCENTAGE - window.utilization) / runOutRate,
      isMeasured: rate !== null,
      keep: isKeepKnown
        ? keepAt({
            paced,
            window,
            rate,
            now
          })
        : null
    };
  }

  return forecasts;
}

/** The shortest waits that still keep an over-pace account to its keep share:
 *  how long to work, then how long to rest, in a cycle that repeats. A cycle of
 *  work `stretch` then rest averages out to the keep share (the duty) when
 *
 *     rest = stretch × (1 / duty - 1)
 *
 *  for any stretch - so the waits are shortest with the shortest stretch worth
 *  working (MINIMUM_WORK_STRETCH_MS). A rest under MINIMUM_REST_MS is all
 *  interruption and no saving, so with a high duty the stretch grows instead,
 *  keeping the rest at its minimum and the duty intact. No rest runs past
 *  MAXIMUM_REST_MS: under the duty that takes, the next stretch measures the
 *  pace again. */
export function workRhythm(duty: number) {
  const boundedDuty = Math.min(1, Math.max(0, duty));
  const isLongestRest = boundedDuty <= MINIMUM_WORK_STRETCH_MS / (MINIMUM_WORK_STRETCH_MS + MAXIMUM_REST_MS);
  if (isLongestRest) {
    return {
      stretch: MINIMUM_WORK_STRETCH_MS,
      rest: MAXIMUM_REST_MS
    };
  }

  if (boundedDuty === 1) {
    return {
      stretch: MINIMUM_WORK_STRETCH_MS,
      rest: MINIMUM_REST_MS
    };
  }

  const restPerWork = 1 / boundedDuty - 1;
  const stretch = Math.max(MINIMUM_WORK_STRETCH_MS, MINIMUM_REST_MS / restPerWork);
  return {
    stretch,
    rest: stretch * restPerWork
  };
}

/** Whether a window has next to nothing left - the one case a session stops
 *  until there is room rather than resting. */
export function isNearlySpent(window: UsageWindow) {
  return FULL_PERCENTAGE - window.utilization <= STOP_REMAINING_PERCENTAGE;
}

/** When a slowed session's rest ends: its rhythm's rest from now, never past the
 *  moment it would be back on pace anyway. */
export function slowRestUntil({ keep, readyAt, now }: { keep: number; readyAt: number; now: number }) {
  return Math.min(readyAt, now + workRhythm(keep).rest);
}
