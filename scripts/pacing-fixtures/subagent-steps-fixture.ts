// The subagent-steps fixture: the eased step ladder and holds for saving.

import { SubagentStepKind } from "@/lib/types/pacing";
import type { SubagentStep } from "@/lib/types/pacing";
import { easedSubagentStep, holdForSaving, nextSubagentStep } from "@/lib/usage/pacing/engine/subagent-steps";

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
    holdForSaving: Array.from({ length: 101 }, (_, i) => i / 100).map(fraction => ({
      fraction,
      expected: holdForSaving(fraction)
    })),
    nextSubagentStep: currents.flatMap(current => [null, 0, 0.1, 0.5, 0.99, 1].map(shed => ({
      current,
      shed,
      expected: nextSubagentStep({
        current: current ?? undefined,
        shed
      })
    }))),
    easedSubagentStep: steps.map(current => ({
      current,
      expected: easedSubagentStep(current)
    }))
  };
}
