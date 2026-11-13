// The subagent-steps fixture: the eased step ladder and holds for saving.

import { SubagentStepKind } from "@/lib/types/pacing";
import type { SubagentStep } from "@/lib/types/pacing";
import { easedSubagentStep, holdForSaving, nextSubagentStep } from "@/lib/usage/pacing/engine/subagent-steps";

const HOLD_FRACTION_STEPS = 100;

const SHED_FRACTIONS = [null, 0, 0.1, 0.5, 0.99, 1];

export function gate(holdSeconds: number) {
  return {
    step: SubagentStepKind.enum.gate,
    holdSeconds
  } satisfies SubagentStep;
}

export const WRAP_UP = { step: SubagentStepKind.enum.wrapUp } satisfies SubagentStep;

export function subagentStepsFixture() {
  const holds = [5, 10, 20, 25, 40, 44, 45];
  const steps: SubagentStep[] = [...holds.map(gate), WRAP_UP];
  const currents = [null, ...steps];
  return {
    holdForSaving: Array.from({ length: HOLD_FRACTION_STEPS + 1 }, (_, i) => i / HOLD_FRACTION_STEPS).map(fraction => ({
      fraction,
      expected: holdForSaving(fraction)
    })),
    nextSubagentStep: currents.flatMap(current =>
      SHED_FRACTIONS.map(shed => ({
        current,
        shed,
        expected: nextSubagentStep({
          current: current ?? undefined,
          shed
        })
      }))
    ),
    easedSubagentStep: steps.map(current => ({
      current,
      expected: easedSubagentStep(current)
    }))
  };
}
