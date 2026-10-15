<script lang="ts">
  import { aiNamingEnabled } from "@/lib/agent/ai-naming-scope";
  import { createAutoNamer } from "@/lib/agent/auto-name";
  import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
  import { onWindowCloseRequested, setWindowTitle } from "@/lib/ipc/platform";
  import { startingModelChoice } from "@/lib/model/agent-model-scope";
  import RunnerDock from "@/lib/runners/RunnerDock.svelte";
  import {
    layoutWithoutSession,
    layoutWithoutSessions,
    paneIdsAfterSplit,
    sessionsAfterReorder,
    wrappedIndex
  } from "@/lib/session/pane-layout";
  import { registerPaneShortcuts } from "@/lib/session/pane-shortcuts";
  import { didFailToStart, EmptyWorkspaceOutcome, emptyWorkspaceOutcome } from "@/lib/session/session-exit";
  import { mcpRestartTargets, rekeyLayout } from "@/lib/session/session-restart";
  import { clearSessionSnapshot, restoreLiveSnapshot, saveSessionSnapshot } from "@/lib/session/session-restore";
  import SessionTabs, { ADD_SESSION_MENU_ID } from "@/lib/session/SessionTabs.svelte";
  import { registerTabShortcuts } from "@/lib/session/tab-shortcuts";
  import { effective, SIDE_PANEL_MAXIMUM_FRACTION, SIDE_PANEL_MINIMUM_WIDTH } from "@/lib/settings/prefs.svelte";
  import { sendSettingsMessage } from "@/lib/settings/settings-requests";
  import { settings } from "@/lib/settings/settings.svelte";
  import { createActiveRepository } from "@/lib/shell/activeRepository.svelte";
  import BootSplash from "@/lib/shell/BootSplash.svelte";
  import { clearDiscordPresence, updateDiscordPresence } from "@/lib/shell/discord-presence";
  import { animatePaneIn, paneDropAt } from "@/lib/shell/pane-dom";
  import RecoveryNotes from "@/lib/shell/RecoveryNotes.svelte";
  import ShellChrome from "@/lib/shell/ShellChrome.svelte";
  import ShellToast from "@/lib/shell/ShellToast.svelte";
  import { Side } from "@/lib/shell/side-panels";
  import TerminalPanes from "@/lib/shell/TerminalPanes.svelte";
  import { WindowShortcut, windowShortcutFor } from "@/lib/shell/window-shortcuts";
  import { dropAgentActivity } from "@/lib/stores/agent/agentActivity.svelte";
  import { createAgentRetheme } from "@/lib/stores/agent/agentRetheme.svelte";
  import { createApiErrorRetry, dropApiError } from "@/lib/stores/agent/apiErrorRetry.svelte";
  import { createAutoHandoff } from "@/lib/stores/agent/handoff.svelte";
  import { armMcpReloadRecovery, dropMcpReload, failedMcpReloads } from "@/lib/stores/agent/mcpReload.svelte";
  import { choiceForSuccessor, createModelSwitcher } from "@/lib/stores/model/modelSwitch.svelte";
  import {
    dropChoiceAttention,
    ensureChoiceAttention,
    reconcileChoiceAttention
  } from "@/lib/stores/session/sessionAttention.svelte";
  import { dropSessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
  import { applyNamingToAll, dropNaming, setNaming, transferNaming } from "@/lib/stores/session/sessionNaming.svelte";
  import { dropSessionStatus, isSessionIdle, whenSessionIdle } from "@/lib/stores/session/sessions.svelte";
  import { ensureRunnerListeners, stopAllRunners } from "@/lib/stores/tasks/runners.svelte";
  import { initTaskRunDetection, refreshTaskRunDetection } from "@/lib/stores/tasks/taskRuns.svelte";
  import { showToast } from "@/lib/stores/ui/toast.svelte";
  import { dropUsageAdapt } from "@/lib/stores/usage/usageAdapt.svelte";
  import { createUsagePacing, dropUsagePacing } from "@/lib/stores/usage/usagePacing.svelte";
  import { createUsageResume, dropUsageLimit } from "@/lib/stores/usage/usageResume.svelte";
  import {
    armNewProjectDetection,
    disposeNewProjectDetection,
    initNewProjectDetection
  } from "@/lib/stores/workspace/new-project-detection";
  import { projectKind, requestProjectKind } from "@/lib/stores/workspace/projectKinds.svelte";
  import {
    activeMemberIn,
    loadWorkspaceMembers,
    refreshMemberBranches
  } from "@/lib/stores/workspace/workspaceMembers.svelte";
  import { registerSendShortcut, unregisterSendShortcut } from "@/lib/terminal/send-shortcut";
  import SendSelectionButton from "@/lib/terminal/SendSelectionButton.svelte";
  import { StartMode, realAgents as toRealAgents, WindowMode } from "@/lib/types";
  import type { Agent, AgentSession, ModelChoice, OpenTarget } from "@/lib/types";
  import type { DragHint } from "@/lib/ui/drag-reorder";
  import OverflowTooltipLayer from "@/lib/ui/OverflowTooltipLayer.svelte";
  import { displayName, isTemporaryWorkspace, normalizePath, shortDisplayName } from "@/lib/util/paths";
  import { settledValue } from "@/lib/util/settled";
  import { FolderPath, parseInput } from "@/lib/validate";
  import { saveTempWorkspace } from "@/lib/workspace/save-temp-workspace";
  import { createRelocator } from "@/lib/workspace/workspace-relocate";
  import Onboarding from "@/panels/Onboarding.svelte";
  import ProjectPicker from "@/panels/ProjectPicker.svelte";
  import SidePanel from "@/panels/SidePanel.svelte";
  import SidePanelResizer from "@/panels/SidePanelResizer.svelte";
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";

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
  let sessions = $state<AgentSession[]>([]);
  let sessionTabsHandle = $state<{ closeSession: (session: AgentSession) => void } | null>(null);
  let activeId = $state<string | null>(null);
  // Which sessions are shown side by side in the terminal area. One id = the
  // classic single pane; more = a split view. Always a subset of `sessions`.
  let paneIds = $state<string[]>([]);
  let currentProject = $state<string>("");
  // The repository the top bar points at and its branches (lib/shell/activeRepository).
  const repository = createActiveRepository({ project: () => currentProject });
  // Carried through the agent picker so a new-project prompt survives onboarding.
  let pendingPrompt = $state<string | undefined>();

  // Agents excluding the always-present shell fallback - this count decides
  // whether we auto-launch or onboard.
  const realAgents = $derived(toRealAgents(agents));
  // Detection order is the registry's priority order, so the first real agent
  // is the best installed one; the shell fallback carries otherwise.
  const bestAgent = $derived(realAgents[0] ?? agents[0]);
  // Temp workspaces live under the config dir as .../workspaces/temp-<stamp>.
  const isTemp = $derived(isTemporaryWorkspace(currentProject));
  // Friendly auto-derived name for the current workspace, if one was assigned.
  const currentLabel = $derived(settings.labels[currentProject]);
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
  const isUnnamedTemp = $derived(isTemp && !currentLabel);
  const activeSession = $derived(sessions.find(session => session.id === activeId));
  // Active agent id - used to show only its relevant config files.
  const activeAgent = $derived(activeSession?.agent.id ?? "");
  // ── Tab drag → reorder / split ──────────────────────────────────────────────
  // The drop's layout math lives in `@/lib/session/pane-layout` and its DOM side
  // in `@/lib/shell/pane-dom`; TerminalPanes renders the panes and the preview.

  // The panes container, so a drop's pointer can be hit-tested against the panes.
  let elPanes = $state<HTMLElement>();
  // Live drag state from the tab strip; drives the "drop here" overlay + halves.
  let dragHint = $state<DragHint | null>(null);

  // True while a split pane's header is dragged up over the tab strip - the mirror
  // gesture: the strip lights as a "drop → new tab" target and the drop pops the
  // pane out of the split (see TerminalPanes' `onpopout`). Reported by the
  // dragged pane's Terminal via the same engine hint the tab-split path uses.
  let isPaneDragOverTabs = $state(false);

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
    if (await routeFromQuery(new URLSearchParams(location.search))) {
      return;
    }

    await bootFromLaunchContext();
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

  // Boot a spawned window from its `w=` query. Returns true when it handled the
  // launch (so the default launch_context path is skipped), false otherwise.
  async function routeFromQuery(query: URLSearchParams) {
    const mode = query.get("w");
    if (mode === WindowMode.enum.temp) {
      await startInTempWorkspace();
      return true;
    }

    if (mode === WindowMode.enum.empty) {
      phase = Phase.Project;
      return true;
    }

    if (mode !== WindowMode.enum.open) {
      return false;
    }

    // query.get("path") is a trust boundary - validate before opening.
    const path = parseInput({
      schema: FolderPath,
      raw: query.get("path")
    });
    if (!path) {
      return false;
    }

    await openProject({ path });
    return true;
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
    sessions = snapshot.sessions;
    paneIds = snapshot.paneIds;
    activeId = snapshot.activeId;
    // Stamped as launched-now so a session that dies right after the restore
    // reads as a failed start (no respawn loop), like any fresh launch.
    for (const restored of snapshot.sessions) {
      sessionLaunchedAt.set(restored.id, Date.now());
    }

    phase = Phase.Ready;
    await repository.load();
    return true;
  }

  // Persist this window's pane mapping so a reload can re-attach it (above).
  // Never while still booting - the effect's first run lands before the restore
  // has read the snapshot, and would wipe it. Once live, an empty project or
  // session list clears the snapshot instead: nothing to re-attach.
  $effect(() => {
    if (phase === Phase.Loading) {
      return;
    }

    saveSessionSnapshot({
      project: currentProject,
      sessions,
      paneIds,
      activeId
    });
  });

  // The address is state too: `w=` is what a reload re-routes off, and it went
  // stale the moment the user moved on from what the window was spawned for -
  // a `?w=empty` picker window living inside a project rebooted to the picker
  // (the incident), a `?w=temp` one minted a second throwaway. Keep the query
  // telling the truth - the project on screen, else the picker - so even a
  // snapshot-less reload (all sessions had exited) lands on the right screen.
  $effect(() => {
    if (phase === Phase.Loading) {
      return;
    }

    const query = currentProject === ""
      ? `?w=${WindowMode.enum.empty}`
      : `?w=${WindowMode.enum.open}&path=${encodeURIComponent(currentProject)}`;
    if (location.search !== query) {
      history.replaceState(null, "", query);
    }
  });

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

  // Subscribe once to the backend task-runner stream so the dock updates live.
  onMount(async () => {
    await ensureRunnerListeners();
  });

  // Arm WebView2 crash auto-recovery with this boot's real URL. The frontend
  // is the one party that always knows it - a backend capture at window
  // creation raced navigation and once rebuilt a crashed window onto
  // about:blank - and a recovery-rebuilt window re-arms itself right here.
  onMount(async () => {
    await sendMessage(Command.recoveryArm, { url: location.href });
  });

  // Watch the PTY stream once for the agent's multiple-choice prompts, so a tab
  // can flash red when one is pending on it (lib/stores/session/sessionAttention).
  onMount(async () => {
    await ensureChoiceAttention();
  });

  // Reflect known tasks the agent runs as "running" in the Tasks panel.
  onMount(async () => {
    await initTaskRunDetection(() => currentProject);
  });

  // Adopt settings another window changed - it deleted a workspace, renamed one,
  // toggled a pin - so this window's switcher and picker never list a project
  // that is already gone. Best-effort because an event handler can't await: a
  // failed re-read leaves the settings already adopted in place.
  let unlistenSettings: (() => void) | undefined;
  async function subscribeToSettingsChanges() {
    unlistenSettings = await onMessage(EventType.settingsChanged, async () => {
      await Promise.allSettled([sendSettingsMessage(Command.settingsGet)]);
    });
  }
  onMount(() => {
    subscribeToSettingsChanges();
    return () => unlistenSettings?.();
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
        sessions,
        change,
        currentProject
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

  // Name the OS window (title bar + taskbar) `PADE - <workspace>`, where the
  // workspace is the open project's own folder name - the part after the slash,
  // honoring a friendly label - reusing the same `displayName` leaf the switcher
  // rows show. The empty picker and a throwaway temp that never earned a name stay
  // the bare product name. Driven off the project store, so it re-titles on every
  // open, switch, and close.
  const hasNamedProject = $derived(currentProject !== "" && !isUnnamedTemp);
  $effect(() => {
    const workspace = displayName({
      path: currentProject,
      labels: settings.labels
    });
    setWindowTitle(hasNamedProject ? `PADE - ${workspace}` : "PADE");
  });

  // Watch the open project's files app-wide - not only while the Change Feed is
  // open - so the Tasks panel auto-updates on a manifest/script edit, and the
  // task-run and auto-name subscribers see changes from the moment a project
  // opens. Keyed on the project so it re-roots on an in-window switch; the
  // backend is idempotent, so a repeat call for the same root is a no-op.
  $effect(() => {
    if (!currentProject) {
      return;
    }

    sendMessage(Command.watchStart, { root: currentProject });
    refreshTaskRunDetection();
  });

  // What this workspace is made of - nested checkouts and declared packages, as
  // one list (stores/workspaceMembers). Keyed on the project so an in-window
  // switch re-discovers them; the member switcher, the branch pill and the
  // Change Feed's grouping all read that one list.
  $effect(() => {
    const project = currentProject;
    if (!project) {
      return;
    }

    untrack(discoverWorkspaceMembers);
  });

  async function discoverWorkspaceMembers() {
    await loadWorkspaceMembers(currentProject);
  }

  // The branch pill reports the member the switcher points at, so picking a
  // nested checkout re-reads its branch, its branch list and its remote.
  $effect(() => {
    const member = activeMemberIn(currentProject);
    if (!member) {
      return;
    }

    untrack(repository.load);
  });

  // ── Discord Rich Presence ────────────────────────────────────────────────────
  // Broadcast "Playing PADE" (opt-in), optionally naming the open project. The
  // state→bridge mapping lives in lib/shell/discord-presence (SoC). Both flags resolve
  // through `effective` - the one home for their defaults, shared with the Config
  // panel's toggles - so a change there reaches the presence the moment it lands.
  // $derived so the effect only re-sends when presence, the project-name toggle, or
  // the open project actually change - not on every unrelated settings reassignment
  // (a pin, an editor rule).
  const discordEnabled = $derived(effective.discordPresence);
  const discordShowProject = $derived(effective.discordShowProject);
  // The open project's language, shown VS-Code-style as a small overlay icon +
  // status line. Fetched only while presence will actually show it, and refreshed
  // on a project switch.
  const discordProjectKind = $derived(projectKind(currentProject) ?? undefined);
  $effect(() => {
    if (discordEnabled && discordShowProject && currentProject) {
      requestProjectKind(currentProject);
    }
  });
  // Presence follows the FOCUSED window: every PADE window shares one Discord
  // connection (one process), so only the focused window publishes - switching
  // windows switches the reported project, and an unfocused window never
  // overwrites the focused one's status. A blur sends nothing; the window
  // gaining focus overwrites.
  let isWindowFocused = $state(document.hasFocus());
  $effect(() => {
    if (!isWindowFocused) {
      return;
    }

    updateDiscordPresence({
      isEnabled: discordEnabled,
      isProjectShown: discordShowProject,
      project: currentProject,
      // Read here so a rename (a new settings.labels entry) re-runs this effect
      // and re-publishes the friendly name, not the stale temp-<stamp> folder.
      labels: settings.labels,
      kind: discordProjectKind
    });
  });

  // Auto-name a temp workspace once the agent has produced real work
  // (lib/agent/auto-name): after a few distinct files change, ask the agent (or a
  // heuristic) for a friendly label. Fires once per workspace; label-only.
  const autoNamer = createAutoNamer({
    currentProject: () => currentProject,
    isOptedOut: () => settings.prefs.autoNameTemp === false,
    labelOf: path => settings.labels[path],
    activeAgentCommand: () => activeSession?.agent.command ?? ""
  });
  onMount(() => {
    autoNamer.start();
    return () => autoNamer.dispose();
  });

  // Whether this window names its session tabs with AI - the project's own choice
  // when it has one, else the global default (lib/agent/ai-naming-scope).
  const namesSessionsWithAi = $derived(
    aiNamingEnabled({
      prefs: settings.prefs,
      project: currentProject
    })
  );

  // Apply that setting to every tab already open, and again whenever it changes -
  // including when this window switches project, since the new project may answer
  // differently. `sessions` is read untracked on purpose: this effect exists to
  // follow the SETTING, and re-running it on every tab open or close would undo a
  // ✦ the user had just flipped by hand. New tabs are seeded in `launch` instead.
  $effect(() => {
    applyNamingToAll({
      sessions: untrack(() => sessions),
      isOn: namesSessionsWithAi
    });
  });

  // Send-from-IDE bridge (lib/terminal/send-shortcut): copy in any external editor, press
  // the global shortcut, and the clipboard lands in the active agent's input.
  onMount(() => {
    registerSendShortcut({
      activeId: () => activeId,
      activeLabel: () => activeSession?.agent.label ?? "agent"
    });
    return () => unregisterSendShortcut();
  });

  // Tab shortcuts (lib/session/tab-shortcuts): capture-phase so they win over a focused
  // agent terminal - new tab, launch menu, close, and next/previous cycling.
  onMount(() =>
    registerTabShortcuts({
      // Another tab of the last session's kind (mirrors the "+" button).
      newTab: () => {
        if (phase !== Phase.Ready) {
          return;
        }

        const agent = sessions.at(-1)?.agent ?? bestAgent;
        if (agent) {
          launch({ agent });
        }
      },
      // Open the launch dropdown with the first agent focused, so
      // Enter fires it and Esc light-dismisses (native popover handles both).
      launchMenu: () => {
        if (phase !== Phase.Ready) {
          return;
        }

        const elMenu = document.getElementById(ADD_SESSION_MENU_ID);
        if (!(elMenu instanceof HTMLElement) || elMenu.matches(":popover-open")) {
          return;
        }

        elMenu.showPopover();
        elMenu.querySelector<HTMLButtonElement>("button")?.focus();
      },
      closeTab: closeActiveTab,
      next: () => stepSession(1),
      previous: () => stepSession(-1),
      // Ctrl+number - jump straight to a tab by position (Ctrl+9 = the last one).
      // The matcher only ever hands back an index that exists, so the guard is
      // belt-and-suspenders; selection reuses the same make-active path as a click.
      selectTab: index => {
        const session = sessions[index];
        if (session) {
          selectSession(session.id);
        }
      },
      tabCount: () => sessions.length
    }));

  // Pane shortcuts (lib/session/pane-shortcuts), active while a tab is split into panes:
  // Ctrl+[ / Ctrl+] cycle the active pane (wrapping), Ctrl+Alt+1..9 jump to the
  // nth, and Ctrl+Alt+W closes the active pane's session - the slot then animates
  // out via `out:collapsePane`, and closing the sole pane closes the tab. Selecting
  // a pane only moves focus within the split (never collapses it, unlike a tab
  // click). Capture-phase like the tab shortcuts so they beat a focused terminal.
  onMount(() =>
    registerPaneShortcuts({
      selectPane: id => (activeId = id),
      closeActivePane: closeActiveTab,
      paneIds: () => paneIds,
      activeId: () => activeId
    }));

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
  // up to GRACEFUL_LEAVE_TIMEOUT_MS while it stays fully interactive, so a
  // second switch/leave starting inside that window would interleave two
  // kill → open → launch sequences over the same session list. Later attempts
  // are dropped, not queued - the first leave is already taking the user away.
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

  async function openProject(target: OpenTarget) {
    // If another window already has this project open, focus it instead of
    // opening a second copy here - the picker window stays put.
    if (await sendMessage(Command.windowFocusProject, { path: target.path })) {
      return;
    }

    // Picking the project this window already has open is a return, not a
    // reopen - the live sessions stay exactly as they were.
    const isReturnToCurrent =
      normalizePath(target.path) === normalizePath(currentProject) && sessions.length > 0;
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
    launch({
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

  // When each session launched, so a self-exit right after reads as a failed
  // start (lib/session/session-exit) and is not respawned into a loop.
  const sessionLaunchedAt = new SvelteMap<string, number>();
  // A hand-close also kills the PTY, firing the exit event; the exit handler skips
  // these so the two teardown paths don't race.
  const closingByHand = new SvelteSet<string>();

  // A pane that was one of several in a live split when its session detached
  // collapses its width so the survivors glide across to fill (out:collapsePane).
  // Every other removal - a sole-pane tab close above all - swaps instantly: with
  // nothing beside it, a collapsing pane would only crush the surviving terminal
  // to near-zero width and let it reflow back out, a needless squeeze. So just
  // genuine split members are recorded here (and dropped again on `outroend`);
  // the rest get a zero-duration outro.
  const collapsingSplitPanes = new SvelteSet<string>();

  function launch(options: {
    agent: Agent;
    initialPrompt?: string;
    cwd?: string;
    branch?: string;
    /** Extra command args - the project path when running a terminal editor. */
    args?: string[];
    /** Add alongside the current panes (split) instead of replacing them. */
    split?: boolean;
    /** A model / effort to run this session on (a successor keeping its
     *  predecessor's pick). */
    modelChoice?: ModelChoice;
    /** The conversation to carry on - a session resumed after its folder
     *  moved; a new one otherwise. */
    conversationId?: string;
  }) {
    const session: AgentSession = {
      id: crypto.randomUUID(),
      agent: options.agent,
      initialPrompt: options.initialPrompt,
      modelChoice: startingModelChoice({
        explicit: options.modelChoice,
        prefs: settings.prefs,
        project: currentProject,
        agent: options.agent.id
      }),
      // Never leave an agent to inherit the backend process cwd: another PADE
      // window may switch that shared process to a different project while this
      // terminal is mounting. Worktrees still override the window project, and a
      // multi-part workspace starts its agents in the member the top bar points
      // at - the root until you pick another (see stores/workspaceMembers).
      cwd: options.cwd ?? activeMemberIn(currentProject),
      branch: options.branch,
      args: options.args,
      // A stable id for this conversation, distinct from the session `id` (which
      // is re-keyed to remount the terminal on a restart). Pins the agent's own
      // conversation so a restart resumes THIS one - see restartForMcpChange.
      conversationId: options.conversationId ?? crypto.randomUUID()
    };
    sessions.push(session);
    // A new tab is born under the window's setting - the same one the effect
    // above applies to the tabs already open.
    setNaming({
      id: session.id,
      agent: session.agent.command,
      isOn: namesSessionsWithAi
    });
    sessionLaunchedAt.set(session.id, Date.now());
    activeId = session.id;
    paneIds = options.split ? [...paneIds, session.id] : [session.id];

    if (options.split) {
      animatePaneIn({
        elPanes,
        id: session.id
      });
    }

    pendingPrompt = undefined;
    phase = Phase.Ready;
    return session.id;
  }

  // A tab click shows that session as the sole pane (classic single view).
  // Collapsing a split is deliberately instant (as the design is): a View
  // Transition here would scale the surviving pane's snapshot as it grows to
  // fill, ghosting the old half-width terminal text over the reflowed full-width
  // grid - the platform morph can't track a terminal that repaints on resize.
  function selectSession(id: string) {
    activeId = id;
    paneIds = [id];
  }

  // Ctrl+W / Ctrl+F4 - close the active session.
  function closeActiveTab() {
    if (!activeSession) {
      return;
    }

    if (sessionTabsHandle) {
      sessionTabsHandle.closeSession(activeSession);
      return;
    }

    close(activeSession);
  }

  // Ctrl+Tab / Alt+Arrow - cycle the active tab, wrapping at the ends.
  function stepSession(delta: number) {
    if (sessions.length === 0) {
      return;
    }

    const iNext = wrappedIndex({
      index: sessions.findIndex(session => session.id === activeId),
      delta,
      count: sessions.length
    });
    selectSession(sessions[iNext].id);
  }

  // Keep the branch pill honest: a branch switch, git init, or remote change in
  // the open project re-fetches what it shows (git://state carries no payload).
  let unlistenGitState: (() => void) | undefined;
  async function subscribeToGitState() {
    unlistenGitState = await onMessage(EventType.gitState, async () => {
      await Promise.all([repository.load(), refreshMemberBranches()]);
    });
  }
  onMount(() => {
    subscribeToGitState();
    return () => unlistenGitState?.();
  });

  // The caller kills the PTY and decides the empty-workspace policy.
  function detachSession(id: string) {
    if (paneIds.length > 1 && paneIds.includes(id)) {
      collapsingSplitPanes.add(id);
    }

    ({ sessions, paneIds, activeId } = layoutWithoutSession({
      layout: {
        sessions,
        paneIds,
        activeId
      },
      id
    }));
    sessionLaunchedAt.delete(id);
    agentRetheme.forget(id);
    dropSessionStatus(id);
    dropAgentActivity(id);
    dropSessionLabel(id);
    dropNaming(id);
    dropUsageLimit(id);
    dropUsagePacing(id);
    dropUsageAdapt(id);
    dropApiError(id);
    dropChoiceAttention(id);
  }

  // How long a deliberate leave waits for a busy agent to reach an idle prompt
  // before killing it anyway - graceful, but never a trap behind a wedged agent.
  const GRACEFUL_LEAVE_TIMEOUT_MS = 30_000;

  // A DELIBERATE leave (switching project, going back to the picker) ends every
  // session - but gracefully: wait for each to reach an idle prompt first
  // (sessionStatus ready, the output-quiet signal - never child-process
  // counting, which mis-reads persistent MCP servers) so nothing mid-flight is
  // severed; the agent auto-saves, so /resume covers continuity. The wait runs
  // before any phase change, while the Terminals are still mounted and
  // publishing status. An accidental reload records no leave - its sessions
  // stay alive and the next boot re-attaches them (session-restore).
  async function closeAllSessionsGracefully() {
    const hasBusySession = sessions.some(session => !isSessionIdle(session.id));
    if (hasBusySession) {
      showToast("Waiting for the agent to finish before leaving…");
    }

    await Promise.all(
      sessions.map(session => whenSessionIdle({
        id: session.id,
        timeoutMs: GRACEFUL_LEAVE_TIMEOUT_MS
      }))
    );
    await closeAllSessions();
  }

  // Stop every runner and close every session on a deliberate workspace leave.
  // The runner store is scoped to this window, so clearing it also removes the
  // old workspace's dock before the new project paints.
  async function closeWorkspaceGracefully() {
    await stopAllRunners();
    await closeAllSessionsGracefully();
  }

  // Tear down every session at once - the project-switch path. Each PTY is
  // killed (reaping its child, so the old workspace's cwd lock is released)
  // and its exit event is claimed as a hand-close so the exit handler never
  // races this with a respawn or a return to the picker.
  async function closeAllSessions() {
    await closeSessions(sessions.map(session => session.id));
  }

  // Kill these sessions' PTYs and detach them, claiming each exit event as a
  // hand-close so the exit handler never respawns or re-routes over it.
  async function closeSessions(ids: readonly string[]) {
    for (const id of ids) {
      closingByHand.add(id);
    }

    await Promise.all(ids.map(id => sendMessage(Command.ptyKill, { id })));

    for (const id of ids) {
      detachSession(id);
      closingByHand.delete(id);
    }
  }

  async function close(session: AgentSession) {
    closingByHand.add(session.id);
    await sendMessage(Command.ptyKill, { id: session.id });

    // `pty_kill` yields to the event loop. During that gap a project switch (or
    // another close) can already have detached this session and launched a
    // replacement. A stale close must never send that live workspace back to
    // onboarding.
    const isStillOpen = sessions.some(current => current.id === session.id);
    if (!isStillOpen) {
      closingByHand.delete(session.id);
      return;
    }

    detachSession(session.id);
    closingByHand.delete(session.id);

    // A hand-close is never a failed start: a real project replaces its final
    // tab with a fresh one rather than dropping the user into onboarding.
    if (sessions.length === 0) {
      settleEmptyWorkspace({
        agent: session.agent,
        isFailedStart: false
      });
    }
  }

  // The workspace's last session just ended (lib/session/session-exit decides what next).
  function settleEmptyWorkspace({ agent, isFailedStart }: {
    agent: Agent;
    isFailedStart: boolean;
  }) {
    const outcome = emptyWorkspaceOutcome({
      isUnnamedTemp,
      isFailedStart
    });
    if (outcome === EmptyWorkspaceOutcome.Picker) {
      returnToPicker();
      return;
    }

    if (outcome === EmptyWorkspaceOutcome.Respawn) {
      launch({ agent });
      return;
    }

    pendingPrompt = undefined;
    phase = Phase.Onboarding;
  }

  // Kill the targets and re-key them to fresh ids in place - each terminal
  // remounts, respawns its agent (resuming its conversation via the session id),
  // and the pane layout follows. The shared core behind every in-place restart
  // (spawn-time theme changes).
  async function restartSessions(targets: readonly AgentSession[]) {
    await Promise.all(
      targets.map(target => {
        closingByHand.add(target.id);
        return sendMessage(Command.ptyKill, { id: target.id });
      })
    );

    const rekeyed: Record<string, string> = {};
    for (const target of targets) {
      const restartedId = crypto.randomUUID();
      rekeyed[target.id] = restartedId;
      // Carry the ✦ auto-naming toggle across the re-key so a reset session keeps
      // it on (same agent, so its command is unchanged).
      transferNaming({
        fromId: target.id,
        toId: restartedId,
        agent: target.agent.command
      });
      sessionLaunchedAt.set(restartedId, Date.now());
      sessionLaunchedAt.delete(target.id);
      agentRetheme.forget(target.id);
      closingByHand.delete(target.id);
    }

    const relaid = rekeyLayout({
      sessions,
      paneIds,
      activeId,
      rekeyed
    });
    sessions = relaid.sessions;
    paneIds = relaid.paneIds;
    activeId = relaid.activeId;
  }

  // A terminal-scheme flip reaches the running agents through
  // lib/stores/agent/agentRetheme: Claude repaints in place, the spawn-themed agents
  // are respawned on their own conversation once idle.
  const agentRetheme = createAgentRetheme({
    sessions: () => sessions,
    currentProject: () => currentProject,
    applyResumeArgs,
    restartSessions
  });
  $effect(() => agentRetheme.recordSpawnSchemes());
  $effect(() => agentRetheme.followSchemeChange());

  // Hand a session the args that resume its own conversation, so the restart
  // that follows reopens it rather than starting a blank one.
  function applyResumeArgs({ id, args }: {
    id: string;
    args: string[];
  }) {
    if (args.length === 0) {
      return;
    }

    sessions = sessions.map(session => session.id === id ? {
      ...session,
      args
    } : session);
  }

  function switchToPicker() {
    document.startViewTransition(async () => {
      phase = Phase.Project;
      await tick();
    });
  }

  // Hand the project back: this window no longer claims it, so another window
  // (or this one, later) can open it fresh - no picker tries to focus us here.
  function releaseProject() {
    currentProject = "";
    repository.clear();
    pendingPrompt = undefined;
    sendMessage(Command.windowRegisterProject, { path: "" });
  }

  // Hand the project back and show the picker. Every caller has already
  // killed/reaped the project's PTYs, so nothing keeps running in it.
  function returnToPicker() {
    releaseProject();
    switchToPicker();
  }

  // ── Relocate (move / rename) with lock handling ─────────────────────────────
  // The kill → backend-op → resume flow lives in lib/workspace/workspace-relocate; this
  // shell only lends it the session list and takes back the results.
  const relocator = createRelocator({
    sessions: () => sessions,
    currentProject: () => currentProject,
    markClosing(ids) {
      for (const id of ids) {
        closingByHand.add(id);
      }
    },
    removeSessions(ids) {
      ({ sessions, paneIds, activeId } = layoutWithoutSessions({
        layout: {
          sessions,
          paneIds,
          activeId
        },
        ids
      }));

      for (const id of ids) {
        closingByHand.delete(id);
      }
    },
    stopRunners: stopAllRunners,
    async setCurrentProject(path) {
      currentProject = path;
      await sendMessage(Command.windowRegisterProject, { path });
      // The URL follows the project in an effect; a crash rebuild must reopen
      // the folder where it now is.
      await tick();
      await sendMessage(Command.recoveryArm, { url: location.href });
    },
    relaunch: launch
  });

  // ── Auto-handoff ───────────────────────────────────────────────────────────
  // Near-limit sessions hand off to a fresh agent; the machinery lives in
  // lib/stores/agent/handoff. The scan runs inside this $effect so it re-fires as the
  // session list, prefs and context stores change.
  const autoHandoff = createAutoHandoff({
    sessions: () => sessions,
    availableAgents: () => realAgents,
    isOptedOut: () => settings.prefs.autoHandoff === false,
    thresholdPercentage: () => effective.handoffPercentage,
    slugSource: () => projectLabel,
    projectDirectory: () => currentProject,
    // Announced close: without the flag, this kill's own exit event reaches
    // handleSessionExit as "the agent quit on its own" and respawns a
    // replacement - a bare extra tab that also steals the active pane from the
    // successor being seeded right behind it. Held until removeSession has
    // dropped the id, so a late exit event finds neither the flag nor the
    // session.
    async endSession(id) {
      closingByHand.add(id);
      await Promise.allSettled([sendMessage(Command.ptyKill, { id })]);
    },
    removeSession(id) {
      sessions = sessions.filter(session => session.id !== id);
      paneIds = paneIds.filter(paneId => paneId !== id);
      closingByHand.delete(id);
    },
    launchSuccessor({ agent, cwd, initialPrompt, predecessorId }) {
      // The successor keeps the model picked for its predecessor - including
      // one picked while this handoff was under way - unless a usage
      // crossover hands the work to a different agent.
      const predecessor = sessions.find(session => session.id === predecessorId);
      const isSameAgent = predecessor?.agent.id === agent.id;
      const successorId = launch({
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
      sessions = sessions.map(session => session.id === id ? {
        ...session,
        modelChoice: {
          ...session.modelChoice,
          ...choice
        }
      } : session);
    },
    async relaunch(id) {
      const target = sessions.find(session => session.id === id);
      if (target === undefined) {
        return;
      }

      applyResumeArgs({
        id,
        args: await sendMessage(Command.agentResumeArgs, {
          command: target.agent.command,
          cwd: target.cwd ?? currentProject,
          conversationId: target.conversationId
        })
      });
      showToast(`Switching ${target.agent.label}'s model - resuming its conversation`);
      await restartSessions([target]);
    },
    isOpen: id => sessions.some(session => session.id === id),
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
      const session = sessions.find(candidate => candidate.id === id);
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
    sessions: () => sessions,
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
    sessions: () => sessions,
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
    for (const session of sessions) {
      reconcileChoiceAttention({
        id: session.id,
        isActive: session.id === activeId
      });
    }
  });

  // The open side panel (lib/shell/side-panels); `null` means none is open.
  let side = $state<Side | null>(Side.Feed);
  // The side column's width, which SidePanelResizer moves live during a drag.
  let sidePanelWidth = $state(effective.sidePanelWidth);
  let isResizingSidePanel = $state(false);
</script>

<svelte:document
  onvisibilitychange={() => {
    if (!document.hidden) {
      redetectAgents();
    }
  }}
/>
<!-- Window-level shortcuts, handled in the capture phase: a focused xterm calls
     stopPropagation on keys it handles, so a bubble-phase listener never sees the
     combo while the terminal has focus. -->
<svelte:window
  onblur={() => {
    isWindowFocused = false;
  }}
  onfocus={() => {
    isWindowFocused = true;
  }}
  onkeydowncapture={async e => {
    // The chords themselves live in lib/shell/window-shortcuts.
    const shortcut = windowShortcutFor({
      event: e,
      isDevelopment: import.meta.env.DEV
    });
    if (shortcut === null) {
      return;
    }

    e.preventDefault();

    if (shortcut === WindowShortcut.Reload) {
      location.reload();
      return;
    }

    // Keeps the terminal from also receiving the chord.
    e.stopPropagation();

    if (shortcut === WindowShortcut.NewWindow) {
      await sendMessage(Command.windowCreate, { mode: WindowMode.enum.empty });
      showToast("Opened a new window");
      return;
    }

    await sendMessage(Command.windowFocusRelative, {
      direction: shortcut === WindowShortcut.PreviousWindow ? "previous" : "next"
    });
  }}
/>

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
      onpick={agent => launch({
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
          {activeSession}
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
              sessionIds: sessions.map(session => session.id),
              continuing: activeSession ?? sessions[0],
              writeHandoffDoc: autoHandoff.writeHandoffDoc,
              reroot: path => (currentProject = path),
              launch,
              closeSessions
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
            launch({
              agent: {
                id: `editor-${editor.id}`,
                label: editor.label,
                command: editor.command,
                // A terminal editor writes logical order like any other program.
                reordersBidi: false
              },
              // Inherit the active session's worktree, if any, else the project dir.
              cwd: activeSession?.cwd,
              // Open the working directory in the editor.
              args: ["."],
              split: true
            })}
          project={currentProject}
          {repository}
          {sessions}
          bind:side
        />

        <SessionTabs
          bind:this={sessionTabsHandle}
          {activeId}
          {agents}
          branches={repository.branches}
          onclose={close}
          ondraghint={hint => (dragHint = hint)}
          onlaunch={agent => launch({ agent })}
          onlaunchbranch={async branch => {
            // Spawn an agent on its own git worktree for `branch`, isolated from
            // the other sessions. Uses the active session's agent (or the first).
            const agent = activeSession?.agent ?? bestAgent;
            const cwd = await sendMessage(Command.vcsWorktreeAdd, {
              cwd: activeMemberIn(currentProject),
              branch,
              create: false
            });
            launch({
              agent,
              cwd,
              branch
            });
          }}
          onmodelchoice={async pick => await modelSwitcher.request(pick)}
          onreorder={orderedIds => (sessions = sessionsAfterReorder({
            sessions,
            orderedIds
          }))}
          onselect={selectSession}
          onsplit={drop => {
            // Show the dropped tab as a split pane on the side of the pane the
            // pointer landed on (repositioning it if already shown).
            const target = paneDropAt({
              elPanes,
              paneIds,
              x: drop.pointerX,
              y: drop.pointerY
            });
            if (!target) {
              return;
            }

            paneIds = paneIdsAfterSplit({
              paneIds,
              draggedId: drop.id,
              targetId: target.id,
              side: target.side
            });
            activeId = drop.id;
            animatePaneIn({
              elPanes,
              id: drop.id
            });
          }}
          {paneIds}
          popPaneActive={isPaneDragOverTabs}
          {sessions}
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
          {collapsingSplitPanes}
          {dragHint}
          onexit={async id => {
            // The PTY exited on its own - e.g. the user pressed Ctrl-C to quit the agent.
            const exited = sessions.find(candidate => candidate.id === id);
            if (closingByHand.has(id) || !exited) {
              return;
            }

            const wasLastSession = sessions.length === 1;
            const isExitFailedStart = didFailToStart({
              launchedAt: sessionLaunchedAt.get(id) ?? 0,
              now: Date.now()
            });
            detachSession(id);
            // Reap the backend record; the child is already gone, so a failure is moot.
            await Promise.allSettled([sendMessage(Command.ptyKill, { id })]);

            if (wasLastSession) {
              settleEmptyWorkspace({
                agent: exited.agent,
                isFailedStart: isExitFailedStart
              });
            }
          }}
          onlaunch={agent => launch({
            agent,
            split: true
          })}
          onselect={selectSession}
          {sessions}
          bind:elPanes
          bind:paneIds
          bind:activeId
          bind:isPaneDragOverTabs
        />

        {#if side !== null}
          <SidePanelResizer bind:width={sidePanelWidth} bind:isResizing={isResizingSidePanel} />
          <SidePanel agent={activeAgent} agents={realAgents} project={currentProject} {side} />
        {/if}
      </main>

      <RunnerDock activeSessionId={activeId} />

      <RecoveryNotes note={autoHandoff.note || usageResume.note || apiErrorRetry.note} pacing={usagePacing} />
      <ShellToast />
      <SendSelectionButton {activeId} />
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
