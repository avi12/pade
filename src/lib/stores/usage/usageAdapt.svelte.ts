// Effort and model steps for usage pacing: an agent on track to run out of usage
// is turned one notch cheaper at a time - its effort one level down (the `effort`
// step), or a cheaper model (the `model` step, lib/model/model-cost) - and back up
// the way it came once it has room to spare. As the window is about to reset it
// goes back to the user's own choice. Each step is remembered per window label,
// so a reload keeps the way back. A choice the user makes in between is theirs:
// pacing touches a session again only while it still runs what pacing set.
// While a session is stepped down its subagents are capped at what it runs, so
// one that names a dearer model of its own can't spend past the step (the
// agent reads the cap at every spawn - agents/running_model/subagent_cap).
// This module owns the steps and their memory; lib/stores/usage/usagePacing
// decides when to take them, and the app shell supplies the switch itself.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { windowLabel } from "@/lib/ipc/platform";
import { cheaperModelChoice, lowerEffortChoice, subagentCap } from "@/lib/model/model-cost";
import { bareModel, modelLabel, modelName } from "@/lib/model/model-label";
import { registerSessionState, SessionStateKey } from "@/lib/session/lifecycle/session-scope";
import { sessionModel } from "@/lib/stores/agent/context.svelte";
import { ensureModelOptions, modelOptions } from "@/lib/stores/model/modelOptions.svelte";
import { ModelChoice, PacingStep } from "@/lib/types";
import type { AgentSession } from "@/lib/types";
import { readStoredJson, writeStoredJson } from "@/lib/util/data/stored-json";
import { SvelteMap } from "svelte/reactivity";
import { z } from "zod";

/** One step pacing took: which kind, and what the session ran before it. */
const AdaptStep = z.object({
  step: PacingStep,
  previous: ModelChoice
});

/** A session pacing stepped down to a cheaper effort or model. */
const AdaptedChoice = z.object({
  /** Each step taken, oldest first - the first one's `previous` is the user's
   *  own choice, and stepping back up retraces the rest. */
  trail: z.array(AdaptStep).min(1),
  /** What pacing last set. */
  current: ModelChoice,
  /** As its window is about to reset it goes back to the user's choice. */
  restoreAt: z.number()
});
type AdaptedChoice = z.infer<typeof AdaptedChoice>;

const AdaptedSessions = z.record(z.string(), AdaptedChoice);

function adaptStorageKey() {
  return `pade.usage-adapt:${windowLabel()}`;
}

const adapted = new SvelteMap<string, AdaptedChoice>();

function remember() {
  writeStoredJson({
    key: adaptStorageKey(),
    value: Object.fromEntries(adapted)
  });
}

function setAdapted({ id, choice }: {
  id: string;
  choice: AdaptedChoice | null;
}) {
  if (choice === null) {
    adapted.delete(id);
  } else {
    adapted.set(id, choice);
  }

  remember();
}

registerSessionState(SessionStateKey.enum.usageAdapt, {
  forget(id) {
    if (adapted.delete(id)) {
      remember();
    }
  }
});

/** The steps pacing has taken on a session, oldest first (reactive). */
export function adaptedSteps(id: string) {
  return adapted.get(id)?.trail.map(taken => taken.step) ?? [];
}

/** What pacing has the session run now and what it ran before, as a person
 *  reads them - null when pacing hasn't stepped it (reactive). */
export function adaptedChoice(id: string) {
  const choice = adapted.get(id);
  if (choice === undefined) {
    return null;
  }

  return {
    current: describe(choice.current),
    original: describe(choice.trail[0].previous),
    restoreAt: choice.restoreAt
  };
}

/** What the session runs, as its agent last reported it. */
function runningChoice(id: string) {
  const running = sessionModel(id);
  if (running === null) {
    return null;
  }

  return {
    model: running.id,
    effort: running.effort ?? undefined
  } satisfies ModelChoice;
}

/** Whether the session still runs what pacing set - the same model, and the
 *  same effort when pacing set one. */
function isStillPacingsChoice({ id, choice }: {
  id: string;
  choice: AdaptedChoice;
}) {
  const running = runningChoice(id);
  const isSameModel = running !== null && bareModel(running.model) === bareModel(choice.current.model ?? "");
  const isSameEffort = choice.current.effort === undefined || running?.effort === choice.current.effort;
  return isSameModel && isSameEffort;
}

/** Cap the session's subagents at `model`, or lift the cap with null. A
 *  session that hasn't started its conversation has no subagents yet. */
async function capSubagents({ session, model }: {
  session: AgentSession;
  model: string | null;
}) {
  const conversationId = session.conversationId;
  if (conversationId === undefined) {
    return;
  }

  const command = session.agent.command;
  try {
    await sendMessage(Command.agentSetSubagentCap, {
      command,
      conversationId,
      cap: model === null ? null : subagentCap({
        model,
        options: modelOptions(command)
      })
    });
  } catch (error) {
    console.warn("pade: could not cap the session's subagents", error);
  }
}

function describe(choice: ModelChoice) {
  return modelLabel({
    name: modelName(choice.model ?? ""),
    effort: choice.effort ?? null
  });
}

/** What the app shell provides. */
export interface AdaptHost {
  /** Switch the session to `choice` - the same path a pick from its tab takes. */
  switchModel: (options: {
    session: AgentSession;
    choice: ModelChoice;
  }) => Promise<void>;
  /** Tell the user what pacing did. */
  announce: (message: string) => void;
}

export function createUsageAdapter(host: AdaptHost) {
  for (const [id, choice] of Object.entries(
    readStoredJson({
      key: adaptStorageKey(),
      schema: AdaptedSessions
    }) ?? {}
  )) {
    adapted.set(id, choice);
  }

  async function switchTo({ session, choice, why }: {
    session: AgentSession;
    choice: ModelChoice;
    why: string;
  }) {
    host.announce(`${session.agent.label} switched to ${describe(choice)} ${why}`);
    await host.switchModel({
      session,
      choice
    });
  }

  /** The record for a session, dropped - with its subagents' cap - when the
   *  user has chosen since. */
  function pacingRecord(session: AgentSession) {
    const choice = adapted.get(session.id);
    if (choice === undefined) {
      return undefined;
    }

    if (!isStillPacingsChoice({
      id: session.id,
      choice
    })) {
      setAdapted({
        id: session.id,
        choice: null
      });
      capSubagents({
        session,
        model: null
      });
      return undefined;
    }

    return choice;
  }

  /** The choice one `step` cheaper than what the session runs, or null. */
  function cheaperBy({ step, running, command }: {
    step: PacingStep;
    running: ModelChoice;
    command: string;
  }) {
    const options = modelOptions(command);
    const model = running.model ?? "";
    if (step === PacingStep.enum.effort) {
      return lowerEffortChoice({
        model,
        effort: running.effort ?? null,
        options
      });
    }

    if (step === PacingStep.enum.model) {
      return cheaperModelChoice({
        model,
        options
      });
    }

    return null;
  }

  /** Take one `step` (effort or model) cheaper. Resolves whether it could -
   *  false at the bottom, or before its agent has reported what it runs. */
  async function stepDown({ session, restoreAt, step }: {
    session: AgentSession;
    restoreAt: number;
    step: PacingStep;
  }) {
    const running = runningChoice(session.id);
    if (running === null) {
      return false;
    }

    await ensureModelOptions(session.agent.command);
    const next = cheaperBy({
      step,
      running,
      command: session.agent.command
    });
    if (next === null) {
      return false;
    }

    const record = pacingRecord(session);
    setAdapted({
      id: session.id,
      choice: {
        trail: [...(record?.trail ?? []), {
          step,
          previous: running
        }],
        current: next,
        restoreAt: record?.restoreAt ?? restoreAt
      }
    });
    await capSubagents({
      session,
      model: next.model ?? running.model ?? null
    });
    await switchTo({
      session,
      choice: next,
      why: "to stretch its usage to the reset"
    });
    return true;
  }

  /** Step a stepped-down session one notch back up the way it came. Resolves
   *  whether there was a step to undo. */
  async function stepUp(session: AgentSession) {
    const record = pacingRecord(session);
    const last = record?.trail.at(-1);
    if (record === undefined || last === undefined) {
      return false;
    }

    const rest = record.trail.slice(0, -1);
    const isStillSteppedDown = rest.length > 0;
    setAdapted({
      id: session.id,
      choice: isStillSteppedDown ? {
        ...record,
        trail: rest,
        current: last.previous
      } : null
    });
    await capSubagents({
      session,
      model: isStillSteppedDown ? last.previous.model ?? null : null
    });
    await switchTo({
      session,
      choice: last.previous,
      why: "- its usage has room to spare again"
    });
    return true;
  }

  /** Put one session back on the user's own choice. */
  async function restoreSession(session: AgentSession) {
    const record = adapted.get(session.id);
    if (record === undefined) {
      return;
    }

    const isUntouched = pacingRecord(session) !== undefined;
    setAdapted({
      id: session.id,
      choice: null
    });
    await capSubagents({
      session,
      model: null
    });

    if (isUntouched) {
      await switchTo({
        session,
        choice: record.trail[0].previous,
        why: "- back to its own choice"
      });
    }
  }

  /** Put sessions whose window is about to reset back on the user's own choice. */
  async function restore({ sessions, now }: {
    sessions: readonly AgentSession[];
    now: number;
  }) {
    for (const session of sessions) {
      const record = adapted.get(session.id);
      const isDue = record !== undefined && now >= record.restoreAt;
      if (isDue) {
        await restoreSession(session);
      }
    }
  }

  return {
    /** Whether pacing has stepped this session down. */
    isSteppedDown: (id: string) => adapted.has(id),
    stepDown,
    stepUp,
    restoreSession,
    restore
  };
}
