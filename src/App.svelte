<script lang="ts">
  import { publishToBoard } from "@/lib/dashboard/boardPublisher.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { onWindowCloseRequested } from "@/lib/ipc/platform";
  import {
    chatsForProject,
    loadChatRegistry,
    persistedChatsFromSessions,
    registryWithoutChat,
    registryWithProjectChats,
    saveChatRegistry
  } from "@/lib/session/chat-registry";
  import type { StoredChatRegistry } from "@/lib/session/chat-registry";
  import { restoreProjectChats, shouldStartFreshChat } from "@/lib/session/chat-restore";
  import { EmptyWorkspaceOutcome, emptyWorkspaceOutcome } from "@/lib/session/session-exit";
  import { clearSessionSnapshot, restoreLiveSnapshot } from "@/lib/session/session-restore";
  import { effective } from "@/lib/settings/prefs.svelte";
  import { sendSettingsMessage } from "@/lib/settings/settings-requests";
  import { settings } from "@/lib/settings/settings.svelte";
  import { createActiveRepository } from "@/lib/shell/activeRepository.svelte";
  import { clearDiscordPresence } from "@/lib/shell/discord-presence";
  import BootSplash from "@/lib/shell/screen/BootSplash.svelte";
  import { animatePaneIn } from "@/lib/shell/screen/pane-dom";
  import ProjectSidebar from "@/lib/shell/screen/ProjectSidebar.svelte";
  import WorkspaceScreen from "@/lib/shell/screen/WorkspaceScreen.svelte";
  import { registerSessionShortcuts } from "@/lib/shell/session-shortcuts";
  import { createSessionAutomation } from "@/lib/shell/sessionAutomation.svelte";
  import { Side } from "@/lib/shell/side-panels";
  import { WindowRoute, windowRouteFor } from "@/lib/shell/window-route";
  import { syncWindowAddress } from "@/lib/shell/windowAddress.svelte";
  import WindowEvents from "@/lib/shell/WindowEvents.svelte";
  import { startWindowServices } from "@/lib/shell/windowServices.svelte";
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
  import type { Agent, OpenTarget, RecordedConversation } from "@/lib/types";
  import OverflowTooltipLayer from "@/lib/ui/OverflowTooltipLayer.svelte";
  import { isTemporaryWorkspace, normalizePath, shortDisplayName } from "@/lib/util/paths";
  import { settledValue } from "@/lib/util/settled";
  import { createRelocator } from "@/lib/workspace/workspace-relocate";
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
  let agents = $state<Agent[]>([]);
  let currentProject = $state<string>("");
  // Every project's persisted chats (lib/session/chat-registry) - the left
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
  // Temp workspaces live under the config dir as .../workspaces/temp-<stamp>.
  const isTemp = $derived(isTemporaryWorkspace(currentProject));
  // The one label for the open project - a friendly name when assigned, else the
  // compact "parent/leaf" directory (legible without eating the topbar; full path
  // in the tooltip). Every surface (top-bar chip, slug source, window title, and
  // the switcher's open-window rows) reads this one `shortDisplayName` so none of
  // them recomputes the last-two-segments split independently and drifts.
  const projectLabel = $derived(
    shortDisplayName({
      path: currentProject,
      labels: settings.labels
    })
  );
  // A temp workspace that never earned a name: when its last session ends, the
  // window returns to the picker rather than respawning an agent. Its folder is
  // kept - only the unused-temp cleanup setting ever deletes it. One that was
  // auto-named keeps the normal last-session behavior instead.
  const isUnnamedTemp = $derived(isTemp && !settings.labels[currentProject]);

  // The repository the top bar points at and its branches (lib/shell/activeRepository).
  const repository = createActiveRepository({ project: () => currentProject });

  // This window's sessions and their pane layout (lib/shell/windowSessions).
  const windowSessions = createWindowSessions({
    project: () => currentProject,
    animateIn: id => animatePaneIn({
      elPanes,
      id
    }),
    onLaunched() {
      pendingPrompt = undefined;
      phase = Phase.Ready;
    },
    // The workspace's last session just ended (lib/session/session-exit decides what next).
    onEmptied({ agent, isFailedStart }) {
      const outcome = emptyWorkspaceOutcome({
        isUnnamedTemp,
        isFailedStart
      });
      if (outcome === EmptyWorkspaceOutcome.Picker) {
        returnToPicker();
        return;
      }

      if (outcome === EmptyWorkspaceOutcome.Respawn) {
        windowSessions.launch({ agent });
        return;
      }

      pendingPrompt = undefined;
      phase = Phase.Onboarding;
    }
  });

  // Keep the persisted chat registry in step with this window's live
  // sessions, labels included, so a relaunch can list and resume them. An
  // empty session list never overwrites: leaving a project passes through
  // empty, and its stored chats must survive that.
  $effect(() => {
    if (currentProject === "" || windowSessions.sessions.length === 0) {
      return;
    }

    const chats = persistedChatsFromSessions({
      project: currentProject,
      sessions: windowSessions.sessions,
      labels: Object.fromEntries(
        windowSessions.sessions.map(session => [session.id, sessionLabel(session.id)] as const)
      ),
      now: Date.now()
    });
    if (chats.length === 0) {
      return;
    }

    const next = registryWithProjectChats({
      registry: untrack(() => chatRegistry),
      project: currentProject,
      chats,
      activeChatId: windowSessions.activeId
    });
    chatRegistry = next;
    saveChatRegistry(next);
  });

  // Handoff, model switching, usage resume and pacing, API-error retry, MCP
  // restarts and tab attention (lib/shell/sessionAutomation).
  const automation = createSessionAutomation({
    windowSessions,
    project: () => currentProject,
    availableAgents: () => realAgents,
    projectLabel: () => projectLabel
  });

  syncWindowAddress({
    isBooted: () => phase !== Phase.Loading,
    project: () => currentProject,
    isUnnamedTemp: () => isUnnamedTemp,
    windowSessions
  });

  startWindowServices({
    project: () => currentProject,
    windowSessions,
    repository,
    isWindowFocused: () => isWindowFocused
  });

  // Keep the agents dashboard (lib/dashboard) up to date with this window's agents.
  publishToBoard({
    project: () => currentProject,
    projectLabel: () => projectLabel,
    windowSessions
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

  // Boot a spawned window from its `w=` address (lib/shell/window-route). Returns
  // true when it handled the launch, so the launch_context path is skipped.
  async function routeFromAddress() {
    const route = windowRouteFor(location.search);
    if (route.kind === WindowRoute.LaunchContext) {
      return false;
    }

    if (route.kind === WindowRoute.Temp) {
      await startInTempWorkspace();
    } else if (route.kind === WindowRoute.Picker) {
      phase = Phase.Project;
    } else {
      await openProject({ path: route.path });
    }

    return true;
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
      isNewProject: true
    });
  }

  // Re-attach the live sessions an accidental reload orphaned. The snapshot
  // (sessionStorage - survives a reload, dies with the window) says which panes
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

    currentProject = snapshot.project;
    sendMessage(Command.windowRegisterProject, { path: snapshot.project });
    windowSessions.restore(snapshot);
    phase = Phase.Ready;
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

  // Tab and pane keyboard shortcuts (lib/shell/session-shortcuts).
  registerSessionShortcuts({
    windowSessions,
    isWorkspaceShown: () => phase === Phase.Ready,
    fallbackAgent: () => bestAgent,
    // Through its tab while the workspace is on screen, so it animates out.
    closeSession(session) {
      if (workspaceScreen) {
        workspaceScreen.closeSession(session);
        return;
      }

      windowSessions.close(session);
    }
  });

  // Closing PADE (the title-bar X) is a deliberate leave too: intercept the
  // close, let every agent reach an idle prompt, kill them, and only then let
  // the window go (Tauri destroys it once the handler settles unprevented).
  // While a leave is already in flight the gate skips the wait, so a second
  // X-click closes immediately - graceful, never a trap; the backend's
  // exit-time kill_all reaps whatever that force-close leaves behind.
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
      initialPrompt
    });
  }

  // Reopen a conversation from the sidebar's `/resume` list: bring it
  // forward when a chat already runs it, else launch its agent on it,
  // titled the way the agent titled it.
  function resumeRecordedConversation(conversation: RecordedConversation) {
    const runningChat = windowSessions.sessions.find(session => session.conversationId === conversation.conversationId);
    if (runningChat !== undefined) {
      windowSessions.select(runningChat.id);
      return;
    }

    const agent = agents.find(candidate => candidate.id === conversation.agentId);
    if (agent === undefined) {
      return;
    }

    const id = windowSessions.launch({
      agent,
      conversationId: conversation.conversationId,
      args: conversation.resumeArguments.length > 0 ? conversation.resumeArguments : undefined
    });
    if (conversation.title !== null) {
      setSessionLabel({
        id,
        label: conversation.title
      });
    }
  }

  // What a return to the open project does with its target: bring the
  // picked chat to the front, or launch a fresh one beside the live chats.
  function focusReturnedProject(target: OpenTarget) {
    if (target.resume !== undefined) {
      resumeRecordedConversation(target.resume);
      return;
    }

    if (target.chatId !== undefined && windowSessions.isOpen(target.chatId)) {
      windowSessions.select(target.chatId);
      return;
    }

    if (target.newChat === true) {
      launchFreshChat({
        path: target.path,
        agentId: target.agent
      });
    }
  }

  // Picking the project this window already has open is a return, not a
  // reopen - the live sessions stay exactly as they were. True when this
  // handled the open.
  function returnToOpenProject(target: OpenTarget) {
    const isReturnToCurrent =
      normalizePath(target.path) === normalizePath(currentProject) && windowSessions.sessions.length > 0;
    if (!isReturnToCurrent) {
      return false;
    }

    focusReturnedProject(target);
    phase = Phase.Ready;
    return true;
  }

  async function openProject(target: OpenTarget) {
    // If another window already has this project open, focus it instead of
    // opening a second copy here - the picker window stays put.
    if (await sendMessage(Command.windowFocusProject, { path: target.path })) {
      return;
    }

    if (returnToOpenProject(target)) {
      return;
    }

    // Switching this window to another project: the old project's agents don't
    // come along. Kill them before entering, so no session keeps running - and
    // cwd-locking - a workspace this window has left. Their chats stay in the
    // registry, so coming back resumes them.
    await runExclusiveLeave(async () => {
      await closeWorkspaceGracefully();

      await sendMessage(Command.workspaceOpen, { path: target.path });
      await sendSettingsMessage(Command.settingsGet); // pick up the updated recent history
      await startAgentFlow({
        path: target.path,
        initialPrompt: target.initialPrompt,
        agentId: target.agent,
        isNewProject: target.created === true,
        chatId: target.chatId,
        wantNewChat: target.newChat === true,
        resume: target.resume
      });
      await repository.load();
    });
  }

  // Restore a project's persisted chats through this window's sessions.
  async function restoreChatsForProject({ path, chatId }: {
    path: string;
    chatId?: string;
  }) {
    return await restoreProjectChats({
      host: {
        registry: chatRegistry,
        agents,
        launch: windowSessions.launch,
        select: windowSessions.select,
        isOpen: windowSessions.isOpen,
        setLabel: setSessionLabel
      },
      project: path,
      chatId
    });
  }

  // Decide how to enter a project: bring its persisted chats back when it
  // has any, else honor a saved per-project/default agent and launch the
  // best installed agent outright - opening a project always lands in the
  // workspace, never on a blocking chooser. A first prompt (the home
  // composer) or an explicit new-chat request starts a fresh chat alongside
  // the restored ones. The agent picker still appears after a hand-closed
  // or exited last session, where "what next?" is a genuine question.
  // (Reused for every entry path.)
  async function startAgentFlow({ path, initialPrompt, agentId, isNewProject = false, chatId, wantNewChat = false, resume }: {
    path: string;
    initialPrompt?: string;
    agentId?: string;
    isNewProject?: boolean;
    chatId?: string;
    wantNewChat?: boolean;
    resume?: RecordedConversation;
  }) {
    currentProject = path;

    if (isNewProject) {
      armNewProjectDetection(path);
    }

    // Let other windows' pickers focus this one instead of reopening the project.
    sendMessage(Command.windowRegisterProject, { path });

    const hasRestoredChats = isNewProject
      ? false
      : await restoreChatsForProject({
        path,
        chatId
      });
    if (resume !== undefined) {
      resumeRecordedConversation(resume);
    }

    const wantsFreshChat = shouldStartFreshChat({
      initialPrompt,
      wantNewChat,
      hasRestoredChats: hasRestoredChats || resume !== undefined
    });
    if (wantsFreshChat) {
      launchFreshChat({
        path,
        agentId,
        initialPrompt
      });
    }
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

  function switchToPicker() {
    document.startViewTransition(async () => {
      phase = Phase.Project;
      await tick();
    });
  }

  // ── Left rail: switch, start and delete chats ─────────────────────────────
  // Picking a chat in the open project just brings it to the front; picking
  // one elsewhere opens its project and restores its chats with that one in
  // front. Deleting forgets the chat first, then closes it if it is live -
  // the registry sync would keep it otherwise. A new chat in the open
  // project launches beside the ones there; elsewhere it opens the project
  // with a fresh chat added to its restored ones.
  function selectChat({ project, chatId }: {
    project: string;
    chatId: string;
  }) {
    if (normalizePath(project) === normalizePath(currentProject) && windowSessions.isOpen(chatId)) {
      windowSessions.select(chatId);
      phase = Phase.Ready;
      return;
    }

    openProject({
      path: project,
      chatId
    });
  }

  async function deleteChat({ project, chatId }: {
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

    if (normalizePath(project) !== normalizePath(currentProject)) {
      return;
    }

    const liveSession = windowSessions.sessions.find(session => session.id === chatId);
    if (liveSession !== undefined) {
      await windowSessions.close(liveSession);
    }
  }

  function newChat(project: string) {
    openProject({
      path: project,
      newChat: true
    });
  }

  // Hand the project back and show the picker: this window no longer claims
  // it, so another window (or this one, later) can open it fresh - no picker
  // tries to focus us here. Every caller has already killed/reaped the
  // project's PTYs, so nothing keeps running in it.
  function returnToPicker() {
    currentProject = "";
    repository.clear();
    pendingPrompt = undefined;
    sendMessage(Command.windowRegisterProject, { path: "" });
    switchToPicker();
  }

  // ── Relocate (move / rename) with lock handling ─────────────────────────────
  // The kill → backend-op → resume flow lives in lib/workspace/workspace-relocate; this
  // shell only lends it the session list and takes back the results.
  const relocator = createRelocator({
    sessions: () => windowSessions.sessions,
    currentProject: () => currentProject,
    markClosing: ids => windowSessions.markClosing(ids),
    removeSessions: ids => windowSessions.removeClosed(ids),
    stopRunners: stopAllRunners,
    async setCurrentProject(path) {
      currentProject = path;
      await sendMessage(Command.windowRegisterProject, { path });
      // The URL follows the project in an effect; a crash rebuild must reopen
      // the folder where it now is.
      await tick();
      await sendMessage(Command.recoveryArm, { url: location.href });
    },
    relaunch: windowSessions.launch
  });
</script>

<WindowEvents onvisible={redetectAgents} bind:isFocused={isWindowFocused} />

<!-- Font tokens bound declaratively; they cascade to every descendant. -->
<div style:--font-ui={effective.uiFamily} style:--font-monospace={effective.monospaceFamily} class="app-root">
  {#if phase === Phase.Project}
    <div class="home-shell">
      <ProjectSidebar
        activeId={windowSessions.activeId}
        ondeletechat={async target => await deleteChat(target)}
        onnewchat={newChat}
        onopenproject={projectPath => openProject({ path: projectPath })}
        onselectchat={selectChat}
        project={currentProject}
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
      path={currentProject}
    />
  {:else if phase === Phase.Ready}
    <WorkspaceScreen
      bind:this={workspaceScreen}
      {agents}
      {automation}
      {bestAgent}
      {chatRegistry}
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
      onnewchat={newChat}
      onopen={projectPath => openProject({ path: projectPath })}
      onrenamefolder={relocator.renameFolder}
      onreroot={path => (currentProject = path)}
      onresumeconversation={({ project, conversation }) => openProject({
        path: project,
        resume: conversation
      })}
      onselectchat={selectChat}
      onswitch={async () => {
        // "Switch project" - a DELIBERATE leave to the picker. The project's
        // agents and task runners do not idle invisibly behind it: runners
        // stop immediately, then agents are killed once they reach an idle
        // prompt.
        await runExclusiveLeave(async () => {
          await closeWorkspaceGracefully();
          returnToPicker();
        });
      }}
      project={currentProject}
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
