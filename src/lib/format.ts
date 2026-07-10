// Locale-aware number formatting (DRY). Prefer these over hand-rolled
// round / toFixed / string-building so digits group and localize consistently.

const INTEGER = new Intl.NumberFormat(undefined, {
  maximumFractionDigits: 0
});

const COMPACT = new Intl.NumberFormat(undefined, {
  notation: "compact",
  maximumFractionDigits: 1
});

/** A whole number with locale grouping — 1234 → "1,234". */
export function formatCount(value: number): string {
  return INTEGER.format(value);
}

/** A compact large number — 1250 → "1.3K", 1_000_000 → "1M". */
export function formatCompact(value: number): string {
  return COMPACT.format(value);
}

/** A rounded whole percent — 30.4 → "30%". */
export function formatPercent(value: number): string {
  return `${INTEGER.format(value)}%`;
}
