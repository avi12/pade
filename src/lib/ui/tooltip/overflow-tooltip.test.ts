import { describe, expect, it } from "vitest";

import {
  clearOverflowTooltip,
  overflowTooltipState,
  showOverflowTooltip
} from "@/lib/ui/tooltip/overflow-tooltip.svelte";

const OWNER = "--owner";
const NEWER = "--newer";

describe("clearOverflowTooltip", () => {
  it("clears after the current task, so a render that moves a focused trigger never writes state", async () => {
    showOverflowTooltip({
      anchorName: OWNER,
      text: "clipped"
    });
    clearOverflowTooltip(OWNER);
    expect(overflowTooltipState.active?.anchorName).toBe(OWNER);

    await Promise.resolve();
    expect(overflowTooltipState.active).toBeNull();
  });

  it("leaves a bubble another trigger took over", async () => {
    showOverflowTooltip({
      anchorName: OWNER,
      text: "clipped"
    });
    clearOverflowTooltip(OWNER);
    showOverflowTooltip({
      anchorName: NEWER,
      text: "other"
    });

    await Promise.resolve();
    expect(overflowTooltipState.active?.anchorName).toBe(NEWER);
  });
});
