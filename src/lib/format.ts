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
