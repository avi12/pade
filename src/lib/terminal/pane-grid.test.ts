import { gridForPane, squeezeToFit } from "@/lib/terminal/pane-grid";
import { describe, expect, it } from "vitest";

const CELL = {
  width: 8,
  height: 16
};

describe("gridForPane", () => {
  it("rounds whole cells down after reserving the scrollbar", () => {
    expect(
      gridForPane({
        width: 814 + 7,
        height: 16 * 30 + 15,
        cell: CELL
      })
    ).toEqual({
      columns: 100,
      rows: 30
    });
  });

  it("has no grid until the font metric is usable", () => {
    expect(
      gridForPane({
        width: 800,
        height: 600,
        cell: undefined
      })
    ).toBeUndefined();
    expect(
      gridForPane({
        width: 800,
        height: 600,
        cell: {
          width: 0,
          height: 16
        }
      })
    ).toBeUndefined();
  });

  it("refuses a transient sliver instead of shrinking the agent to it", () => {
    expect(
      gridForPane({
        width: 100,
        height: 600,
        cell: CELL
      })
    ).toBeUndefined();
    expect(
      gridForPane({
        width: 800,
        height: 40,
        cell: CELL
      })
    ).toBeUndefined();
  });
});

describe("squeezeToFit", () => {
  it("is exactly 1 for a grid that fits", () => {
    expect(
      squeezeToFit({
        rows: 25,
        cellHeight: 16,
        paneHeight: 480
      })
    ).toBe(1);
    expect(
      squeezeToFit({
        rows: 0,
        cellHeight: 16,
        paneHeight: 0
      })
    ).toBe(1);
  });

  it("scales a too-tall grid down to the pane", () => {
    expect(
      squeezeToFit({
        rows: 50,
        cellHeight: 16,
        paneHeight: 400
      })
    ).toBe(0.5);
  });

  it("has nothing to say until the font metric has a height", () => {
    expect(
      squeezeToFit({
        rows: 50,
        cellHeight: undefined,
        paneHeight: 400
      })
    ).toBeUndefined();
    expect(
      squeezeToFit({
        rows: 50,
        cellHeight: 0,
        paneHeight: 400
      })
    ).toBeUndefined();
  });
});
