// The load-shedding fixture: cost weights, shed fractions and shedding plans.

import { costWeight, planShedding, shedFraction } from "@/lib/usage/pacing/engine/load-shedding";
import type { ShedCandidate } from "@/lib/usage/pacing/engine/load-shedding";

import { between, pick, random } from "./fixture-primitives";

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
  return MODELS.flatMap(model =>
    EFFORTS.flatMap(effort =>
      LADDERS.map(ladder => ({
        model,
        effort: effort ?? null,
        ladder,
        expected: costWeight({
          model,
          effort,
          ladder
        })
      }))
    )
  );
}

const PLAN_CASE_COUNT = 30;

/** Keep shares from past empty to room to spare. */
const KEEP_SHARES = [-0.5, 0, 0.05, 0.3, 0.6, 0.95, 1, 1.7];

function shedFractionCases() {
  return KEEP_SHARES.map(keep => ({
    keep,
    expected: shedFraction(keep)
  }));
}

function planSheddingCases() {
  return Array.from({ length: PLAN_CASE_COUNT }, () => {
    const candidates: ShedCandidate[] = Array.from({ length: 1 + Math.floor(random() * 5) }, (_, i) => ({
      id: `candidate-${i}`,
      load: between({
        low: 1,
        high: 60,
        step: 0.5
      }),
      saving: pick([0, 0.2, 0.4, 0.5, 1]),
      fanOut: pick([1, 1, 2, 4])
    }));
    const totalLoad =
      candidates.reduce((sum, candidate) => sum + candidate.load, 0) +
      between({
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
