import { describe, expect, it } from "vitest";

import {
  costWeight,
  lightestToRestore,
  measuredPerEstimated,
  planShedding,
  sessionLoad,
  shedFraction
} from "@/lib/usage/pacing/engine/load-shedding";

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
  it("asks for the share of the spend the window doesn't let the account keep", () => {
    expect(shedFraction(0.25)).toBeCloseTo(0.75);
  });

  it("asks for everything at most, and nothing on pace", () => {
    expect(shedFraction(-0.5)).toBe(1);
    expect(shedFraction(1)).toBeNull();
    expect(shedFraction(1.7)).toBeNull();
  });
});

describe("planShedding", () => {
  const heavy = {
    id: "opus-alone",
    load: 20,
    saving: 0.4,
    fanOut: 1
  };
  const light = {
    id: "sonnet-alone",
    load: 3,
    saving: 0.4,
    fanOut: 1
  };
  const resting = {
    id: "haiku-resting",
    load: 1,
    saving: 1,
    fanOut: 1
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

  it("slows a session fanning work out to subagents before a lone one spending as much", () => {
    const fannedOut = {
      id: "opus-with-three-subagents",
      load: 12,
      saving: 0.4,
      fanOut: 4
    };
    expect(
      planShedding({
        candidates: [heavy, fannedOut],
        totalLoad: 32,
        fraction: 0.1
      })
    ).toEqual([fannedOut.id]);
  });

  it("still slows the lone session when the fanned-out ones can't cover the shed", () => {
    const fannedOut = {
      id: "opus-with-three-subagents",
      load: 12,
      saving: 0.4,
      fanOut: 4
    };
    expect(
      planShedding({
        candidates: [heavy, fannedOut],
        totalLoad: 32,
        fraction: 0.3
      })
    ).toEqual([fannedOut.id, heavy.id]);
  });

  it("never picks a session with no step left", () => {
    expect(
      planShedding({
        candidates: [
          {
            ...heavy,
            saving: 0
          }
        ],
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
