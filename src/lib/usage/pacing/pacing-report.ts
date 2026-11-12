// What a window tells the backend's pacing engine each scan (the shadow phase:
// the engine only compares what it would decide with what this window decided).
// The wire shapes are the contract with src-tauri/src/pacing/facts.rs.

import { PacedWindowKind } from "@/lib/types";
import { SubagentReport } from "@/lib/usage/pacing/account-round";
import { z } from "zod";

/** Where a paced session is: asked to stop and still finishing its step,
 *  stopped and waiting for room, or restarted by the user while over pace. */
export const PacePhase = z.enum(["pausing", "paused", "overridden"]);

/** What pacing decided about one session in a scan. */
export const ObservedKind = z.enum([
  "stepEffortDown",
  "stepModelDown",
  "stepUp",
  "rest",
  "stop",
  "resume",
  "release",
  "none"
]);
export type ObservedKind = z.infer<typeof ObservedKind>;

const EpochMilliseconds = z.number();

const ObservedDecision = z.object({
  sessionId: z.string().min(1),
  at: EpochMilliseconds,
  kind: ObservedKind
});
export type ObservedDecision = z.infer<typeof ObservedDecision>;

const ChoiceSettings = z.object({
  model: z.string().nullable(),
  effort: z.string().nullable()
});

const ChoiceFacts = z.object({
  model: z.string().nullable(),
  effort: z.string().nullable(),
  /** What the session ran before pacing stepped it, null while unstepped. */
  original: ChoiceSettings.nullable(),
  canStepDown: z.object({
    effort: z.boolean(),
    model: z.boolean()
  }),
  canStepUp: z.boolean(),
  /** When the stepped-down choice is due back, null while unstepped. */
  restoreAt: EpochMilliseconds.nullable(),
  /** The fraction of its own load the next step of each kind saves. */
  effortSaving: z.number(),
  modelSaving: z.number()
});
export type ChoiceFacts = z.infer<typeof ChoiceFacts>;

const PaceFacts = z.object({
  phase: PacePhase,
  windowKind: PacedWindowKind,
  readyAt: EpochMilliseconds,
  continueAt: EpochMilliseconds,
  restUntil: EpochMilliseconds.nullable(),
  hasStoppedWork: z.boolean(),
  isHeld: z.boolean(),
  pausedAt: EpochMilliseconds,
  /** The burn rate measured when it paused, by usage window key. */
  rates: z.record(z.string(), z.number())
});
export type PaceFacts = z.infer<typeof PaceFacts>;

const SessionFacts = z.object({
  id: z.string().min(1),
  conversationId: z.string().nullable(),
  agentId: z.string(),
  command: z.string(),
  name: z.string(),
  isWorking: z.boolean(),
  backgroundAgentCount: z.number().int().nonnegative(),
  isPacingOn: z.boolean(),
  isHandingOff: z.boolean(),
  hasUsageLimit: z.boolean(),
  isSkipped: z.boolean(),
  isContinuedRun: z.boolean(),
  isShielded: z.boolean(),
  isModelSwitchApplying: z.boolean(),
  workingSince: EpochMilliseconds.nullable(),
  workingShare: z.number().min(0).max(1),
  ownLoad: z.number().nonnegative(),
  subagents: z.array(SubagentReport),
  choice: ChoiceFacts.nullable(),
  pace: PaceFacts.nullable()
});
export type SessionFacts = z.infer<typeof SessionFacts>;

export const PacingReport = z.object({
  windowLabel: z.string().min(1),
  at: EpochMilliseconds,
  sessions: z.array(SessionFacts),
  observed: z.array(ObservedDecision)
});
export type PacingReport = z.infer<typeof PacingReport>;

/** The engine's shadow comparison so far, for inspection. */
export const ShadowSummary = z.object({
  counts: z.partialRecord(
    ObservedKind, z.object({
      agree: z.number().int().nonnegative(),
      disagree: z.number().int().nonnegative()
    })
  ),
  lastDisagreements: z.array(
    z.object({
      at: EpochMilliseconds,
      sessionId: z.string(),
      observed: ObservedKind.nullable(),
      decided: ObservedKind.nullable()
    })
  )
});
export type ShadowSummary = z.infer<typeof ShadowSummary>;
