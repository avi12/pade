import { Key } from "@/lib/ui/keys";
import { arrowListTarget, nextIndex, openFocusIndex } from "@/lib/ui/roving-menu";
import { describe, expect, it } from "vitest";

describe("nextIndex - menu arrow navigation", () => {
  it("ArrowDown advances and wraps; from -1 (no item focused) lands on the first", () => {
    expect(
      nextIndex({
        key: "ArrowDown",
        from: -1,
        count: 3
      })
    ).toBe(0);
    expect(
      nextIndex({
        key: "ArrowDown",
        from: 0,
        count: 3
      })
    ).toBe(1);
    expect(
      nextIndex({
        key: "ArrowDown",
        from: 2,
        count: 3
      })
    ).toBe(0);
  });

  it("ArrowUp retreats and wraps; from -1 lands on the last", () => {
    expect(
      nextIndex({
        key: "ArrowUp",
        from: -1,
        count: 3
      })
    ).toBe(2);
    expect(
      nextIndex({
        key: "ArrowUp",
        from: 0,
        count: 3
      })
    ).toBe(2);
    expect(
      nextIndex({
        key: "ArrowUp",
        from: 2,
        count: 3
      })
    ).toBe(1);
  });

  it("Home and End jump to the ends", () => {
    expect(
      nextIndex({
        key: "Home",
        from: 2,
        count: 3
      })
    ).toBe(0);
    expect(
      nextIndex({
        key: "End",
        from: 0,
        count: 3
      })
    ).toBe(2);
  });

  it("ignores keys the menu doesn't own (Tab leaves) and an empty menu", () => {
    expect(
      nextIndex({
        key: "Tab",
        from: 0,
        count: 3
      })
    ).toBeNull();
    expect(
      nextIndex({
        key: "a",
        from: 0,
        count: 3
      })
    ).toBeNull();
    expect(
      nextIndex({
        key: "ArrowDown",
        from: -1,
        count: 0
      })
    ).toBeNull();
  });
});

describe("arrowListTarget - several lists in one popover", () => {
  it("walks a list from one of its items and wraps", () => {
    expect(
      arrowListTarget({
        key: Key.ArrowDown,
        isEntry: false,
        from: 2,
        count: 3
      })
    ).toBe(0);
    expect(
      arrowListTarget({
        key: Key.ArrowUp,
        isEntry: false,
        from: 1,
        count: 3
      })
    ).toBe(0);
    expect(
      arrowListTarget({
        key: Key.End,
        isEntry: false,
        from: 0,
        count: 3
      })
    ).toBe(2);
  });

  it("steps in from the entry: Down to the first item, Up to the last", () => {
    expect(
      arrowListTarget({
        key: Key.ArrowDown,
        isEntry: true,
        from: -1,
        count: 3
      })
    ).toBe(0);
    expect(
      arrowListTarget({
        key: Key.ArrowUp,
        isEntry: true,
        from: -1,
        count: 3
      })
    ).toBe(2);
  });

  it("leaves Home, End and the side arrows to the entry's own pattern", () => {
    for (const key of [Key.Home, Key.End, Key.ArrowLeft, Key.ArrowRight]) {
      expect(
        arrowListTarget({
          key,
          isEntry: true,
          from: -1,
          count: 3
        })
      ).toBeNull();
    }
  });

  it("goes nowhere in an empty list", () => {
    expect(
      arrowListTarget({
        key: Key.ArrowDown,
        isEntry: true,
        from: -1,
        count: 0
      })
    ).toBeNull();
  });
});

describe("openFocusIndex - who takes focus when the popover opens", () => {
  it("is the first candidate on screen - the first project row", () => {
    expect(openFocusIndex([true, true, true])).toBe(0);
  });

  it("falls to the selected tab, the only candidate, when there are no rows", () => {
    expect(openFocusIndex([true])).toBe(0);
  });

  it("skips candidates that are not displayed", () => {
    expect(openFocusIndex([false, true])).toBe(1);
  });

  it("is none when nothing is displayed", () => {
    expect(openFocusIndex([false])).toBe(-1);
  });
});
