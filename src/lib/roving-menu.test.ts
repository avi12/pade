import { nextIndex } from "@/lib/roving-menu";
import { describe, expect, it } from "vitest";

describe("nextIndex — menu arrow navigation", () => {
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
