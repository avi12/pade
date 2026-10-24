import { StartMode, TEMP_CLEANUP_DAY_CHOICES } from "@/lib/types";
import { START_MODE_CHOICES, TEMP_CLEANUP_CHOICES } from "@/panels/picker/on-launch";
import { describe, expect, it } from "vitest";

describe("on-launch choices", () => {
  it("offers every start mode once", () => {
    expect(START_MODE_CHOICES.map(choice => choice.value)).toEqual(StartMode.options);
  });

  it("labels each cleanup choice in days, with Off for never", () => {
    expect(TEMP_CLEANUP_CHOICES.map(choice => choice.value)).toEqual([...TEMP_CLEANUP_DAY_CHOICES]);
    expect(TEMP_CLEANUP_CHOICES.map(choice => choice.label)).toEqual(["Off", "7 days", "30 days", "90 days"]);
  });
});
