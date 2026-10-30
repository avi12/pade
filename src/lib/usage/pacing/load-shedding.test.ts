import { UsageWindowKind } from "@/lib/types";
import {
  costWeight,
  lightestToRestore,
  measuredPerEstimated,
  planShedding,
  sessionLoad,
  shedFraction
} from "@/lib/usage/pacing/load-shedding";
import { describe, expect, it } from "vitest";

const MINUTE_MS = 60_000;
const NOW = Date.UTC(2026, 9, 30, 12);

describe("costWeight", () => {
  it("prices a dearer tier above a cheaper one at the same effort", () => {
    const opus = costWeight({
      model: "claude-opus-5-5",
      effort: "high"
    });
    const sonnet = costWeight({
      model: "claude-sonnet-5-5",
      effort: "high"
    });
    const haiku = costWeight({
      model: "claude-haiku-5-5",
      effort: "high"
    });
    expect(opus).toBeGreaterThan(sonnet);
    expect(sonnet).toBeGreaterThan(haiku);
  });

  it("prices the tiers by their long-context list-price ratio", () => {
    const haiku = costWeight({
      model: "claude-haiku-5-5"
    });
    expect(
      costWeight({
        model: "claude-sonnet-5-5"
      }) / haiku
    ).toBe(4);
    expect(
      costWeight({
        model: "claude-opus-5-5"
      }) / haiku
    ).toBe(8);
    expect(
      costWeight({
        model: "claude-fable-5-1"
      }) / haiku
    ).toBe(20);
  });

  it("prices more effort above less on the same model", () => {
    const high = costWeight({
      model: "claude-opus-5-5",
      effort: "max"
    });
    const low = costWeight({
      model: "claude-opus-5-5",
      effort: "low"
    });
    expect(high).toBeGreaterThan(low);
  });

  it("puts an unknown model and effort at the middle", () => {
    expect(
      costWeight({
        model: "mystery-model"
      })
    ).toBe(
      costWeight({
        model: "claude-sonnet-5-5"
      })
    );
  });
});

describe("measuredPerEstimated", () => {
  it("scales an estimate onto what the measured sessions burn", () => {
    expect(
      measuredPerEstimated([
        {
          measured: 3000,
          estimated: 2
        },
        {
          measured: 1000,
          estimated: 2
        }
      ])
    ).toBe(1000);
  });

  it("leaves an estimate as it is while nothing is measured", () => {
    expect(measuredPerEstimated([])).toBe(1);
    expect(
      measuredPerEstimated([
        {
          measured: 500,
          estimated: 0
        }
      ])
    ).toBe(1);
  });
});

describe("sessionLoad", () => {
  it("grows with each subagent the session runs", () => {
    const alone = sessionLoad({
      cost: 5,
      workingShare: 1,
      subagents: 0
    });
    const withThree = sessionLoad({
      cost: 5,
      workingShare: 1,
      subagents: 3
    });
    expect(withThree).toBe(alone * 4);
  });

  it("weighs a session by how much of the time it works", () => {
    expect(
      sessionLoad({
        cost: 4,
        workingShare: 0.25,
        subagents: 0
      })
    ).toBe(1);
  });
});

describe("shedFraction", () => {
  const window = {
    key: "five_hour",
    kind: UsageWindowKind.enum.session,
    label: "Five hour",
    utilization: 80,
    resetsAt: null
  };

  it("asks for the share the burn runs over the pace that lasts", () => {
    // 20% left over 100 minutes lasts at 0.2%/min; burning 0.8%/min is 4x over.
    expect(
      shedFraction({
        window,
        rate: 0.8 / MINUTE_MS,
        freeAt: NOW + 100 * MINUTE_MS,
        now: NOW
      })
    ).toBeCloseTo(0.75);
  });

  it("asks for nothing on pace or while the burn is unmeasured", () => {
    expect(
      shedFraction({
        window,
        rate: 0.1 / MINUTE_MS,
        freeAt: NOW + 100 * MINUTE_MS,
        now: NOW
      })
    ).toBeNull();
    expect(
      shedFraction({
        window,
        rate: null,
        freeAt: NOW + 100 * MINUTE_MS,
        now: NOW
      })
    ).toBeNull();
  });
});

describe("planShedding", () => {
  const heavy = {
    id: "opus-with-subagents",
    load: 20,
    saving: 0.4
  };
  const light = {
    id: "sonnet-alone",
    load: 3,
    saving: 0.4
  };
  const resting = {
    id: "haiku-resting",
    load: 1,
    saving: 1
  };

  it("steps the biggest saving first and stops once the shed is covered", () => {
    expect(
      planShedding({
        candidates: [light, heavy, resting],
        totalLoad: 24,
        fraction: 0.3
      })
    ).toEqual([heavy.id]);
  });

  it("keeps stepping down the ranking while the shed isn't covered", () => {
    expect(
      planShedding({
        candidates: [light, heavy, resting],
        totalLoad: 24,
        fraction: 0.35
      })
    ).toEqual([heavy.id, light.id]);
  });

  it("steps only the biggest saving while the shed is unknown", () => {
    expect(
      planShedding({
        candidates: [light, heavy],
        totalLoad: 23,
        fraction: null
      })
    ).toEqual([heavy.id]);
  });

  it("never picks a session with no step left", () => {
    expect(
      planShedding({
        candidates: [{
          ...heavy,
          saving: 0
        }],
        totalLoad: 20,
        fraction: 0.5
      })
    ).toEqual([]);
  });
});

describe("lightestToRestore", () => {
  it("steps the lightest session back up first", () => {
    expect(
      lightestToRestore([
        {
          id: "heavy",
          load: 9
        },
        {
          id: "light",
          load: 2
        }
      ])
    ).toBe("light");
  });

  it("has nothing to restore when no session is stepped down", () => {
    expect(lightestToRestore([])).toBeNull();
  });
});
