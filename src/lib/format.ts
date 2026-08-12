// Locale-aware number formatting (DRY). Prefer these over hand-rolled
// round / toFixed / string-building so digits group and localize consistently.

const INTEGER = new Intl.NumberFormat(undefined, {
  maximumFractionDigits: 0
});

/** A whole number with locale grouping — 1234 → "1,234". */
export function formatCount(value: number): string {
  return INTEGER.format(value);
}

/** A rounded whole percent — 30.4 → "30%". */
export function formatPercent(value: number): string {
  return `${INTEGER.format(value)}%`;
}

// Precise date + time (to the second), locale-aware. One home for the exact
// timestamp shown behind a relative "3m ago" label (Change Feed, commit log …).
const TIMESTAMP = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "medium"
});

/** The exact date-time for `epochMilliseconds` — e.g. "Jul 19, 2026, 7:34:12 AM".
 *  Pairs with a relative label as its hover tooltip. */
export function formatTimestamp(epochMilliseconds: number): string {
  return TIMESTAMP.format(new Date(epochMilliseconds));
}

/** How long ago `stamp` was, at second → minute → hour granularity ("41s",
 *  "3m", "2h"). One home for every relative age the UI prints — the Change
 *  Feed's card labels and its "watching since" line read the same clock. */
export function formatAge({ stamp, now }: {
  stamp: number;
  now: number;
}): string {
  const seconds = Math.max(0, Math.round((now - stamp) / 1000));
  if (seconds < 60) {
    return `${formatCount(seconds)}s`;
  }

  if (seconds < 3600) {
    return `${formatCount(Math.round(seconds / 60))}m`;
  }

  return `${formatCount(Math.round(seconds / 3600))}h`;
}
