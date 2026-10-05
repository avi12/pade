import { formatAge, formatCount, formatPercent, formatTimestamp } from "@/lib/format";
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

describe("formatAge", () => {
  const now = 10_000_000_000;
  function secondsAgo(seconds: number): string {
    return formatAge({
      stamp: now - seconds * 1000,
      now
    });
  }

  it("steps from seconds through minutes and hours to days", () => {
    expect(secondsAgo(41)).toBe("41s");
    expect(secondsAgo(3 * 60)).toBe("3m");
    expect(secondsAgo(2 * 3600)).toBe("2h");
    expect(secondsAgo(5 * 86_400)).toBe("5d");
  });

  it("switches to days at a full day", () => {
    expect(secondsAgo(86_399)).toBe("24h");
    expect(secondsAgo(86_400)).toBe("1d");
  });

  it("never prints a negative age for a stamp in the future", () => {
    expect(secondsAgo(-30)).toBe("0s");
  });
});
