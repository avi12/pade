<script lang="ts">
  import { publishToBoard } from "@/lib/dashboard/boardPublisher.svelte";
  import { createBoardRoster } from "@/lib/dashboard/boardRoster.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { isChatRegistryKey } from "@/lib/session/chats/chat-registry";
  import { EmptyWorkspaceOutcome, emptyWorkspaceOutcome } from "@/lib/session/lifecycle/session-exit";
  import { effective } from "@/lib/settings/prefs.svelte";
  import { settings } from "@/lib/settings/settings.svelte";
  import { createActiveRepository } from "@/lib/shell/activeRepository.svelte";
  import { createAgentDetection } from "@/lib/shell/app/agentDetection.svelte";
  import { startAppBoot } from "@/lib/shell/app/appBoot.svelte";
  import { createChatActions } from "@/lib/shell/app/chatActions.svelte";
  import { createChatRegistry } from "@/lib/shell/app/chatRegistry.svelte";
  import { createProjectFlow } from "@/lib/shell/app/projectFlow.svelte";
  import { createScreenPhase, Phase } from "@/lib/shell/app/screenPhase.svelte";
  import { createWindowLeave } from "@/lib/shell/app/windowLeave.svelte";
  import { createWindowHandoffs } from "@/lib/shell/handoff/windowHandoffs.svelte";
  import BootSplash from "@/lib/shell/screen/BootSplash.svelte";
  import { animatePaneIn } from "@/lib/shell/screen/pane-dom";
  import WorkspaceScreen from "@/lib/shell/screen/WorkspaceScreen.svelte";
  import { createSessionAutomation } from "@/lib/shell/sessionAutomation.svelte";
  import { registerSessionShortcuts } from "@/lib/shell/shortcuts/session-shortcuts";
  import { Side } from "@/lib/shell/side-panels";
  import ProjectSidebar from "@/lib/shell/sidebar/ProjectSidebar.svelte";
  import { syncWindowAddress } from "@/lib/shell/window/windowAddress.svelte";
  import WindowEvents from "@/lib/shell/window/WindowEvents.svelte";
  import { startWindowServices } from "@/lib/shell/window/windowServices.svelte";
  import { createWindowSessions } from "@/lib/shell/windowSessions.svelte";
  import { stopAllRunners } from "@/lib/stores/tasks/runners.svelte";
  import OverflowTooltipLayer from "@/lib/ui/tooltip/OverflowTooltipLayer.svelte";
  import { isTemporaryWorkspace, shortDisplayName } from "@/lib/util/paths";
  import { createRelocator } from "@/lib/workspace/relocate/workspace-relocate";
  import Onboarding from "@/panels/Onboarding.svelte";
  import ProjectPicker from "@/panels/ProjectPicker.svelte";
  import { tick } from "svelte";

  const screen = createScreenPhase();
  const detection = createAgentDetection();
  const registry = createChatRegistry();
  // The panes container, so a drop's pointer can be hit-tested against the panes.
  let elPanes = $state<HTMLElement>();
  let workspaceScreen = $state<WorkspaceScreen>();
  // The open side panel (lib/shell/side-panels); `null` means none is open.
  let side = $state<Side | null>(Side.FEED);
  // The side column's width, which SidePanelResizer moves live during a drag.
  let sidePanelWidth = $state(effective.sidePanelWidth);
  // Only the focused window publishes Discord presence (see windowServices).
  let isWindowFocused = $state(document.hasFocus());

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
      screen.clearPendingPrompt();
      screen.showWorkspace();
    },
    // Closed for good: its chat leaves the rail's memory (the agent's own
    // recording still lists the conversation to resume).
    onRetired: session => registry.forgetChat({
      project: session.project,
      chatId: session.id
    }),
    // The window's last session just ended (lib/session/lifecycle/session-exit decides what next).
    onEmptied({ agent, project, isFailedStart }) {
      const outcome = emptyWorkspaceOutcome({
        isUnnamedTemp: isUnnamedTemporary(project),
        isFailedStart
      });
      if (outcome === EmptyWorkspaceOutcome.PICKER) {
        projectFlow.returnToPicker();
        return;
      }

      if (outcome === EmptyWorkspaceOutcome.RESPAWN) {
        windowSessions.launch({
          agent,
          project
        });
        return;
      }

      screen.clearPendingPrompt();
      screen.setPhase(Phase.ONBOARDING);
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

  registry.followSessions({
    windowSessions,
    activeProject: () => activeProject
  });

  // Handoff, model switching, usage resume and pacing, API-error retry, MCP
  // restarts and tab attention (lib/shell/sessionAutomation).
  const automation = createSessionAutomation({
    windowSessions,
    availableAgents: () => detection.realAgents,
    projectLabel: () => projectLabel
  });

  syncWindowAddress({
    isBooted: () => screen.phase !== Phase.LOADING,
    project: () => activeProject,
    isUnnamedTemp: () => isUnnamedTemp,
    windowSessions
  });

  startWindowServices({
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

  const leave = createWindowLeave({
    windowSessions,
    isWindowFocused: () => isWindowFocused
  });
  const projectFlow = createProjectFlow({
    windowSessions,
    repository,
    screen,
    registry,
    detection,
    leave
  });

  // Sessions sent to and taken from other windows (lib/shell/handoff/windowHandoffs).
  const handoffs = createWindowHandoffs({
    windowSessions,
    joinProject: projectFlow.joinProject,
    async openWorkspace(path) {
      await sendMessage(Command.WORKSPACE_OPEN, { path });
    },
    onEmptiedByMove: projectFlow.returnToPicker
  });

  startAppBoot({
    windowSessions,
    repository,
    screen,
    registry,
    detection,
    projectFlow,
    takeHandoffs: handoffs.takeHandoffs
  });

  // The left rail's chat actions: switch, start, resume, stop and delete.
  const chatActions = createChatActions({
    windowSessions,
    roster,
    handoffs,
    screen,
    registry,
    projectFlow,
    workspaceScreen: () => workspaceScreen
  });

  // Tab and pane keyboard shortcuts (lib/shell/shortcuts/session-shortcuts).
  registerSessionShortcuts({
    windowSessions,
    isWorkspaceShown: () => screen.phase === Phase.READY,
    fallbackAgent: () => detection.bestAgent,
    closeSession: chatActions.closeThroughTab
  });

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

<WindowEvents onvisible={detection.redetectAgents} bind:isFocused={isWindowFocused} />

<!-- Another window of this instance saved the chat registry: adopt its copy, so
     this rail lists the chats (and their projects) it made or forgot, and this
     window's next save builds on it rather than writing a stale copy back -->
<svelte:window
  onstorage={e => {
    if (isChatRegistryKey(e.key)) {
      registry.adoptStoredCopy();
    }
  }}
/>

<!-- Font tokens bound declaratively; they cascade to every descendant. -->
<div style:--font-ui={effective.uiFamily} style:--font-monospace={effective.monospaceFamily} class="app-root">
  {#if screen.phase === Phase.PROJECT}
    <div class="home-shell">
      <ProjectSidebar
        activeId={windowSessions.activeId}
        elsewhereSessions={roster.sessions}
        ondeletechat={async target => await chatActions.deleteChat(target)}
        ondeleteproject={async projectPath => {
          await relocator.removeDirectory(projectPath);
        }}
        onnewchat={projectFlow.newChat}
        onopenproject={projectFlow.newChat}
        onopenthrowaway={projectFlow.startInTempWorkspace}
        onrenamefolder={relocator.renameFolder}
        onresumeconversation={chatActions.resumeConversation}
        onselectchat={chatActions.selectChat}
        onstopchat={chatActions.stopChat}
        project={activeProject}
        registry={registry.registry}
        sessions={windowSessions.sessions}
      />
      <ProjectPicker
        agents={detection.agents}
        ondelete={relocator.remove}
        ondeleteproject={relocator.removeDirectory}
        onmove={relocator.move}
        onopen={projectFlow.openProject}
        onrename={relocator.rename}
        onrenamefolder={relocator.renameFolder}
      />
    </div>
  {:else if screen.phase === Phase.ONBOARDING}
    <Onboarding
      agents={detection.agents}
      onpick={agent => windowSessions.launch({
        agent,
        initialPrompt: screen.pendingPrompt
      })}
      onswitchproject={screen.switchToPicker}
      path={activeProject}
    />
  {:else if screen.phase === Phase.READY}
    <WorkspaceScreen
      bind:this={workspaceScreen}
      agents={detection.agents}
      {automation}
      bestAgent={detection.bestAgent}
      chatRegistry={registry.registry}
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
      ondeletechat={async target => await chatActions.deleteChat(target)}
      onmerge={handoffs.mergeInto}
      onnewchat={projectFlow.newChat}
      onopen={projectFlow.newChat}
      onopenthrowaway={projectFlow.startInTempWorkspace}
      onrenamefolder={relocator.renameFolder}
      onreroot={path => windowSessions.enter(path)}
      onresumeconversation={chatActions.resumeConversation}
      onselectchat={chatActions.selectChat}
      onshowpicker={screen.switchToPicker}
      onstopchat={chatActions.stopChat}
      onswitch={async () => {
        // "Switch project" - a DELIBERATE leave to the picker. Every project's
        // agents and task runners in this window stop rather than idle
        // invisibly behind it: runners stop immediately, then agents are
        // killed once they reach an idle prompt.
        await leave.runExclusiveLeave(async () => {
          await leave.closeWorkspaceGracefully();
          projectFlow.returnToPicker();
        });
      }}
      project={activeProject}
      {projectLabel}
      realAgents={detection.realAgents}
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

    /* Switching the Cyberpunk palette on tears the whole window in once: the
       animation starts the moment the root's attribute makes this match, so
       the switch itself is the trigger and no script is involved. A light /
       dark flip keeps the same animation and so never replays it. */
    @media (prefers-reduced-motion: no-preference) {
      :global(:root[data-palette="cyberpunk"]) & {
        animation: var(--cyber-tear-animation);
      }
    }
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
