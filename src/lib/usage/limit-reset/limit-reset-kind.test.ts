import {
  limitResetExpiry,
  limitResetKind,
  limitResetTotal,
  spentResetPhrase
} from "@/lib/usage/limit-reset/limit-reset-kind";
import { describe, expect, it } from "vitest";

describe("limitResetKind", () => {
  it("names a Fable reset and says it gives back the week's Fable use", () => {
    expect(limitResetKind("fable")).toEqual({
      name: "Fable reset",
      title: "Fable reset",
      scope: "Fable",
      effect: "Resets your Fable limit, and takes this week's Fable use off your weekly limit"
    });
  });

  it("names each other kind after what it clears", () => {
    expect(limitResetKind("session").name).toBe("5-hour reset");
    expect(limitResetKind("weekly").name).toBe("weekly reset");
    expect(limitResetKind("full").name).toBe("full reset");
  });

  it("capitalises the title for a heading or a label", () => {
    expect(limitResetKind("weekly").title).toBe("Weekly reset");
  });
});

describe("spentResetPhrase", () => {
  it("says which reset a claim spends", () => {
    expect(
      spentResetPhrase({
        name: "Fable reset",
        resetsLeft: 1
      })
    ).toBe("your one Fable reset");
    expect(
      spentResetPhrase({
        name: "weekly reset",
        resetsLeft: 3
      })
    ).toBe("one of your 3 weekly resets");
  });
});

const NOW = Date.parse("2026-11-11T10:00:00Z");
const HOUR_MS = 3_600_000;

const fable = {
  grantId: "fable",
  label: "fable",
  resetsLeft: 3,
  endsAt: null,
  kind: "fable",
  hasUsageToClear: true
} as const;
const weekly = {
  grantId: "weekly",
  label: "weekly",
  resetsLeft: 1,
  endsAt: null,
  kind: "weekly",
  hasUsageToClear: true
} as const;

describe("limitResetTotal", () => {
  it("counts every reset across the kinds", () => {
    expect(limitResetTotal([weekly, fable])).toBe(4);
    expect(limitResetTotal([])).toBe(0);
  });
});

describe("limitResetExpiry", () => {
  it("counts down to the first expiry, and warns under a day out", () => {
    const soon = limitResetExpiry({
      offer: {
        ...fable,
        endsAt: new Date(NOW + 5 * HOUR_MS).toISOString()
      },
      now: NOW
    });
    expect(soon?.countdown).toBe("Expires in 5h 0m");
    expect(soon?.isSoon).toBe(true);
    expect(
      limitResetExpiry({
        offer: {
          ...fable,
          endsAt: new Date(NOW + 50 * HOUR_MS).toISOString()
        },
        now: NOW
      })?.isSoon
    ).toBe(false);
  });

  it("says nothing for a reset that never expires", () => {
    expect(
      limitResetExpiry({
        offer: fable,
        now: NOW
      })
    ).toBeNull();
  });
});

describe("limitResetKind scopes", () => {
  it("badges each kind by what it works on", () => {
    expect(limitResetKind("weekly").scope).toBe("All models");
    expect(limitResetKind("fable").scope).toBe("Fable");
  });
});
