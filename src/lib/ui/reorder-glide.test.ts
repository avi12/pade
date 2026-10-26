import { glideOffset, risenKey } from "@/lib/ui/reorder-glide";
import { describe, expect, it } from "vitest";

describe("risenKey", () => {
  it("is the key of a row that moved toward the top", () => {
    expect(
      risenKey({
        key: "c",
        from: ["a", "b", "c"],
        to: ["c", "a", "b"]
      })
    ).toBe("c");
  });

  it("is nothing for a row pushed down, one that just joined, or no key", () => {
    expect(
      risenKey({
        key: "a",
        from: ["a", "b", "c"],
        to: ["c", "a", "b"]
      })
    ).toBeUndefined();
    expect(
      risenKey({
        key: "d",
        from: ["a", "b"],
        to: ["d", "a", "b"]
      })
    ).toBeUndefined();
    expect(
      risenKey({
        key: undefined,
        from: ["a", "b"],
        to: ["b", "a"]
      })
    ).toBeUndefined();
  });
});

describe("glideOffset", () => {
  it("starts a row that moved up from where it was painted, below its new box", () => {
    expect(
      glideOffset({
        from: {
          left: 6,
          top: 271
        },
        to: {
          left: 6,
          top: 118
        }
      })
    ).toEqual({
      x: 0,
      y: 153
    });
  });

  it("is no distance for a row that kept its place", () => {
    expect(
      glideOffset({
        from: {
          left: 6,
          top: 118
        },
        to: {
          left: 6,
          top: 118
        }
      })
    ).toEqual({
      x: 0,
      y: 0
    });
  });
});
