import { formatCompact, formatCount, formatPercent } from "@/lib/format";
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

describe("formatCompact", () => {
  it("keeps small values unshortened", () => {
    expect(formatCompact(950)).toBe("950");
  });

  it("shortens large values with a compact unit", () => {
    const compact = formatCompact(1_500_000);

    expect(compact).toContain("1");
    expect(compact.length).toBeLessThan(formatCount(1_500_000).length);
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
