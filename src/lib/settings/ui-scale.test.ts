import { UI_SCALE_MAXIMUM, UI_SCALE_MINIMUM, UI_SCALE_STEP } from "@/lib/settings/prefs-bounds";
import { steppedUiScale } from "@/lib/settings/ui-scale";
import { describe, expect, it } from "vitest";

describe("steppedUiScale", () => {
  it("steps up and down by the given delta", () => {
    expect(
      steppedUiScale({
        scale: 1,
        delta: UI_SCALE_STEP
      })
    ).toBe(1.05);
    expect(
      steppedUiScale({
        scale: 1,
        delta: -UI_SCALE_STEP
      })
    ).toBe(0.95);
  });

  it("rounds away accumulated float drift", () => {
    expect(
      steppedUiScale({
        scale: 1.1,
        delta: UI_SCALE_STEP
      })
    ).toBe(1.15);
  });

  it("holds the scale inside its bounds", () => {
    expect(
      steppedUiScale({
        scale: UI_SCALE_MAXIMUM,
        delta: UI_SCALE_STEP
      })
    ).toBe(UI_SCALE_MAXIMUM);
    expect(
      steppedUiScale({
        scale: UI_SCALE_MINIMUM,
        delta: -UI_SCALE_STEP
      })
    ).toBe(UI_SCALE_MINIMUM);
  });
});
