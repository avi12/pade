import { cheaperModelChoice, lowerEffortChoice } from "@/lib/model/model-cost";
import type { ModelChoice, ModelOption } from "@/lib/types/agent";
import { PacingStep } from "@/lib/types/pacing";
import { costWeight } from "@/lib/usage/pacing/engine/load-shedding";
import type { ChoiceFacts, PaceFacts, SessionFacts } from "@/lib/usage/pacing/pacing-report";

/** What a session runs, as its agent reported it. */
interface RunningChoice {
  id: string;
  effort: string | null;
}

/** The facts only the pacing loop owns, per session, for the report. */
export type OwnedFacts = Pick<
  SessionFacts,
  | "isWorking"
  | "isSkipped"
  | "isContinuedRun"
  | "isShielded"
  | "workingSince"
  | "workingShare"
  | "ownLoad"
  | "subagents"
> & {
  /** The remembered pause, null when the session is not paused. */
  pace: PaceFacts | null;
};

/** The fraction of a session's own load a step to `next` saves. */
function savingOf({ current, next }: {
  current: ModelChoice;
  next: ModelChoice | null;
}) {
  if (next === null) {
    return 0;
  }

  return 1 - costWeight(next) / costWeight(current);
}

/** What stepping a session down by effort and by model would do - each kind
 *  priced by its cost weight, the way the account round prices a step. */
export function choiceFactsOf({ running, original, restoreAt, isSteppedDown, options }: {
  running: RunningChoice | null;
  original: ChoiceFacts["original"];
  restoreAt: ChoiceFacts["restoreAt"];
  isSteppedDown: boolean;
  options: readonly ModelOption[];
}): ChoiceFacts | null {
  if (running === null) {
    return null;
  }

  const current = {
    model: running.id,
    effort: running.effort ?? undefined
  } satisfies ModelChoice;
  const lowerEffort = lowerEffortChoice({
    model: running.id,
    effort: running.effort,
    options
  });
  const cheaperModel = cheaperModelChoice({
    model: running.id,
    options
  });
  return {
    model: running.id,
    effort: running.effort,
    original,
    canStepDown: {
      [PacingStep.enum.effort]: lowerEffort !== null,
      [PacingStep.enum.model]: cheaperModel !== null
    },
    canStepUp: isSteppedDown,
    restoreAt,
    effortSaving: savingOf({
      current,
      next: lowerEffort
    }),
    modelSaving: savingOf({
      current,
      next: cheaperModel
    })
  };
}

/** One paused session's state as the wire carries it - the loop's own record
 *  holds more (burn rates, utilization) than the engine needs. */
export function paceFactsOf(state: PaceFacts | undefined): PaceFacts | null {
  if (state === undefined) {
    return null;
  }

  return {
    phase: state.phase,
    windowKind: state.windowKind,
    readyAt: state.readyAt,
    continueAt: state.continueAt,
    restUntil: state.restUntil,
    hasStoppedWork: state.hasStoppedWork,
    isHeld: state.isHeld,
    pausedAt: state.pausedAt,
    rates: state.rates
  };
}

/** Put one session's facts together from what the app reports about it and
 *  what the loop owns. */
export function sessionFactsOf({ reported, owned }: {
  reported: Omit<SessionFacts, keyof OwnedFacts>;
  owned: OwnedFacts;
}): SessionFacts {
  return {
    ...reported,
    ...owned
  };
}
