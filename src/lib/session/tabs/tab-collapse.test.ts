import { collapseTab } from "@/lib/session/tabs/tab-collapse";
import { describe, expect, it } from "vitest";

const PILL = {
  width: 120,
  height: 26
};

describe("collapseTab", () => {
  it("starts at the pill's full width with no tuck", () => {
    expect(collapseTab(PILL).css(1)).toBe(
      "overflow: hidden; block-size: 26px; inline-size: 120px;opacity: 1; margin-inline-start: 0px;"
    );
  });

  it("ends at zero width, faded and tucked under its neighbour", () => {
    expect(collapseTab(PILL).css(0)).toBe(
      "overflow: hidden; block-size: 26px; inline-size: 0px;opacity: 0; margin-inline-start: -6px;"
    );
  });

  it("pins the height throughout the collapse", () => {
    expect(collapseTab(PILL).css(0.5)).toContain("block-size: 26px;");
  });
});
