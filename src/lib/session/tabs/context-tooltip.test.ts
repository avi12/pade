import { contextTooltip } from "@/lib/session/tabs/context-tooltip";
import { formatPercent } from "@/lib/util/format";
import { describe, expect, it } from "vitest";

describe("contextTooltip", () => {
  it("states the agent's own measurement as fact", () => {
    expect(
      contextTooltip({
        measured: 42,
        estimate: 90
      })
    ).toBe(`${formatPercent(42)} of context window used`);
  });

  it("labels the byte estimate as rough when nothing is measured", () => {
    expect(
      contextTooltip({
        measured: null,
        estimate: 17
      })
    ).toBe(`≈${formatPercent(17)} of context window used - rough estimate`);
  });

  it("says it is still measuring before any signal arrives", () => {
    expect(
      contextTooltip({
        measured: null,
        estimate: null
      })
    ).toBe("Context window - measuring…");
  });
});
