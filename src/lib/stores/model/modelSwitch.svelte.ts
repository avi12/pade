// Applying a model / effort picked from a tab to its running session (SoC: the
// decision and its state live here; the shell supplies the actions it owns).
//
// Claude switches in place - even mid-turn - through the backend's typed
// commands (`agent_apply_model`). Every other agent can only take a new model
// at launch, so its session is relaunched on its own conversation with the
// choice as launch flags the moment it is quiet: idle, and no background work
// it is still waiting on. Either way the pick applies to this session alone.
//
// It must not collide with an auto-handoff: the handoff scan holds back while a
// switch is being applied (`isModelSwitchApplying`), and a pick made while a
// handoff is already under way is kept for the successor instead of being
// typed into a session that is about to end (`choiceForSuccessor`).

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { agentReportsBusy, whenAgentSettled } from "@/lib/stores/agent/agentActivity.svelte";
import { whenSessionIdle } from "@/lib/stores/session/sessions.svelte";
import { SwitchOutcome } from "@/lib/types";
import type { AgentSession, ModelChoice } from "@/lib/types";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

/** What a pending switch is waiting for, as the menu tells it. */
export const SwitchWait = {
  Applying: "applying",
  Quiet: "quiet",
  Successor: "successor"
} as const;
export type SwitchWait = (typeof SwitchWait)[keyof typeof SwitchWait];

interface PendingModelSwitch {
  choice: ModelChoice;
  waitingFor: SwitchWait;
}

const pendingBySession = new SvelteMap<string, PendingModelSwitch>();
const applyingSessions = new SvelteSet<string>();

export function pendingModelSwitch(id: string) {
  return pendingBySession.get(id) ?? null;
}

/** The handoff scan holds back meanwhile, so the two never type over each other. */
export function isModelSwitchApplying(id: string) {
  return applyingSessions.has(id);
}

/** The choice a successor should launch with: one still pending (never applied
 *  to the session being handed off) laid over the one it already ran. */
export function choiceForSuccessor(session: AgentSession) {
  const pending = pendingBySession.get(session.id)?.choice;
  pendingBySession.delete(session.id);

  if (pending === undefined) {
    return session.modelChoice;
  }

  return {
    ...session.modelChoice,
    ...pending
  };
}

/** What the shell does on the switcher's behalf. */
export interface ModelSwitchHost {
  isHandingOff: (id: string) => boolean;
  /** Remember the choice on the session, so every later launch of it (a
   *  restart, a reattach) keeps running it. */
  rememberChoice: (options: {
    id: string;
    choice: ModelChoice;
  }) => void;
  /** Relaunch the session on its own conversation (it launches with the
   *  remembered choice). Resolves once the relaunch has been started. */
  relaunch: (id: string) => Promise<void>;
  isOpen: (id: string) => boolean;
  /** Tell the user a switch failed. */
  report: (message: string) => void;
}

/** The first moment a relaunch can't cut anything off: idle, with no
 *  background work the agent still waits on. */
async function untilQuiet(id: string) {
  await whenSessionIdle({ id });
  while (agentReportsBusy(id)) {
    await whenAgentSettled(id);
    await whenSessionIdle({ id });
  }
}

export function createModelSwitcher(host: ModelSwitchHost) {
  function setPending({ id, choice, waitingFor }: {
    id: string;
    choice: ModelChoice;
    waitingFor: SwitchWait;
  }) {
    pendingBySession.set(id, {
      choice,
      waitingFor
    });
  }

  async function relaunchWhenQuiet(session: AgentSession) {
    await untilQuiet(session.id);

    const isStillRelaunchable = host.isOpen(session.id) && !host.isHandingOff(session.id);
    if (!isStillRelaunchable) {
      return;
    }

    await host.relaunch(session.id);
  }

  async function apply({ session, choice }: {
    session: AgentSession;
    choice: ModelChoice;
  }) {
    const conversationId = session.conversationId;
    if (conversationId === undefined) {
      return;
    }

    const outcome = await sendMessage(Command.agentApplyModel, {
      sessionId: session.id,
      command: session.agent.command,
      conversationId,
      choice
    });
    host.rememberChoice({
      id: session.id,
      choice
    });

    if (outcome === SwitchOutcome.enum.applied) {
      return;
    }

    setPending({
      id: session.id,
      choice,
      waitingFor: SwitchWait.Quiet
    });
    await relaunchWhenQuiet(session);
  }

  /** Switch `session` to a picked model and/or effort - merged over a switch
   *  still pending, so a model then an effort picked quickly land together. */
  async function request({ session, choice }: {
    session: AgentSession;
    choice: ModelChoice;
  }) {
    const merged: ModelChoice = {
      ...pendingBySession.get(session.id)?.choice,
      ...choice
    };
    if (host.isHandingOff(session.id)) {
      setPending({
        id: session.id,
        choice: merged,
        waitingFor: SwitchWait.Successor
      });
      return;
    }

    setPending({
      id: session.id,
      choice: merged,
      waitingFor: SwitchWait.Applying
    });
    applyingSessions.add(session.id);
    try {
      await apply({
        session,
        choice: merged
      });
    } catch (error) {
      host.report(`Couldn't switch ${session.agent.label}'s model - ${String(error)}`);
    } finally {
      pendingBySession.delete(session.id);
      applyingSessions.delete(session.id);
    }
  }

  return { request };
}
