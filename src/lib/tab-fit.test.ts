import { DOT_SLOT, MORE_SLOT, packTabs, TAB_GAP } from "@/lib/tab-fit";
import { describe, expect, it } from "vitest";

// Four 100px-wide tabs in strip order — wide enough that the tier boundaries
// land on round numbers (DOT_SLOT is 28, MORE_SLOT is 40 with the 6px gap).
const IDS = ["a", "b", "c", "d"];
const TOTAL = 400 + TAB_GAP * 3;

function widthOf(): number {
  return 100;
}

describe("packTabs", () => {
  it("keeps every tab a full pill when the strip is wide enough", () => {
    const pack = packTabs({
      ids: IDS,
      widthOf,
      stripWidth: TOTAL
    });

    expect(pack).toEqual({
      visible: IDS,
      dots: [],
      more: []
    });
  });

  it("keeps every tab a full pill before the strip is measured", () => {
    const pack = packTabs({
      ids: IDS,
      widthOf,
      stripWidth: 0
    });

    expect(pack.visible).toEqual(IDS);
  });

  it("reserves the overflow slot and fills the three tiers greedily", () => {
    // Budget is 190 - 40 = 150: one 100px pill fits, the second would need
    // 206. The 50px left after the pill holds one 28px dot, not two.
    const pack = packTabs({
      ids: IDS,
      widthOf,
      stripWidth: 190
    });

    expect(pack).toEqual({
      visible: ["a"],
      dots: ["b"],
      more: ["c", "d"]
    });
  });

  it("spends remaining room on dots before overflowing", () => {
    // Budget is 340 - 40 = 300: two pills (206) fit, and the 94px left
    // holds two more dots — nothing has to fall into "+N".
    const pack = packTabs({
      ids: IDS,
      widthOf,
      stripWidth: 340
    });

    expect(pack).toEqual({
      visible: ["a", "b"],
      dots: ["c", "d"],
      more: []
    });
  });

  it("always keeps at least one full pill, even when nothing fits", () => {
    const pack = packTabs({
      ids: IDS,
      widthOf,
      stripWidth: MORE_SLOT + 50
    });

    expect(pack.visible).toEqual(["a"]);
    expect(pack.dots).toEqual([]);
    expect(pack.more).toEqual(["b", "c", "d"]);
  });

  it("collapses a tab too wide for a pill into a fixed-width dot", () => {
    const widths = new Map([
      ["slim", 40],
      ["wide", 400]
    ]);
    const pack = packTabs({
      ids: ["slim", "wide"],
      widthOf: id => widths.get(id) ?? 0,
      stripWidth: 200
    });

    expect(pack.visible).toEqual(["slim"]);
    expect(pack.dots).toEqual(["wide"]);
  });

  it("returns empty tiers for an empty strip", () => {
    const pack = packTabs({
      ids: [],
      widthOf,
      stripWidth: 100
    });

    expect(pack).toEqual({
      visible: [],
      dots: [],
      more: []
    });
  });

  it("leaves a dot short of a full slot in the overflow", () => {
    // Budget 240 - 40 = 200: one pill (100), dot room 100 holds three 28px
    // dots with 16px left — the fourth tab must overflow.
    const pack = packTabs({
      ids: [...IDS, "e"],
      widthOf,
      stripWidth: 240
    });

    expect(pack.visible).toEqual(["a"]);
    expect(pack.dots).toEqual(["b", "c", "d"]);
    expect(pack.more).toEqual(["e"]);
    expect(DOT_SLOT).toBe(28);
  });
});
