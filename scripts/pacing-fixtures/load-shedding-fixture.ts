// The load-shedding fixture: cost weights, shed fractions and shedding plans.

import { UsageWindowKind } from "@/lib/types/usage";
import { costWeight, planShedding, shedFraction } from "@/lib/usage/pacing/load-shedding";
import type { ShedCandidate } from "@/lib/usage/pacing/load-shedding";
import { HOUR, MINUTE, NOW, between, perHour, pick, random } from "./fixture-primitives";
import { windowOf } from "./usage-pace-fixture";

const MODELS = [
  "claude-haiku-5-5",
  "claude-sonnet-5-5",
  "claude-opus-5-5[1m]",
  "claude-fable-5-1",
  "anthropic/claude-opus-5",
  "gpt-5.6-sol",
  "gpt-5-mini",
  "mystery-model"
];

const EFFORTS = [undefined, "minimal", "low", "medium", "high", "xhigh", "max", "HIGH", "turbo"];

const LADDERS: string[][] = [[], ["low", "high"]];

function costWeightCases() {
  return MODELS.flatMap(model => EFFORTS.flatMap(effort => LADDERS.map(ladder => ({
    model,
    effort: effort ?? null,
    ladder,
    expected: costWeight({
      model,
      effort,
      ladder
    })
  }))));
}

function shedFractionCases() {
  const rates = [null, 0, perHour(1), perHour(10), perHour(60)];
  const timesLeft = [10 * MINUTE, 100 * MINUTE, 5 * HOUR];
  return [60, 80, 95].flatMap(utilization => timesLeft.flatMap(timeLeft => rates.map(rate => ({
    utilization,
    timeLeft,
    rate,
    expected: shedFraction({
      window: windowOf({
        kind: UsageWindowKind.enum.session,
        utilization,
        resetsIn: timeLeft
      }),
      rate,
      freeAt: NOW + timeLeft,
      now: NOW
    })
  }))));
}

function planSheddingCases() {
  return Array.from({ length: 30 }, () => {
    const candidates: ShedCandidate[] = Array.from({ length: 1 + Math.floor(random() * 5) }, (_, i) => ({
      id: `candidate-${i}`,
      load: between({
        low: 1,
        high: 60,
        step: 0.5
      }),
      saving: pick([0, 0.2, 0.4, 0.5, 1])
    }));
    const totalLoad = candidates.reduce((sum, candidate) => sum + candidate.load, 0) + between({
      low: 0,
      high: 30,
      step: 1
    });
    const fraction = pick([null, 0.1, 0.35, 0.6, 0.95]);
    return {
      candidates,
      totalLoad,
      fraction,
      expected: planShedding({
        candidates,
        totalLoad,
        fraction
      })
    };
  });
}

export function loadSheddingFixture() {
  return {
    costWeight: costWeightCases(),
    shedFraction: shedFractionCases(),
    planShedding: planSheddingCases()
  };
}
