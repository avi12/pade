// Effort and model stepping for usage pacing (prefs.pacingAdapt): an agent over
// pace steps down one notch at a time - its effort first, then a cheaper model
// (lib/model/model-cost) - before pacing stops or slows it, and back up the way
// it came once it has room to spare. As the window is about to reset it goes
// back to the user's own choice. Each step is remembered per window label, so a
// reload keeps the way back. A choice the user makes in between is theirs:
// pacing touches a session again only while it still runs what pacing set.
// This module owns the steps and their memory; lib/stores/usage/usagePacing
// decides when to take them, and the app shell supplies the switch itself.

import { windowLabel } from "@/lib/ipc/platform";
import { cheaperChoice } from "@/lib/model/model-cost";
import { bareModel, modelLabel, modelName } from "@/lib/model/model-label";
import { sessionModel } from "@/lib/stores/agent/context.svelte";
import { ensureModelOptions, modelOptions } from "@/lib/stores/model/modelOptions.svelte";
import { ModelChoice } from "@/lib/types";
import type { AgentSession } from "@/lib/types";
import { readStoredJson, writeStoredJson } from "@/lib/util/stored-json";
import { z } from "zod";

/** A session pacing stepped down to a cheaper effort or model. */
const AdaptedChoice = z.object({
  /** What it ran before each step, oldest first - the first is the user's own
   *  choice, and stepping back up retraces the rest. */
  trail: z.array(ModelChoice).min(1),
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

// eslint-disable-next-line svelte/prefer-svelte-reactivity -- pacing bookkeeping, never rendered
const adapted = new Map<string, AdaptedChoice>();

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

/** Forget a session's steps when it ends. */
export function dropUsageAdapt(id: string) {
  if (adapted.delete(id)) {
    remember();
  }
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

  /** The record for a session, dropped when the user has chosen since. */
  function pacingRecord(id: string) {
    const choice = adapted.get(id);
    if (choice === undefined) {
      return undefined;
    }

    if (!isStillPacingsChoice({
      id,
      choice
    })) {
      setAdapted({
        id,
        choice: null
      });
      return undefined;
    }

    return choice;
  }

  /** Step a session one notch cheaper. Resolves whether it could - false at the
   *  bottom, or before its agent has reported what it runs. */
  async function stepDown({ session, restoreAt }: {
    session: AgentSession;
    restoreAt: number;
  }) {
    const running = runningChoice(session.id);
    if (running === null) {
      return false;
    }

    await ensureModelOptions(session.agent.command);
    const next = cheaperChoice({
      model: running.model,
      effort: running.effort ?? null,
      options: modelOptions(session.agent.command)
    });
    if (next === null) {
      return false;
    }

    const record = pacingRecord(session.id);
    setAdapted({
      id: session.id,
      choice: {
        trail: [...(record?.trail ?? []), running],
        current: next,
        restoreAt: record?.restoreAt ?? restoreAt
      }
    });
    await switchTo({
      session,
      choice: next,
      why: "to stretch its usage to the reset"
    });
    return true;
  }

  /** Step a stepped-down session one notch back up the way it came. */
  async function stepUp(session: AgentSession) {
    const record = pacingRecord(session.id);
    const previous = record?.trail.at(-1);
    if (record === undefined || previous === undefined) {
      return;
    }

    const rest = record.trail.slice(0, -1);
    setAdapted({
      id: session.id,
      choice: rest.length > 0 ? {
        ...record,
        trail: rest,
        current: previous
      } : null
    });
    await switchTo({
      session,
      choice: previous,
      why: "- its usage has room to spare again"
    });
  }

  /** Put sessions back on the user's own choice: those whose window is about to
   *  reset, or all of them once stepping is turned off. */
  async function restore({ sessions, now, isEveryone }: {
    sessions: readonly AgentSession[];
    now: number;
    isEveryone: boolean;
  }) {
    for (const session of sessions) {
      const record = adapted.get(session.id);
      const isDue = record !== undefined && (isEveryone || now >= record.restoreAt);
      if (!isDue) {
        continue;
      }

      const isUntouched = pacingRecord(session.id) !== undefined;
      setAdapted({
        id: session.id,
        choice: null
      });

      if (isUntouched) {
        await switchTo({
          session,
          choice: record.trail[0],
          why: "- back to its own choice for the new window"
        });
      }
    }
  }

  return {
    /** Whether pacing has stepped this session down. */
    isSteppedDown: (id: string) => adapted.has(id),
    stepDown,
    stepUp,
    restore
  };
}
