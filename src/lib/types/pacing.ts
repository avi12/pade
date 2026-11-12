import { UsageWindowKind } from "@/lib/types/usage";
import { z } from "zod";

/** The steps usage pacing climbs, in order, while an agent stays on track to
 *  run out of usage before its window resets (lib/usage/pacing/pacing-policy). */
export const PacingStep = z.enum(["effort", "model", "rest", "stop"]);

export type PacingStep = z.infer<typeof PacingStep>;

/** The usage windows pacing watches. */
export const PacedWindowKind = UsageWindowKind.extract(["session", "weekly"]);

export type PacedWindowKind = z.infer<typeof PacedWindowKind>;

/** One running subagent's pacing step, as the agent's hook reads it
 *  (agents/running_model/pacing_control): held before each tool call, or told
 *  to finish now. */
export const SubagentStepKind = z.enum(["gate", "wrapUp"]);

export const SubagentStep = z.discriminatedUnion("step", [
  z.object({
    step: z.literal(SubagentStepKind.enum.gate),
    holdSeconds: z.number().int().positive()
  }),
  z.object({ step: z.literal(SubagentStepKind.enum.wrapUp) })
]);

export type SubagentStep = z.infer<typeof SubagentStep>;

/** The account-wide pacing round's part of one session's pacing control. */
const PacingThrottle = z.object({
  isBlockingSpawns: z.boolean(),
  subagents: z.record(z.string(), SubagentStep)
});

export type PacingThrottle = z.infer<typeof PacingThrottle>;

/** The parts of a session's pacing control, each written by its own owner. */
export const PacingControlPartKind = z.enum(["cap", "stop", "rest", "throttle"]);

/** When a rest ends (epoch ms) - the hook holds the session's tool calls until
 *  then; null lifts the rest. */
const PacingRestUntil = z.number().int().nonnegative().nullable();

export type PacingRestUntil = z.infer<typeof PacingRestUntil>;
