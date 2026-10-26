import { glideOffset } from "@/lib/ui/reorder-glide";
import { describe, expect, it } from "vitest";

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
