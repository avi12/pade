<script lang="ts">
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { onWindowCloseRequested } from "@/lib/ipc/platform";
  import RunnerDock from "@/lib/runners/RunnerDock.svelte";
  import { paneIdsAfterSplit, sessionsAfterReorder } from "@/lib/session/pane-layout";
  import { EmptyWorkspaceOutcome, emptyWorkspaceOutcome } from "@/lib/session/session-exit";
  import { clearSessionSnapshot, restoreLiveSnapshot } from "@/lib/session/session-restore";
  import SessionTabs from "@/lib/session/SessionTabs.svelte";
  import { effective, SIDE_PANEL_MAXIMUM_FRACTION, SIDE_PANEL_MINIMUM_WIDTH } from "@/lib/settings/prefs.svelte";
  import { sendSettingsMessage } from "@/lib/settings/settings-requests";
  import { settings } from "@/lib/settings/settings.svelte";
  import { createActiveRepository } from "@/lib/shell/activeRepository.svelte";
  import BootSplash from "@/lib/shell/BootSplash.svelte";
  import { clearDiscordPresence } from "@/lib/shell/discord-presence";
  import { animatePaneIn, paneDropAt } from "@/lib/shell/pane-dom";
  import RecoveryNotes from "@/lib/shell/RecoveryNotes.svelte";
  import { registerSessionShortcuts } from "@/lib/shell/session-shortcuts";
  import { createSessionAutomation } from "@/lib/shell/sessionAutomation.svelte";
  import ShellChrome from "@/lib/shell/ShellChrome.svelte";
  import ShellToast from "@/lib/shell/ShellToast.svelte";
  import { Side } from "@/lib/shell/side-panels";
  import TerminalPanes from "@/lib/shell/TerminalPanes.svelte";
  import { WindowRoute, windowRouteFor } from "@/lib/shell/window-route";
  import { syncWindowAddress } from "@/lib/shell/windowAddress.svelte";
  import WindowEvents from "@/lib/shell/WindowEvents.svelte";
  import { startWindowServices } from "@/lib/shell/windowServices.svelte";
  import { createWindowSessions } from "@/lib/shell/windowSessions.svelte";
  import { stopAllRunners } from "@/lib/stores/tasks/runners.svelte";
  import { showToast } from "@/lib/stores/ui/toast.svelte";
  import {
    armNewProjectDetection,
    disposeNewProjectDetection,
    initNewProjectDetection
  } from "@/lib/stores/workspace/new-project-detection";
  import { activeMemberIn } from "@/lib/stores/workspace/workspaceMembers.svelte";
  import SendSelectionButton from "@/lib/terminal/SendSelectionButton.svelte";
  import { StartMode, realAgents as toRealAgents } from "@/lib/types";
  import type { Agent, AgentSession, OpenTarget } from "@/lib/types";
  import type { DragHint } from "@/lib/ui/drag-reorder";
  import OverflowTooltipLayer from "@/lib/ui/OverflowTooltipLayer.svelte";
  import { isTemporaryWorkspace, normalizePath, shortDisplayName } from "@/lib/util/paths";
  import { settledValue } from "@/lib/util/settled";
  import { saveTempWorkspace } from "@/lib/workspace/save-temp-workspace";
  import { createRelocator } from "@/lib/workspace/workspace-relocate";
  import Onboarding from "@/panels/Onboarding.svelte";
  import ProjectPicker from "@/panels/ProjectPicker.svelte";
  import SidePanel from "@/panels/SidePanel.svelte";
  import SidePanelResizer from "@/panels/SidePanelResizer.svelte";
  import { onDestroy, onMount, tick } from "svelte";

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
  // Carried through the agent picker so a new-project prompt survives onboarding.
  let pendingPrompt = $state<string | undefined>();
  // The panes container, so a drop's pointer can be hit-tested against the panes.
  let elPanes = $state<HTMLElement>();
  let sessionTabsHandle = $state<{ closeSession: (session: AgentSession) => void } | null>(null);
  // Live drag state from the tab strip; drives the "drop here" overlay + halves.
  let dragHint = $state<DragHint | null>(null);
  // True while a split pane's header is dragged up over the tab strip - the mirror
  // gesture: the strip lights as a "drop → new tab" target and the drop pops the
  // pane out of the split (see TerminalPanes' `onpopout`). Reported by the
  // dragged pane's Terminal via the same engine hint the tab-split path uses.
  let isPaneDragOverTabs = $state(false);
  // The open side panel (lib/shell/side-panels); `null` means none is open.
  let side = $state<Side | null>(Side.Feed);
  // The side column's width, which SidePanelResizer moves live during a drag.
  let sidePanelWidth = $state(effective.sidePanelWidth);
  let isResizingSidePanel = $state(false);
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

  // Active agent id - used to show only its relevant config files.
  const activeAgent = $derived(windowSessions.activeSession?.agent.id ?? "");

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
      startAgentFlow({ path: context.cwd });
      await repository.load();
      return;
    }

    // Opt-in: show the project picker instead of starting in a temp workspace.
    const prefersPicker = settings.prefs.startMode === StartMode.enum.picker;
    if (prefersPicker) {
      phase = Phase.Project;
      return;
    }

    // Default: start immediately in a throwaway workspace so there's no
    // blocking picker. The user can switch any time (Switch button).
    await startInTempWorkspace();
  }

  async function startInTempWorkspace() {
    startAgentFlow({
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
      if (sessionTabsHandle) {
        sessionTabsHandle.closeSession(session);
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

  async function openProject(target: OpenTarget) {
    // If another window already has this project open, focus it instead of
    // opening a second copy here - the picker window stays put.
    if (await sendMessage(Command.windowFocusProject, { path: target.path })) {
      return;
    }

    // Picking the project this window already has open is a return, not a
    // reopen - the live sessions stay exactly as they were.
    const isReturnToCurrent =
      normalizePath(target.path) === normalizePath(currentProject) && windowSessions.sessions.length > 0;
    if (isReturnToCurrent) {
      phase = Phase.Ready;
      return;
    }

    // Switching this window to another project: the old project's agents don't
    // come along. Kill them before entering, so no session keeps running - and
    // cwd-locking - a workspace this window has left.
    await runExclusiveLeave(async () => {
      await closeWorkspaceGracefully();

      await sendMessage(Command.workspaceOpen, { path: target.path });
      await sendSettingsMessage(Command.settingsGet); // pick up the updated recent history
      startAgentFlow({
        path: target.path,
        initialPrompt: target.initialPrompt,
        agentId: target.agent,
        isNewProject: target.created === true
      });
      await repository.load();
    });
  }

  // Decide how to enter a project: honor a saved per-project/default agent,
  // else launch the best installed agent outright - opening a project always
  // lands in the workspace, never on a blocking chooser. The agent picker
  // still appears after a hand-closed or exited last session, where "what
  // next?" is a genuine question. (Reused for every entry path.)
  function startAgentFlow({ path, initialPrompt, agentId, isNewProject = false }: {
    path: string;
    initialPrompt?: string;
    agentId?: string;
    isNewProject?: boolean;
  }) {
    currentProject = path;

    if (isNewProject) {
      armNewProjectDetection(path);
    }

    // Let other windows' pickers focus this one instead of reopening the project.
    sendMessage(Command.windowRegisterProject, { path });
    windowSessions.launch({
      agent: preferredAgentFor({
        path,
        agentId
      }) ?? bestAgent,
      initialPrompt
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

  function switchToPicker() {
    document.startViewTransition(async () => {
      phase = Phase.Project;
      await tick();
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
    <ProjectPicker
      {agents}
      ondelete={relocator.remove}
      onmove={relocator.move}
      onopen={openProject}
      onrename={relocator.rename}
      onrenamefolder={relocator.renameFolder}
    />
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
    <div class="shell">
      <header class="topbar">
        <ShellChrome
          {activeAgent}
          activeSession={windowSessions.activeSession}
          {isTemp}
          label={projectLabel}
          ondelete={async projectPath => {
            // "Delete directory" - a destructive removal of a real project's
            // folder. The switcher owns the confirmation UI (so it stays open and
            // animates the row out); here we just release the folder (killing
            // any session holding it) and delete it, letting the refreshed
            // settings flow back. Rejects with its message so the switcher can
            // surface it in the still-open prompt.
            await relocator.removeDirectory(projectPath);
          }}
          onopen={projectPath => openProject({ path: projectPath })}
          onrenamefolder={relocator.renameFolder}
          onsavetemp={async ({ name, root }) => await saveTempWorkspace({
            name,
            root,
            host: {
              project: currentProject,
              sessionIds: windowSessions.sessions.map(session => session.id),
              continuing: windowSessions.activeSession ?? windowSessions.sessions[0],
              writeHandoffDoc: automation.writeHandoffDoc,
              reroot: path => (currentProject = path),
              launch: windowSessions.launch,
              closeSessions: windowSessions.closeSessions
            }
          })}
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
          onterminaleditor={editor =>
            windowSessions.launch({
              agent: {
                id: `editor-${editor.id}`,
                label: editor.label,
                command: editor.command,
                // A terminal editor writes logical order like any other program.
                reordersBidi: false
              },
              // Inherit the active session's worktree, if any, else the project dir.
              cwd: windowSessions.activeSession?.cwd,
              // Open the working directory in the editor.
              args: ["."],
              split: true
            })}
          project={currentProject}
          {repository}
          sessions={windowSessions.sessions}
          bind:side
        />

        <SessionTabs
          bind:this={sessionTabsHandle}
          activeId={windowSessions.activeId}
          {agents}
          branches={repository.branches}
          onclose={windowSessions.close}
          ondraghint={hint => (dragHint = hint)}
          onlaunch={agent => windowSessions.launch({ agent })}
          onlaunchbranch={async branch => {
            // Spawn an agent on its own git worktree for `branch`, isolated from
            // the other sessions. Uses the active session's agent (or the first).
            const agent = windowSessions.activeSession?.agent ?? bestAgent;
            const cwd = await sendMessage(Command.vcsWorktreeAdd, {
              cwd: activeMemberIn(currentProject),
              branch,
              create: false
            });
            windowSessions.launch({
              agent,
              cwd,
              branch
            });
          }}
          onmodelchoice={async pick => await automation.requestModel(pick)}
          onreorder={orderedIds => (windowSessions.sessions = sessionsAfterReorder({
            sessions: windowSessions.sessions,
            orderedIds
          }))}
          onselect={windowSessions.select}
          onsplit={drop => {
            // Show the dropped tab as a split pane on the side of the pane the
            // pointer landed on (repositioning it if already shown).
            const target = paneDropAt({
              elPanes,
              paneIds: windowSessions.paneIds,
              x: drop.pointerX,
              y: drop.pointerY
            });
            if (!target) {
              return;
            }

            windowSessions.paneIds = paneIdsAfterSplit({
              paneIds: windowSessions.paneIds,
              draggedId: drop.id,
              targetId: target.id,
              side: target.side
            });
            windowSessions.activeId = drop.id;
            animatePaneIn({
              elPanes,
              id: drop.id
            });
          }}
          paneIds={windowSessions.paneIds}
          popPaneActive={isPaneDragOverTabs}
          sessions={windowSessions.sessions}
        />
      </header>

      <main
        style:--side-panel-width="{sidePanelWidth}px"
        style:--side-panel-min-width="{SIDE_PANEL_MINIMUM_WIDTH}px"
        style:--side-panel-max-fraction="{SIDE_PANEL_MAXIMUM_FRACTION * 100}%"
        class="body"
        class:with-side={side !== null}
        data-resizing={isResizingSidePanel || undefined}
      >
        <TerminalPanes
          {agents}
          collapsingSplitPanes={windowSessions.collapsingSplitPanes}
          {dragHint}
          onexit={windowSessions.exited}
          onlaunch={agent => windowSessions.launch({
            agent,
            split: true
          })}
          onselect={windowSessions.select}
          sessions={windowSessions.sessions}
          bind:elPanes
          bind:paneIds={windowSessions.paneIds}
          bind:activeId={windowSessions.activeId}
          bind:isPaneDragOverTabs
        />

        {#if side !== null}
          <SidePanelResizer bind:width={sidePanelWidth} bind:isResizing={isResizingSidePanel} />
          <SidePanel agent={activeAgent} agents={realAgents} project={currentProject} {side} />
        {/if}
      </main>

      <RunnerDock activeSessionId={windowSessions.activeId} />

      <RecoveryNotes note={automation.note} pacing={automation.usagePacing} />
      <ShellToast />
      <SendSelectionButton activeId={windowSessions.activeId} />
    </div>
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

  .shell {
    display: flex;
    flex-direction: column;
    block-size: 100%;
  }

  .topbar {
    display: flex;
    flex-shrink: 0;
    flex-direction: column;
    gap: 8px;
    min-inline-size: 0;
    padding-block: 8px;
    padding-inline: clamp(10px, 1.6vw, 16px);
    border-block-end: 1px solid var(--outline);
    background: var(--surface-1);
  }

  .body {
    /* The side column's resolved width - one authoritative expression the grid
       track and the drag divider both read, so they can never clamp differently. */
    --side-panel-track:
      clamp(
        var(--side-panel-min-width),
        var(--side-panel-width),
        var(--side-panel-max-fraction)
      );

    position: relative;
    display: grid;
    flex: 1;

    /* Explicit full-height row track: without it the implicit row is `auto`
       (content-sized), so the terminal shrinks with the window but never grows
       back - the resize observer only fires on the shrink. */
    grid-template-rows: 1fr;
    grid-template-columns: 1fr;
    min-block-size: 0;
    transition: grid-template-columns 250ms var(--ease);

    /* The side column follows the dragged `--side-panel-width`, clamped so it
       never falls below its minimum nor swallows more than 60% of the window
       (design: `Math.max(280, Math.min(width*0.6, …))`). The width tweens with
       the emphasized easing so a double-click snap-back to the default animates. */
    &.with-side {
      grid-template-columns: 1fr var(--side-panel-track);
      transition: grid-template-columns 250ms var(--ease);

      @media (prefers-reduced-motion: reduce) {
        transition: none;
      }
    }

    /* A live drag drives the frame directly - any transition would lag it behind
       the pointer, so it is switched off for the duration of the drag. */
    &[data-resizing] {
      transition: none;
    }
  }

  @media (width <= 720px) {
    .body.with-side {
      grid-template-rows: 1fr 40%;
      grid-template-columns: 1fr;
    }
  }
</style>
