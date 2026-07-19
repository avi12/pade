import { formatCount, formatPercent, formatTimestamp } from "@/lib/format";
import { describe, expect, it } from "vitest";

// The wrappers delegate localisation to Intl, so these assertions pin what the
// module owns — rounding, digit preservation, suffixes — not any one locale's
// separators (they assume only a latin-digit locale, true of dev machines).
describe("formatCount", () => {
  it("formats a small integer as plain digits", () => {
    expect(formatCount(7)).toBe("7");
  });

  it("rounds fractions to a whole number", () => {
    expect(formatCount(1.6)).toBe("2");
  });

  it("groups thousands without losing digits", () => {
    const formatted = formatCount(1234567);

    expect(formatted.replaceAll(/\D/gu, "")).toBe("1234567");
    expect(formatted.length).toBeGreaterThan("1234567".length);
  });
});

describe("formatPercent", () => {
  it("rounds to a whole percent and appends the sign", () => {
    expect(formatPercent(30.4)).toBe("30%");
  });

  it("rounds up from the half", () => {
    expect(formatPercent(66.6)).toBe("67%");
  });

  it("formats zero", () => {
    expect(formatPercent(0)).toBe("0%");
  });
});

describe("formatTimestamp", () => {
  it("renders a precise date + time for an epoch-ms value", () => {
    // Noon UTC stays on the same calendar day in every timezone, so the year is
    // stable regardless of where the test runs; the exact separators are Intl's.
    const noonUtc = Date.UTC(2026, 6, 19, 12, 0, 0);

    const formatted = formatTimestamp(noonUtc);
    expect(formatted).toMatch(/2026/);
    expect(formatted.length).toBeGreaterThan(8);
  });

  it("distinguishes two different instants", () => {
    expect(formatTimestamp(0)).not.toBe(formatTimestamp(1_000_000_000_000));
  });
});
