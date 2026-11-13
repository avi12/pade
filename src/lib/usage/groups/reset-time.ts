// Reset-time formatting: the absolute timestamp and the countdown a limit shows.

const MILLISECONDS_PER_SECOND = 1000;

const SECONDS_PER_MINUTE = 60;

const SECONDS_PER_HOUR = 3_600;

const SECONDS_PER_DAY = 86_400;

/** The endpoint's sub-millisecond digits, trimmed so every engine parses the
 *  timestamp the same way. */
const SUB_MILLISECOND_DIGITS = /(\.\d{3})\d+/;

// Normalize the endpoint's microsecond timestamps to ms so every engine parses
// them identically - otherwise the countdown can drift.
//
// Total by construction: a window's `resetsAt` is `nullish` (the vendor states a
// reset time only when it knows one), so this takes the absent cases too and
// answers NaN, which every caller already filters through `Number.isFinite`.
// It used to demand a `string`, and one caller's filter excluded `undefined` but
// not `null` - the null reached `.replace` and threw *inside an effect*, which
// takes the whole effect batch down with it: a terminal mounting in the same
// flush never got its xterm and sat at "Starting…" forever.
export function usageResetTime(iso: string | null | undefined) {
  if (!iso) {
    return Number.NaN;
  }

  return new Date(iso.replace(SUB_MILLISECOND_DIGITS, "$1")).getTime();
}

export function hasResetPassed({ resetsAt, now }: { resetsAt: string | null | undefined; now: number }) {
  const resetMilliseconds = usageResetTime(resetsAt);
  return Number.isFinite(resetMilliseconds) && resetMilliseconds <= now;
}

// Absolute reset time for the hover tooltip - weekday + date + time so a window
// days out reads unambiguously. Local + locale-aware, like the number formatters.
const RESET_TIMESTAMP = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  month: "short",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit"
});

// ISO reset time → its absolute local timestamp, or "" when unknown/unparseable.
export function resetTimestamp(iso: string | null | undefined) {
  const resetMilliseconds = usageResetTime(iso);
  return Number.isFinite(resetMilliseconds) ? RESET_TIMESTAMP.format(resetMilliseconds) : "";
}

// ISO reset time → a live "in …" countdown (largest two units), or "". Consumers
// phrase it: the spotlight prefixes "resets", each limit row shows it as-is.
export function resetCountdown({ iso, now }: { iso: string | null | undefined; now: number }) {
  const remaining = usageResetTime(iso) - now;
  const isResetAhead = Number.isFinite(remaining) && remaining > 0;
  if (!isResetAhead) {
    return "";
  }

  const totalSeconds = Math.floor(remaining / MILLISECONDS_PER_SECOND);
  const days = Math.floor(totalSeconds / SECONDS_PER_DAY);
  const hours = Math.floor((totalSeconds % SECONDS_PER_DAY) / SECONDS_PER_HOUR);
  const minutes = Math.floor((totalSeconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE);
  const seconds = totalSeconds % SECONDS_PER_MINUTE;
  if (days > 0) {
    return `in ${days}d ${hours}h`;
  }

  if (hours > 0) {
    return `in ${hours}h ${minutes}m`;
  }

  if (minutes > 0) {
    return `in ${minutes}m ${String(seconds).padStart(2, "0")}s`;
  }

  return `in ${seconds}s`;
}
