<script lang="ts">
  import { publishToBoard } from "@/lib/dashboard/boardPublisher.svelte";
  import { createBoardRoster } from "@/lib/dashboard/boardRoster.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { onWindowCloseRequested } from "@/lib/ipc/platform";
  import {
    chatsForProject,
    isChatRegistryKey,
    loadChatRegistry,
    persistedChatsFromSessions,
    registryWithLiveChats,
    registryWithoutChat,
    saveChatRegistry
  } from "@/lib/session/chats/chat-registry";
  import type { StoredChatRegistry } from "@/lib/session/chats/chat-registry";
  import { enterProjectChats, joinProjectChats } from "@/lib/session/chats/project-entry";
  import type { ProjectEntryHost } from "@/lib/session/chats/project-entry";
  import { sessionsOfProject } from "@/lib/session/chats/session-projects";
  import { EmptyWorkspaceOutcome, emptyWorkspaceOutcome } from "@/lib/session/lifecycle/session-exit";
  import { clearSessionSnapshot, restoreLiveSnapshot } from "@/lib/session/lifecycle/session-restore";
  import { effective } from "@/lib/settings/prefs.svelte";
  import { sendSettingsMessage } from "@/lib/settings/settings-requests";
  import { settings } from "@/lib/settings/settings.svelte";
  import { createActiveRepository } from "@/lib/shell/activeRepository.svelte";
  import { HandoffKind } from "@/lib/shell/handoff/window-handoff";
  import { createWindowHandoffs } from "@/lib/shell/handoff/windowHandoffs.svelte";
  import BootSplash from "@/lib/shell/screen/BootSplash.svelte";
  import { animatePaneIn } from "@/lib/shell/screen/pane-dom";
  import WorkspaceScreen from "@/lib/shell/screen/WorkspaceScreen.svelte";
  import { createSessionAutomation } from "@/lib/shell/sessionAutomation.svelte";
  import { registerSessionShortcuts } from "@/lib/shell/shortcuts/session-shortcuts";
  import { Side } from "@/lib/shell/side-panels";
  import { runChatClick } from "@/lib/shell/sidebar/chat-click";
  import type { ChatClickHost, ChatPick, ConversationPick } from "@/lib/shell/sidebar/chat-click";
  import ProjectSidebar from "@/lib/shell/sidebar/ProjectSidebar.svelte";
  import { clearDiscordPresence } from "@/lib/shell/window/discord-presence";
  import { WindowRoute, windowRouteFor } from "@/lib/shell/window/window-route";
  import { syncWindowAddress } from "@/lib/shell/window/windowAddress.svelte";
  import WindowEvents from "@/lib/shell/window/WindowEvents.svelte";
  import { startWindowServices } from "@/lib/shell/window/windowServices.svelte";
  import { createWindowSessions } from "@/lib/shell/windowSessions.svelte";
  import { sessionLabel, setSessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
  import { stopAllRunners } from "@/lib/stores/tasks/runners.svelte";
  import { showToast } from "@/lib/stores/ui/toast.svelte";
  import {
    armNewProjectDetection,
    disposeNewProjectDetection,
    initNewProjectDetection
  } from "@/lib/stores/workspace/new-project-detection";
  import { StartMode, realAgents as toRealAgents } from "@/lib/types";
  import type { Agent, AgentSession, OpenTarget } from "@/lib/types";
  import OverflowTooltipLayer from "@/lib/ui/tooltip/OverflowTooltipLayer.svelte";
  import { settledValue } from "@/lib/util/async/settled";
  import { isTemporaryWorkspace, shortDisplayName } from "@/lib/util/paths";
  import { createRelocator } from "@/lib/workspace/relocate/workspace-relocate";
  import Onboarding from "@/panels/Onboarding.svelte";
  import ProjectPicker from "@/panels/ProjectPicker.svelte";
  import { onDestroy, onMount, tick, untrack } from "svelte";

  // Which top-level screen is showing. A closed set defined once, compared
  // against by member so no bare literal leaks into the flow logic.
  const Phase = {
    Loading: "loading",
    Project: "project",
    Onboarding: "onboarding",
    Ready: "ready"
  } as const;
  type Phase = (typeof Phase)[keyof typeof Phase];
  let phase = $state<Phase>(Phase.Loading);
  /** On the root while the picker morphs into the workspace (ProjectSidebar). */
  const ENTERING_WORKSPACE_CLASS = "entering-workspace";
  let agents = $state<Agent[]>([]);
  // Every project's persisted chats (lib/session/chats/chat-registry) - the left
  // rail's memory, loaded once and kept in sync with the live sessions below.
  let chatRegistry = $state<StoredChatRegistry>(loadChatRegistry());
  // Carried through the agent picker so a new-project prompt survives onboarding.
  let pendingPrompt = $state<string | undefined>();
  // The panes container, so a drop's pointer can be hit-tested against the panes.
  let elPanes = $state<HTMLElement>();
  let workspaceScreen = $state<WorkspaceScreen>();
  // The open side panel (lib/shell/side-panels); `null` means none is open.
  let side = $state<Side | null>(Side.Feed);
  // The side column's width, which SidePanelResizer moves live during a drag.
  let sidePanelWidth = $state(effective.sidePanelWidth);
  // Only the focused window publishes Discord presence (see windowServices).
  let isWindowFocused = $state(document.hasFocus());

  // Agents excluding the always-present shell fallback - this count decides
  // whether we auto-launch or onboard.
  const realAgents = $derived(toRealAgents(agents));
  // Detection order is the registry's priority order, so the first real agent
  // is the best installed one; the shell fallback carries otherwise.
  const bestAgent = $derived(realAgents[0] ?? agents[0]);

  // A temp workspace that never earned a name: when its last session ends, the
  // window returns to the picker rather than respawning an agent. Its folder is
  // kept - only the unused-temp cleanup setting ever deletes it. One that was
  // auto-named keeps the normal last-session behavior instead.
  function isUnnamedTemporary(path: string) {
    return isTemporaryWorkspace(path) && !settings.labels[path];
  }

  // This window's sessions - from every project it holds - and their pane
  // layout (lib/shell/windowSessions).
  const windowSessions = createWindowSessions({
    animateIn: id => animatePaneIn({
      elPanes,
      id
    }),
    onLaunched() {
      pendingPrompt = undefined;
      showWorkspace();
    },
    // Closed for good: its chat leaves the rail's memory (the agent's own
    // recording still lists the conversation to resume).
    onRetired: session => forgetChat({
      project: session.project,
      chatId: session.id
    }),
    // The window's last session just ended (lib/session/lifecycle/session-exit decides what next).
    onEmptied({ agent, project, isFailedStart }) {
      const outcome = emptyWorkspaceOutcome({
        isUnnamedTemp: isUnnamedTemporary(project),
        isFailedStart
      });
      if (outcome === EmptyWorkspaceOutcome.Picker) {
        returnToPicker();
        return;
      }

      if (outcome === EmptyWorkspaceOutcome.Respawn) {
        windowSessions.launch({
          agent,
          project
        });
        return;
      }

      pendingPrompt = undefined;
      phase = Phase.Onboarding;
    }
  });

  // The window's one project context: the active session's project (see
  // windowSessions). The top bar, Change Feed, repository, tasks, editor
  // launcher, address and window registration all read this, so selecting a
  // session of another project switches them together - without touching any
  // session.
  const activeProject = $derived(windowSessions.activeProject);
  // Temp workspaces live under the config dir as .../workspaces/temp-<stamp>.
  const isTemp = $derived(isTemporaryWorkspace(activeProject));
  // The one label for the project in front - a friendly name when assigned,
  // else the compact "parent/leaf" directory (legible without eating the topbar;
  // full path in the tooltip). Every surface (top-bar chip, slug source, window
  // title, and the switcher's open-window rows) reads this one
  // `shortDisplayName` so none of them recomputes the split and drifts.
  const projectLabel = $derived(
    shortDisplayName({
      path: activeProject,
      labels: settings.labels
    })
  );
  const isUnnamedTemp = $derived(isUnnamedTemporary(activeProject));

  // The repository the top bar points at and its branches (lib/shell/activeRepository).
  const repository = createActiveRepository({ project: () => activeProject });

  // Keep the persisted chat registry in step with this window's live
  // sessions, each under its own project, labels included, so a relaunch can
  // list and resume them. A chat closed for good leaves through `forgetChat`,
  // never by being absent here.
  $effect(() => {
    if (windowSessions.sessions.length === 0) {
      return;
    }

    const chats = persistedChatsFromSessions({
      sessions: windowSessions.sessions,
      labels: Object.fromEntries(
        windowSessions.sessions.map(session => [session.id, sessionLabel(session.id)] as const)
      ),
      now: Date.now()
    });
    const current = untrack(() => chatRegistry);
    const next = registryWithLiveChats({
      registry: current,
      chats,
      activeChatId: windowSessions.activeId,
      activeProject
    });
    if (next === current) {
      return;
    }

    chatRegistry = next;
    saveChatRegistry(next);
  });

  function forgetChat({ project, chatId }: {
    project: string;
    chatId: string;
  }) {
    const next = registryWithoutChat({
      registry: chatRegistry,
      project,
      chatId
    });
    chatRegistry = next;
    saveChatRegistry(next);
  }

  // Handoff, model switching, usage resume and pacing, API-error retry, MCP
  // restarts and tab attention (lib/shell/sessionAutomation).
  const automation = createSessionAutomation({
    windowSessions,
    project: () => activeProject,
    availableAgents: () => realAgents,
    projectLabel: () => projectLabel
  });

  syncWindowAddress({
    isBooted: () => phase !== Phase.Loading,
    project: () => activeProject,
    isUnnamedTemp: () => isUnnamedTemp,
    windowSessions
  });

  startWindowServices({
    project: () => activeProject,
    windowSessions,
    repository,
    isWindowFocused: () => isWindowFocused
  });

  // Keep the agents dashboard (lib/dashboard) up to date with this window's
  // agents - the same board that tells the other windows' rails what runs here.
  publishToBoard({
    project: () => activeProject,
    projectLabel: () => projectLabel,
    windowSessions
  });

  // The sessions the other PADE windows run, for the left rail.
  const roster = createBoardRoster();

  // Sessions sent to and taken from other windows (lib/shell/handoff/windowHandoffs).
  const handoffs = createWindowHandoffs({
    windowSessions,
    joinProject,
    async openWorkspace(path) {
      await sendMessage(Command.workspaceOpen, { path });
    },
    onEmptiedByMove: returnToPicker
  });

  // Agent detection runs a subprocess per agent, so cap how long the boot waits
  // on it - a stall must never freeze the splash. An empty list just routes to
  // onboarding, and the redetect interval fills it in shortly after.
  const BOOT_DETECT_TIMEOUT_MS = 6_000;
  // The slow fallback poll behind the visibility-driven re-detect below.
  const AGENT_REDETECT_INTERVAL_MS = 30_000;

  onMount(async () => {
    try {
      await boot();
    } catch {
      // Never strand the user on the splash - fall back to the project picker.
      showToast("Startup hit a snag - pick a project to continue");
      phase = Phase.Project;
    }
  });
  onDestroy(disposeNewProjectDetection);

  async function boot() {
    await initNewProjectDetection();
    // Detection is best-effort: a rejection or a stall both yield an empty
    // list rather than blocking the routing below.
    const detecting = Promise.race([
      settledValue({
        promise: sendMessage(Command.agentsDetect),
        fallback: []
      }),
      new Promise<Agent[]>(resolve => setTimeout(() => resolve([]), BOOT_DETECT_TIMEOUT_MS))
    ]);
    [agents] = await Promise.all([detecting, sendSettingsMessage(Command.settingsGet)]);

    // An accidental reload (F5, a crash recovery) re-attaches this window to
    // the agents still running in the backend - before any query routing,
    // whose `w=` went stale the moment the user moved on inside the window
    // (the incident: a `?w=empty` window living in a project rebooted to the
    // picker and orphaned its live PTYs, unreachable).
    if (await reattachAfterReload()) {
      return;
    }

    // A spawned window carries a `w=` query that overrides the normal cold-start.
    // The plain main window (no query) keeps today's launch_context behavior.
    if (await routeFromAddress()) {
      return;
    }

    await bootFromLaunchContext();
  }

  // Boot a spawned window from its `w=` address (lib/shell/window/window-route). Returns
  // true when it handled the launch, so the launch_context path is skipped.
  async function routeFromAddress() {
    const route = windowRouteFor(location.search);
    if (route.kind === WindowRoute.LaunchContext) {
      return false;
    }

    await bootRoute(route);
    return true;
  }

  type BootRoute = Exclude<ReturnType<typeof windowRouteFor>, { kind: typeof WindowRoute.LaunchContext }>;
  async function bootRoute(route: BootRoute) {
    if (route.kind === WindowRoute.Temp) {
      await startInTempWorkspace();
    } else if (route.kind === WindowRoute.Picker) {
      phase = Phase.Project;
    } else if (route.kind === WindowRoute.Handoff) {
      await bootFromHandoff(route.path);
    } else {
      await openProject({ path: route.path });
    }
  }

  // A window spawned to receive sessions another window handed it: take them
  // in, or - when nothing arrived after all - just open the project.
  async function bootFromHandoff(path: string) {
    windowSessions.enter(path);
    const hasTakenHandoff = await handoffs.takeHandoffs();
    if (!hasTakenHandoff) {
      await openProject({ path });
      return;
    }

    await showWorkspace();
    await repository.load();
  }

  async function bootFromLaunchContext() {
    const context = await sendMessage(Command.launchContext);
    if (context.hasProject) {
      await sendMessage(Command.workspaceOpen, { path: context.cwd }); // records it in recent history
      await startAgentFlow({ path: context.cwd });
      await repository.load();
      return;
    }

    // Default: show the homescreen (the project picker). Starting straight
    // in a temp workspace, resuming the last project on the way, is the
    // opt-in the picker's "On launch" section offers instead.
    const prefersPicker = effective.startMode === StartMode.enum.picker;
    if (prefersPicker) {
      phase = Phase.Project;
      return;
    }

    if (await restoreLastProject()) {
      return;
    }

    // Default: start immediately in a throwaway workspace so there's no
    // blocking picker. The user can switch any time (Switch button).
    await startInTempWorkspace();
  }

  // Continue where the last session left off: reopen the project the user
  // last worked in, with its chats restored, when it still has any and is
  // still on disk. False when there is nothing to continue, or it is gone.
  async function restoreLastProject() {
    const lastProject = chatRegistry.lastProject;
    const hasLastChats = lastProject !== "" && chatsForProject({
      registry: chatRegistry,
      project: lastProject
    }).length > 0;
    if (!hasLastChats) {
      return false;
    }

    try {
      await sendMessage(Command.workspaceOpen, { path: lastProject });
      await startAgentFlow({ path: lastProject });
      await repository.load();
      return true;
    } catch {
      return false;
    }
  }

  async function startInTempWorkspace() {
    await startAgentFlow({
      path: await sendMessage(Command.workspaceTemp),
      created: true
    });
  }

  // Re-attach the live sessions an accidental reload orphaned. The snapshot
  // (per window label - survives a reload and a crash rebuild) says which panes
  // this window was showing; `pty_list` says which of them the backend still
  // hosts, and only that intersection is restored - a deliberate leave killed
  // its PTYs, so nothing survives it and boot proceeds normally. Each restored
  // pane re-attaches through Terminal's existing path: spawn is a no-op for a
  // running session and `pty_history` replays the conversation.
  async function reattachAfterReload() {
    const snapshot = await restoreLiveSnapshot();
    if (!snapshot) {
      return false;
    }

    try {
      await sendMessage(Command.workspaceOpen, { path: snapshot.project });
    } catch {
      // The project vanished while the window was reloading (deleted, moved,
      // unmounted). Boot falls through to the picker - so kill the snapshot's
      // still-live sessions rather than leave them running unreachably, which
      // would recreate the very invisible-agent incident restore exists to fix.
      await Promise.allSettled(snapshot.sessions.map(orphan => sendMessage(Command.ptyKill, { id: orphan.id })));
      clearSessionSnapshot();
      return false;
    }

    windowSessions.enter(snapshot.project);
    windowSessions.restore(snapshot);
    await showWorkspace();
    await repository.load();
    return true;
  }

  // Re-detect installed agents so the picker reflects an agent the user just
  // installed or removed - when the app becomes visible again (they switched back
  // from installing one, see the `visibilitychange` below) and on a slow poll as a
  // fallback. Detection spawns a process per agent, so it must stay off the drag
  // path: page visibility never changes while dragging a window that stays on
  // screen, unlike window focus which a title-bar drag churns and lagged.
  // Best-effort and never rejecting - the poll and the visibility signal both
  // fire it without awaiting - so a transient failure keeps the agents already
  // on screen.
  async function redetectAgents() {
    agents = await settledValue({
      promise: sendMessage(Command.agentsDetect),
      fallback: agents
    });
  }
  onMount(() => {
    const interval = setInterval(() => redetectAgents(), AGENT_REDETECT_INTERVAL_MS);
    return () => clearInterval(interval);
  });

  // Tab and pane keyboard shortcuts (lib/shell/shortcuts/session-shortcuts).
  registerSessionShortcuts({
    windowSessions,
    isWorkspaceShown: () => phase === Phase.Ready,
    fallbackAgent: () => bestAgent,
    closeSession: closeThroughTab
  });

  // Through its tab while the workspace is on screen, so it animates out.
  async function closeThroughTab(session: AgentSession) {
    if (workspaceScreen) {
      workspaceScreen.closeSession(session);
      return;
    }

    await windowSessions.close(session);
  }

  // The rail's stop button: end a live chat the way its tab's close does. It
  // lives on as a recorded conversation it can be resumed from.
  async function stopChat({ chatId }: { chatId: string }) {
    const liveSession = windowSessions.sessions.find(session => session.id === chatId);
    if (liveSession !== undefined) {
      await closeThroughTab(liveSession);
    }
  }

  // Closing PADE (the title-bar X) is a deliberate leave too: intercept the
  // close, let every agent - of every project this window holds - reach an
  // idle prompt, kill them, and only then let the window go (Tauri destroys it
  // once the handler settles unprevented). While a leave is already in flight
  // the gate skips the wait, so a second X-click closes immediately -
  // graceful, never a trap; the backend ends whatever PTYs the window still
  // owns as it is destroyed (window.rs `on_destroyed`).
  let unlistenCloseRequested: (() => void) | undefined;
  async function interceptWindowClose() {
    unlistenCloseRequested = await onWindowCloseRequested(async () => {
      // This window is going away: if it's the one publishing presence (only the
      // focused window does), stop reporting so its last project doesn't linger.
      // A remaining PADE window re-publishes its own status on gaining focus.
      if (isWindowFocused) {
        await clearDiscordPresence();
      }

      await runExclusiveLeave(closeWorkspaceGracefully);
    });
  }
  onMount(() => {
    interceptWindowClose();
    return () => unlistenCloseRequested?.();
  });

  // One deliberate leave at a time. The graceful wait can hold the UI open for
  // a while it stays fully interactive, so a second switch/leave starting
  // inside that window would interleave two kill → open → launch sequences over
  // the same session list. Later attempts are dropped, not queued - the first
  // leave is already taking the user away.
  let isLeaveInFlight = false;
  async function runExclusiveLeave(leave: () => Promise<void>) {
    if (isLeaveInFlight) {
      return;
    }

    isLeaveInFlight = true;
    try {
      await leave();
    } finally {
      isLeaveInFlight = false;
    }
  }

  // Stop every runner and close every session on a deliberate workspace leave.
  // The runner store is scoped to this window, so clearing it also removes the
  // old workspace's dock before the new project paints.
  async function closeWorkspaceGracefully() {
    await stopAllRunners();
    await windowSessions.closeAllGracefully();
  }

  // Launch a fresh chat in `path` on its preferred agent (a create-form
  // pick, else the per-project/default one, else the best installed).
  function launchFreshChat({ path, agentId, initialPrompt }: {
    path: string;
    agentId?: string;
    initialPrompt?: string;
  }) {
    windowSessions.launch({
      agent: preferredAgentFor({
        path,
        agentId
      }) ?? bestAgent,
      project: path,
      initialPrompt
    });
  }

  // The session actions the project-entry decisions run through - getters,
  // so each decision reads this window's current agents and registry.
  const projectEntryHost: ProjectEntryHost = {
    get registry() {
      return chatRegistry;
    },
    get agents() {
      return agents;
    },
    launch: windowSessions.launch,
    select: windowSessions.select,
    isOpen: windowSessions.isOpen,
    setLabel: setSessionLabel,
    sessions: () => windowSessions.sessions,
    launchFreshChat
  };

  // Open a project in this window. Nothing already running here is ever
  // stopped: an empty window enters it (its persisted chats come back), a
  // window with sessions - of any project - starts or brings forward one
  // session of it beside them, and the window's project context follows.
  async function openProject(target: OpenTarget) {
    if (isLeaveInFlight || await isFocusedElsewhere(target.path)) {
      return;
    }

    await sendMessage(Command.workspaceOpen, { path: target.path });
    await sendSettingsMessage(Command.settingsGet); // pick up the updated recent history
    const isWindowEmpty = windowSessions.sessions.length === 0;
    if (isWindowEmpty) {
      await startAgentFlow(target);
    } else {
      await joinProject(target);
    }

    await repository.load();
  }

  // A project another window has in front is focused there instead of opened
  // twice - unless this window already runs it. True when it was focused.
  async function isFocusedElsewhere(project: string) {
    const isRunHere = sessionsOfProject({
      sessions: windowSessions.sessions,
      project
    }).length > 0;
    if (isRunHere) {
      return false;
    }

    return await sendMessage(Command.windowFocusProject, { path: project });
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
    await showWorkspace();
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

  // A create-form agent pick wins outright; otherwise honor the per-project
  // override, then the workspace default.
  function preferredAgentFor({ path, agentId }: {
    path: string;
    agentId?: string;
  }) {
    const preferredId = agentId ?? settings.projectAgents[path] ?? settings.defaultAgent;
    return agents.find(agent => agent.id === preferredId);
  }

  // The one way into the workspace screen. Leaving the picker morphs through a
  // view transition with the project rail held still (ProjectSidebar), instead of
  // the screen snapping; from anywhere else it simply shows.
  async function showWorkspace() {
    const isLeavingPicker = phase === Phase.Project;
    if (!isLeavingPicker) {
      phase = Phase.Ready;
      return;
    }

    document.documentElement.classList.add(ENTERING_WORKSPACE_CLASS);
    const transition = document.startViewTransition(async () => {
      phase = Phase.Ready;
      await tick();
    });
    await transition.finished;
    document.documentElement.classList.remove(ENTERING_WORKSPACE_CLASS);
  }

  function switchToPicker() {
    document.startViewTransition(async () => {
      phase = Phase.Project;
      await tick();
    });
  }

  // ── Left rail: switch, start and delete chats ─────────────────────────────
  // A chat click does what lib/shell/sidebar/chat-click decides: bring a live
  // session forward here or in the window that runs it, relaunch a persisted
  // chat or resume a recorded conversation beside the open sessions, or - on a
  // new-window click - move a live session (still running) or reopen the chat
  // in a new window. Deleting forgets the chat first, then closes it if it is
  // live here.
  const chatClickHost: ChatClickHost = {
    select(id) {
      windowSessions.select(id);
      showWorkspace();
    },
    focusElsewhere: roster.focus,
    moveToNewWindow: handoffs.moveToNewWindow,
    reopenInNewWindow: handoffs.reopenInNewWindow,
    openHere: openProject
  };

  async function selectChat({ project, chatId, opensNewWindow }: ChatPick) {
    await runChatClick({
      host: chatClickHost,
      liveSession: windowSessions.sessions.find(session => session.id === chatId),
      elsewhere: roster.sessions.find(session => session.id === chatId),
      isRecorded: false,
      opensNewWindow,
      reopen: {
        kind: HandoffKind.enum.chat,
        project,
        chatId
      },
      openHere: {
        path: project,
        chatId
      }
    });
  }

  // A recorded conversation picked in the left rail - the same decision, keyed
  // on the conversation a session runs.
  async function resumeConversation({ project, conversation, opensNewWindow }: ConversationPick) {
    function runsConversation(session: { conversationId?: string }) {
      return session.conversationId === conversation.conversationId;
    }

    await runChatClick({
      host: chatClickHost,
      liveSession: windowSessions.sessions.find(runsConversation),
      elsewhere: roster.sessions.find(runsConversation),
      isRecorded: true,
      opensNewWindow,
      reopen: {
        kind: HandoffKind.enum.conversation,
        project,
        conversation
      },
      openHere: {
        path: project,
        resume: conversation
      }
    });
  }

  async function deleteChat({ project, chatId }: {
    project: string;
    chatId: string;
  }) {
    forgetChat({
      project,
      chatId
    });

    const liveSession = windowSessions.sessions.find(session => session.id === chatId);
    if (liveSession !== undefined) {
      await windowSessions.close(liveSession);
    }
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
    pendingPrompt = undefined;
    switchToPicker();
  }

  // ── Relocate (move / rename) with lock handling ─────────────────────────────
  // The kill → backend-op → resume flow lives in lib/workspace/relocate/workspace-relocate; this
  // shell only lends it the session list and takes back the results.
  const relocator = createRelocator({
    sessions: () => windowSessions.sessions,
    currentProject: () => activeProject,
    markClosing: ids => windowSessions.markClosing(ids),
    removeSessions: ids => windowSessions.removeClosed(ids),
    stopRunners: stopAllRunners,
    // The project in front moved: point at its new home until its relaunched
    // sessions (remapped there) take the front again.
    async setCurrentProject(path) {
      windowSessions.enter(path);
      await tick();
    },
    relaunch: windowSessions.launch
  });
</script>

<WindowEvents onvisible={redetectAgents} bind:isFocused={isWindowFocused} />

<!-- Another window of this instance saved the chat registry: adopt its copy, so
     this rail lists the chats (and their projects) it made or forgot, and this
     window's next save builds on it rather than writing a stale copy back -->
<svelte:window
  onstorage={e => {
    if (isChatRegistryKey(e.key)) {
      chatRegistry = loadChatRegistry();
    }
  }}
/>

<!-- Font tokens bound declaratively; they cascade to every descendant. -->
<div style:--font-ui={effective.uiFamily} style:--font-monospace={effective.monospaceFamily} class="app-root">
  {#if phase === Phase.Project}
    <div class="home-shell">
      <ProjectSidebar
        activeId={windowSessions.activeId}
        elsewhereSessions={roster.sessions}
        ondeletechat={async target => await deleteChat(target)}
        ondeleteproject={async projectPath => {
          await relocator.removeDirectory(projectPath);
        }}
        onnewchat={newChat}
        onopenproject={newChat}
        onrenamefolder={relocator.renameFolder}
        onresumeconversation={resumeConversation}
        onselectchat={selectChat}
        onstopchat={stopChat}
        project={activeProject}
        registry={chatRegistry}
        sessions={windowSessions.sessions}
      />
      <ProjectPicker
        {agents}
        ondelete={relocator.remove}
        ondeleteproject={relocator.removeDirectory}
        onmove={relocator.move}
        onopen={openProject}
        onrename={relocator.rename}
        onrenamefolder={relocator.renameFolder}
      />
    </div>
  {:else if phase === Phase.Onboarding}
    <Onboarding
      {agents}
      onpick={agent => windowSessions.launch({
        agent,
        initialPrompt: pendingPrompt
      })}
      onswitchproject={switchToPicker}
      path={activeProject}
    />
  {:else if phase === Phase.Ready}
    <WorkspaceScreen
      bind:this={workspaceScreen}
      {agents}
      {automation}
      {bestAgent}
      {chatRegistry}
      elsewhereSessions={roster.sessions}
      {isTemp}
      ondelete={async projectPath => {
        // "Delete directory" - a destructive removal of a real project's
        // folder. The switcher owns the confirmation UI (so it stays open and
        // animates the row out); here we just release the folder (killing
        // any session holding it) and delete it, letting the refreshed
        // settings flow back. Rejects with its message so the switcher can
        // surface it in the still-open prompt.
        await relocator.removeDirectory(projectPath);
      }}
      ondeletechat={async target => await deleteChat(target)}
      onmerge={handoffs.mergeInto}
      onnewchat={newChat}
      onopen={newChat}
      onrenamefolder={relocator.renameFolder}
      onreroot={path => windowSessions.enter(path)}
      onresumeconversation={resumeConversation}
      onselectchat={selectChat}
      onstopchat={stopChat}
      onswitch={async () => {
        // "Switch project" - a DELIBERATE leave to the picker. Every project's
        // agents and task runners in this window stop rather than idle
        // invisibly behind it: runners stop immediately, then agents are
        // killed once they reach an idle prompt.
        await runExclusiveLeave(async () => {
          await closeWorkspaceGracefully();
          returnToPicker();
        });
      }}
      project={activeProject}
      {projectLabel}
      {realAgents}
      {repository}
      {windowSessions}
      bind:elPanes
      bind:side
      bind:sidePanelWidth
    />
  {:else}
    <BootSplash />
  {/if}

  <!-- One top-level bubble for every truncation tooltip - rendered here so it
       escapes any panel/popover clipping or scroll-width inflation. -->
  <OverflowTooltipLayer />
</div>

<style>
  .app-root {
    block-size: 100%;
  }

  /* The home screen: the same left rail the workspace shows, beside the
     picker - so projects and their chats stay visible and switchable before
     a project is open. */
  .home-shell {
    display: flex;
    flex-direction: row;
    block-size: 100%;
  }
</style>
