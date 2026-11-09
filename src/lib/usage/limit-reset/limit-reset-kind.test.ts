import { limitResetKind, spentResetPhrase } from "@/lib/usage/limit-reset/limit-reset-kind";
import { describe, expect, it } from "vitest";

describe("limitResetKind", () => {
  it("names a grant that clears only the included model's week a Fable reset", () => {
    expect(limitResetKind(["seven_day_overage_included"])).toEqual({
      name: "Fable reset",
      title: "Fable reset",
      effect: "Resets your Fable limit, and takes this week's Fable use off your weekly limit"
    });
  });

  it("names a grant that clears the session and the week a full reset", () => {
    expect(limitResetKind(["five_hour", "seven_day", "seven_day_overage_included"]).name).toBe("full reset");
  });

  it("names single-window grants after their window", () => {
    expect(limitResetKind(["five_hour"]).name).toBe("5-hour reset");
    expect(limitResetKind(["seven_day"]).name).toBe("weekly reset");
  });

  it("capitalises the title for a heading or a label", () => {
    expect(limitResetKind(["seven_day"]).title).toBe("Weekly reset");
  });

  it("falls back to a plain limit reset for an unknown set", () => {
    expect(limitResetKind([]).name).toBe("limit reset");
    expect(limitResetKind(["seven_day_something_new"]).name).toBe("limit reset");
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
