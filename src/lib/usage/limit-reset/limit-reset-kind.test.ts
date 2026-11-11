import {
  limitResetChip,
  limitResetKind,
  NOTHING_TO_RESET,
  spentResetPhrase
} from "@/lib/usage/limit-reset/limit-reset-kind";
import { describe, expect, it } from "vitest";

describe("limitResetKind", () => {
  it("names a Fable reset and says it gives back the week's Fable use", () => {
    expect(limitResetKind("fable")).toEqual({
      name: "Fable reset",
      title: "Fable reset",
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

describe("limitResetChip", () => {
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

  it("names a single kind and says what it frees up", () => {
    expect(limitResetChip([fable])).toEqual({
      label: "Fable reset",
      hint: "Resets your Fable limit, and takes this week's Fable use off your weekly limit · 3 left"
    });
  });

  it("says when a single kind has nothing to clear", () => {
    expect(
      limitResetChip([{
        ...fable,
        hasUsageToClear: false
      }]).hint
    ).toBe(`${NOTHING_TO_RESET} · 3 left`);
  });

  it("counts several kinds and lists each in the hint", () => {
    expect(limitResetChip([weekly, fable])).toEqual({
      label: "4 resets",
      hint: "Weekly reset: 1 left\nFable reset: 3 left\nClick to pick which one to use"
    });
  });
});
