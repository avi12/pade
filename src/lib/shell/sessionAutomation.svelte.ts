// Everything that acts on this window's sessions without being asked: the
// near-limit auto-handoff, the model switcher it and the pacing share, the
// usage-limit resume, the usage pacing, the API-error retry, the MCP-change
// restarts and the multiple-choice tab attention. Each machine lives in its
// own store; this wires them to the window's sessions and to one another, so
// the shell only reads the notes they show.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { mcpRestartTargets } from "@/lib/session/session-restart";
import { effective } from "@/lib/settings/prefs.svelte";
import { settings } from "@/lib/settings/settings.svelte";
import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
import { createApiErrorRetry } from "@/lib/stores/agent/apiErrorRetry.svelte";
import { createAutoHandoff } from "@/lib/stores/agent/handoff.svelte";
import { armMcpReloadRecovery, dropMcpReload, failedMcpReloads } from "@/lib/stores/agent/mcpReload.svelte";
import { choiceForSuccessor, createModelSwitcher } from "@/lib/stores/model/modelSwitch.svelte";
import { reconcileChoiceAttention } from "@/lib/stores/session/sessionAttention.svelte";
import { transferNaming } from "@/lib/stores/session/sessionNaming.svelte";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { createUsagePacing } from "@/lib/stores/usage/usagePacing.svelte";
import { createUsageResume } from "@/lib/stores/usage/usageResume.svelte";
import type { Agent, AgentSession } from "@/lib/types";
import { onDestroy, onMount } from "svelte";

export function createSessionAutomation({ windowSessions, project, availableAgents, projectLabel }: {
  windowSessions: WindowSessions;
  /** The window's open project. */
  project: () => string;
  /** The installed agents a handoff may cross over to. */
  availableAgents: () => Agent[];
  /** The open project's label - the handoff document's slug source. */
  projectLabel: () => string;
}) {
  // ── Auto-handoff ───────────────────────────────────────────────────────────
  // Near-limit sessions hand off to a fresh agent; the machinery lives in
  // lib/stores/agent/handoff. The scan runs inside this $effect so it re-fires as the
  // session list, prefs and context stores change.
  const autoHandoff = createAutoHandoff({
    sessions: () => windowSessions.sessions,
    availableAgents,
    isOptedOut: () => settings.prefs.autoHandoff === false,
    thresholdPercentage: () => effective.handoffPercentage,
    slugSource: projectLabel,
    projectDirectory: project,
    // Announced close: without the flag, this kill's own exit event reaches the
    // session-exit path as "the agent quit on its own" and respawns a
    // replacement - a bare extra tab that also steals the active pane from the
    // successor being seeded right behind it. Held until removeSession has
    // dropped the id, so a late exit event finds neither the flag nor the
    // session.
    async endSession(id) {
      windowSessions.markClosing([id]);
      await Promise.allSettled([sendMessage(Command.ptyKill, { id })]);
    },
    removeSession: id => windowSessions.removeHandedOff(id),
    launchSuccessor({ agent, cwd, initialPrompt, predecessorId }) {
      // The successor keeps the model picked for its predecessor - including
      // one picked while this handoff was under way - unless a usage
      // crossover hands the work to a different agent.
      const predecessor = windowSessions.sessions.find(session => session.id === predecessorId);
      const isSameAgent = predecessor?.agent.id === agent.id;
      const successorId = windowSessions.launch({
        agent,
        cwd,
        initialPrompt,
        modelChoice: predecessor !== undefined && isSameAgent ? choiceForSuccessor(predecessor) : undefined
      });
      // Carry the ✦ auto-naming toggle to the successor so a handed-off session
      // keeps it on; the successor's own agent command names it (it may differ on
      // a usage crossover).
      transferNaming({
        fromId: predecessorId,
        toId: successorId,
        agent: agent.command
      });
      return successorId;
    }
  });
  $effect(() => autoHandoff.check());

  // A model / effort picked from a tab: applied in place where the agent can
  // take it, else by relaunching the session on its own conversation once it
  // is quiet - the same in-place restart a spawn-time theme change uses.
  const modelSwitcher = createModelSwitcher({
    isHandingOff: id => autoHandoff.isHandingOff(id),
    rememberChoice({ id, choice }) {
      windowSessions.sessions = windowSessions.sessions.map(session => session.id === id ? {
        ...session,
        modelChoice: {
          ...session.modelChoice,
          ...choice
        }
      } : session);
    },
    async relaunch(id) {
      const target = windowSessions.sessions.find(session => session.id === id);
      if (target === undefined) {
        return;
      }

      windowSessions.applyResumeArgs({
        id,
        args: await sendMessage(Command.agentResumeArgs, {
          command: target.agent.command,
          cwd: target.cwd ?? project(),
          conversationId: target.conversationId
        })
      });
      showToast(`Switching ${target.agent.label}'s model - resuming its conversation`);
      await windowSessions.restart([target]);
    },
    isOpen: id => windowSessions.isOpen(id),
    report: message => showToast(message)
  });
  onDestroy(() => autoHandoff.dispose());

  async function recoverFailedMcpReload(session: AgentSession) {
    dropMcpReload(session.id);
    showToast(`${session.agent.label} couldn't reload MCP - preparing a handoff`);
    await autoHandoff.restartForConfiguration(session);
  }

  $effect(() => {
    for (const id of failedMcpReloads()) {
      const session = windowSessions.sessions.find(candidate => candidate.id === id);
      if (session) {
        recoverFailedMcpReload(session);
      } else {
        dropMcpReload(id);
      }
    }
  });

  // ── Usage-limit auto-resume ────────────────────────────────────────────────
  // A session stopped by an exhausted usage window resumes the moment the
  // window resets - "continue" into the same session while its context has
  // room, the handoff flow above when it doesn't (lib/stores/usage/usageResume).
  const automaticRecoveryHost = {
    sessions: () => windowSessions.sessions,
    isOptedOut: () => settings.prefs.autoResume === false,
    thresholdPercentage: () => effective.handoffPercentage,
    forceHandoff: session => autoHandoff.force(session)
  } satisfies Parameters<typeof createUsageResume>[0];
  const usageResume = createUsageResume(automaticRecoveryHost);
  $effect(() => usageResume.check());
  onDestroy(() => usageResume.dispose());

  // ── Usage pacing ───────────────────────────────────────────────────────────
  // A working agent that would spend its usage window long before the reset is
  // stepped down to a cheaper effort or model, slowed, or stopped - and picked
  // back up as the Config settings say (lib/stores/usage/usagePacing).
  const usagePacing = createUsagePacing({
    sessions: () => windowSessions.sessions,
    settings: () => effective.pacing,
    isHandingOff: id => autoHandoff.isHandingOff(id),
    switchModel: async pick => await modelSwitcher.request(pick),
    announce: message => showToast(message)
  });
  $effect(() => usagePacing.check());
  onDestroy(() => usagePacing.dispose());

  // ── API-error auto-retry ───────────────────────────────────────────────────
  // A session stopped by a transient API error (overloaded, a 5xx, a dropped
  // connection) is nudged with "continue" every 30s, handing off through the
  // flow above when its context is too full to recover (lib/stores/agent/apiErrorRetry).
  const apiErrorRetry = createApiErrorRetry(automaticRecoveryHost);
  $effect(() => apiErrorRetry.check());
  onDestroy(() => apiErrorRetry.dispose());

  // ── Multiple-choice attention ───────────────────────────────────────────────
  // A running agent that puts up a multiple-choice question flashes its tab red
  // (SessionTabs) until the user looks at it or answers. The reconcile runs in an
  // $effect so it re-fires as focus and per-session status change, clearing the
  // flag on the active tab and once a session goes back to working (answered).
  $effect(() => {
    for (const session of windowSessions.sessions) {
      reconcileChoiceAttention({
        id: session.id,
        isActive: session.id === windowSessions.activeId
      });
    }
  });

  // A project's MCP servers changed (a server added or removed - the backend
  // fires only on a real membership change, never a value-only edit). Each
  // affected agent first writes a handoff document, then a fresh instance of the
  // SAME agent launches from it. The fresh process reads the changed MCP config;
  // the handoff preserves work that an immediate conversation restart could
  // interrupt. Worktrees remain scoped to their own config directory.
  let unlistenMcp: (() => void) | undefined;
  async function subscribeToMcpChanges() {
    unlistenMcp = await onMessage(EventType.mcpChanged, async ({ data: change }) => {
      const targets = mcpRestartTargets({
        sessions: windowSessions.sessions,
        change,
        currentProject: project()
      });
      if (targets.length === 0) {
        return;
      }

      if (!change.membershipChanged) {
        armMcpReloadRecovery(targets.map(target => target.id));
        showToast("MCP server settings changed - reload them in the agent; PADE will recover if it fails");
        return;
      }

      showToast(`MCP servers changed - preparing ${targets.length === 1 ? "an agent handoff" : `${targets.length} agent handoffs`}`);
      await Promise.all(
        targets.map(async target => {
          await autoHandoff.restartForConfiguration(target);
        })
      );
    });
  }
  onMount(() => {
    subscribeToMcpChanges();
    return () => unlistenMcp?.();
  });

  return {
    /** Writes the handoff document a saved temp workspace continues from. */
    writeHandoffDoc: autoHandoff.writeHandoffDoc,
    /** Apply a model / effort picked from a tab. */
    requestModel: modelSwitcher.request,
    usagePacing,
    /** The status line of whichever automatic recovery is running, if any. */
    get note() {
      return autoHandoff.note || usageResume.note || apiErrorRetry.note;
    }
  };
}
