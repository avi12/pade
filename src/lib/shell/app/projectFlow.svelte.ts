import { Command, sendMessage } from "@/lib/ipc/messaging";
import { enterProjectChats, joinProjectChats } from "@/lib/session/chats/project-entry";
import type { ProjectEntryHost } from "@/lib/session/chats/project-entry";
import { sessionsOfProject } from "@/lib/session/chats/session-projects";
import { sendSettingsMessage } from "@/lib/settings/settings-requests";
import { settings } from "@/lib/settings/settings.svelte";
import type { createActiveRepository } from "@/lib/shell/activeRepository.svelte";
import type { AgentDetection } from "@/lib/shell/app/agentDetection.svelte";
import type { ChatRegistry } from "@/lib/shell/app/chatRegistry.svelte";
import type { ScreenPhase } from "@/lib/shell/app/screenPhase.svelte";
import type { WindowLeave } from "@/lib/shell/app/windowLeave.svelte";
import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
import { setSessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
import { armNewProjectDetection } from "@/lib/stores/workspace/active-project/new-project-detection";
import type { OpenTarget } from "@/lib/types/window";

/** How a project is entered in this window, and the way back to the picker. */
export function createProjectFlow({
  windowSessions,
  repository,
  screen,
  registry,
  detection,
  leave
}: {
  windowSessions: WindowSessions;
  repository: ReturnType<typeof createActiveRepository>;
  screen: ScreenPhase;
  registry: ChatRegistry;
  detection: AgentDetection;
  leave: WindowLeave;
}) {
  // A create-form agent pick wins outright; otherwise honor the per-project
  // override, then the workspace default.
  function preferredAgentFor({ path, agentId }: { path: string; agentId?: string }) {
    const preferredId = agentId ?? settings.projectAgents[path] ?? settings.defaultAgent;
    return detection.agents.find(agent => agent.id === preferredId);
  }

  // Launch a fresh chat in `path` on its preferred agent (a create-form
  // pick, else the per-project/default one, else the best installed).
  function launchFreshChat({
    path,
    agentId,
    initialPrompt
  }: {
    path: string;
    agentId?: string;
    initialPrompt?: string;
  }) {
    windowSessions.launch({
      agent:
        preferredAgentFor({
          path,
          agentId
        }) ?? detection.bestAgent,
      project: path,
      initialPrompt
    });
  }

  // The session actions the project-entry decisions run through - getters,
  // so each decision reads this window's current agents and registry.
  const projectEntryHost: ProjectEntryHost = {
    get registry() {
      return registry.registry;
    },
    get agents() {
      return detection.agents;
    },
    launch: windowSessions.launch,
    select: windowSessions.select,
    isOpen: windowSessions.isOpen,
    setLabel: setSessionLabel,
    sessions: () => windowSessions.sessions,
    launchFreshChat
  };

  // A project another window has in front is focused there instead of opened
  // twice - unless this window already runs it. True when it was focused.
  async function isFocusedElsewhere(project: string) {
    const isRunHere =
      sessionsOfProject({
        sessions: windowSessions.sessions,
        project
      }).length > 0;
    if (isRunHere) {
      return false;
    }

    return await sendMessage(Command.WINDOW_FOCUS_PROJECT, { path: project });
  }

  // Enter a project beside the sessions already open (lib/session/chats/project-entry).
  async function joinProject(target: OpenTarget) {
    if (target.created === true) {
      armNewProjectDetection(target.path);
    }

    await joinProjectChats({
      host: projectEntryHost,
      target
    });
    await screen.showWorkspace();
  }

  // Enter a project in an empty window: bring its persisted chats back when
  // it has any, else honor a saved per-project/default agent and launch the
  // best installed agent outright - opening a project always lands in the
  // workspace, never on a blocking chooser. A first prompt (the home
  // composer) or an explicit new-chat request starts a fresh chat alongside
  // the restored ones. The agent picker still appears after a hand-closed
  // or exited last session, where "what next?" is a genuine question.
  async function startAgentFlow(target: OpenTarget) {
    windowSessions.enter(target.path);

    if (target.created === true) {
      armNewProjectDetection(target.path);
    }

    await enterProjectChats({
      host: projectEntryHost,
      target
    });
  }

  // Open a project in this window. Nothing already running here is ever
  // stopped: an empty window enters it (its persisted chats come back), a
  // window with sessions - of any project - starts or brings forward one
  // session of it beside them, and the window's project context follows.
  async function openProject(target: OpenTarget) {
    if (leave.isInFlight || (await isFocusedElsewhere(target.path))) {
      return;
    }

    await sendMessage(Command.WORKSPACE_OPEN, { path: target.path });
    await sendSettingsMessage(Command.SETTINGS_GET); // pick up the updated recent history
    const isWindowEmpty = windowSessions.sessions.length === 0;
    if (isWindowEmpty) {
      await startAgentFlow(target);
    } else {
      await joinProject(target);
    }

    await repository.load();
  }

  // A throwaway workspace, made fresh and opened in this window - beside
  // whatever already runs here (the rail's action), or as its first project
  // (a launch that starts in one).
  async function startInTempWorkspace() {
    await openProject({
      path: await sendMessage(Command.WORKSPACE_TEMP),
      created: true
    });
  }

  // A project row or its new-chat button: a new session in that project,
  // beside every session already open - the same for both.
  async function newChat(project: string) {
    await openProject({
      path: project,
      newChat: true
    });
  }

  // Show the picker with nothing in front: this window no longer claims a
  // project, so another window (or this one, later) can open it fresh. Every
  // caller has already ended or handed away the window's sessions, so nothing
  // keeps running here.
  function returnToPicker() {
    windowSessions.enter("");
    repository.clear();
    screen.clearPendingPrompt();
    screen.switchToPicker();
  }

  return {
    openProject,
    joinProject,
    startAgentFlow,
    startInTempWorkspace,
    newChat,
    returnToPicker
  };
}

export type ProjectFlow = ReturnType<typeof createProjectFlow>;
