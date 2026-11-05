import { runningModelLook } from "@/lib/model/running-model";
import { describe, expect, it } from "vitest";

const OPUS = {
  id: "claude-opus-5-5",
  name: "Opus 5.5",
  isUltracode: false
};

describe("runningModelLook", () => {
  it("reads the model, its effort tone and its bars", () => {
    const look = runningModelLook({
      running: {
        ...OPUS,
        effort: "high"
      }
    });
    expect(look.label).toBe("Opus 5.5 · high effort");
    expect(look.tone).toBe(3);
    expect(look.bars.filter(bar => bar.isLit)).toHaveLength(3);
  });

  it("has no tone and no bars without an effort", () => {
    expect(
      runningModelLook({
        running: {
          ...OPUS,
          effort: null
        }
      })
    ).toEqual({
      label: "Opus 5.5",
      tone: null,
      bars: []
    });
  });

  it("ranks the effort on the model's own catalog ladder", () => {
    const look = runningModelLook({
      running: {
        ...OPUS,
        effort: "high"
      },
      ladder: ["low", "high"]
    });
    expect(look.tone).toBe(5);
    expect(look.bars).toHaveLength(2);
  });

  it("names Ultracode when it is on", () => {
    expect(
      runningModelLook({
        running: {
          ...OPUS,
          effort: null,
          isUltracode: true
        }
      }).label
    ).toBe("Opus 5.5 · Ultracode on");
  });
});
