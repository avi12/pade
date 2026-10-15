<script lang="ts">
  import { startingModelChoice } from "@/lib/agent-model-scope";
  import { aiNamingEnabled } from "@/lib/ai-naming-scope";
  import AppMenu from "@/lib/AppMenu.svelte";
  import { createAutoNamer } from "@/lib/auto-name";
  import BranchSwitcher from "@/lib/BranchSwitcher.svelte";
  import {
    agents as agentsApi,
    feed,
    mcp,
    pty,
    recovery,
    vcs,
    windows,
    workspace
  } from "@/lib/bridge";
  import DesignMenu from "@/lib/DesignMenu.svelte";
  import { clearDiscordPresence, updateDiscordPresence } from "@/lib/discord-presence";
  import type { DragHint } from "@/lib/drag-reorder";
  import Icon from "@/lib/Icon.svelte";
  import IdeMenu from "@/lib/IdeMenu.svelte";
  import Logo from "@/lib/Logo.svelte";
  import MemberSwitcher from "@/lib/MemberSwitcher.svelte";
  import { collapsePane } from "@/lib/motion";
  import OverflowTooltipLayer from "@/lib/OverflowTooltipLayer.svelte";
  import {
    layoutWithoutPane,
    layoutWithoutSession,
    layoutWithoutSessions,
    paneAtPoint,
    paneIdsAfterSplit,
    sessionsAfterReorder,
    wrappedIndex
  } from "@/lib/pane-layout";
  import { registerPaneShortcuts } from "@/lib/pane-shortcuts";
  import { displayName, isTemporaryWorkspace, normalizePath, shortDisplayName } from "@/lib/paths";
  import { effective, SIDE_PANEL_MAX_FRACTION, SIDE_PANEL_MIN_WIDTH } from "@/lib/prefs.svelte";
  import { DropSide } from "@/lib/reorder";
  import { openRepositoryTarget } from "@/lib/repository-links";
  import RunnerDock from "@/lib/RunnerDock.svelte";
  import { saveTempWorkspace } from "@/lib/save-temp-workspace";
  import { registerSendShortcut, unregisterSendShortcut } from "@/lib/send-shortcut";
  import { didFailToStart, EmptyWorkspaceOutcome, emptyWorkspaceOutcome } from "@/lib/session-exit";
  import { mcpRestartTargets, rekeyLayout } from "@/lib/session-restart";
  import { clearSessionSnapshot, restoreLiveSnapshot, saveSessionSnapshot } from "@/lib/session-restore";
  import SessionTabs from "@/lib/SessionTabs.svelte";
  import { settings } from "@/lib/settings.svelte";
  import { settledValue } from "@/lib/settled";
  import { Side, SIDE_PANEL_TABS } from "@/lib/side-panels";
  import { dropAgentActivity } from "@/lib/stores/agentActivity.svelte";
  import { createAgentRetheme } from "@/lib/stores/agentRetheme.svelte";
  import { createApiErrorRetry, dropApiError } from "@/lib/stores/apiErrorRetry.svelte";
  import { createAutoHandoff } from "@/lib/stores/handoff.svelte";
  import { armMcpReloadRecovery, dropMcpReload, failedMcpReloads } from "@/lib/stores/mcpReload.svelte";
  import { choiceForSuccessor, createModelSwitcher } from "@/lib/stores/modelSwitch.svelte";
  import {
    armNewProjectDetection,
    disposeNewProjectDetection,
    initNewProjectDetection
  } from "@/lib/stores/new-project-detection";
  import { projectKind, requestProjectKind } from "@/lib/stores/projectKinds.svelte";
  import { ensureRunnerListeners, stopAllRunners } from "@/lib/stores/runners.svelte";
  import {
    dropChoiceAttention,
    ensureChoiceAttention,
    reconcileChoiceAttention
  } from "@/lib/stores/sessionAttention.svelte";
  import { dropSessionLabel } from "@/lib/stores/sessionLabels.svelte";
  import { applyNamingToAll, dropNaming, setNaming, transferNaming } from "@/lib/stores/sessionNaming.svelte";
  import { dropSessionStatus, isSessionIdle, whenSessionIdle } from "@/lib/stores/sessions.svelte";
  import { initTaskRunDetection, refreshTaskRunDetection } from "@/lib/stores/taskRuns.svelte";
  import { showToast, toastText } from "@/lib/stores/toast.svelte";
  import { createUsageResume, dropUsageLimit } from "@/lib/stores/usageResume.svelte";
  import { activeMemberIn, loadWorkspaceMembers, refreshMemberBranches } from "@/lib/stores/workspaceMembers.svelte";
  import { registerTabShortcuts } from "@/lib/tab-shortcuts";
  import { StartMode, realAgents as toRealAgents, WindowMode } from "@/lib/types";
  import type {
    Agent,
    AgentSession,
    McpChange,
    ModelChoice,
    OpenTarget
  } from "@/lib/types";
  import UsageMeter from "@/lib/UsageMeter.svelte";
  import { FolderPath, parseInput } from "@/lib/validate";
  import { WindowShortcut, windowShortcutFor } from "@/lib/window-shortcuts";
  import { createRelocator } from "@/lib/workspace-relocate";
  import Onboarding from "@/panels/Onboarding.svelte";
  import ProjectPicker from "@/panels/ProjectPicker.svelte";
  import SendSelectionButton from "@/panels/SendSelectionButton.svelte";
  import SidePanel from "@/panels/SidePanel.svelte";
  import SidePanelResizer from "@/panels/SidePanelResizer.svelte";
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";

  // Which top-level screen is showing. A closed set defined once, compared
  // against by member so no bare literal leaks into the flow logic.
  const Phase = {
    loading: "loading",
    project: "project",
    onboarding: "onboarding",
    ready: "ready"
  } as const;
  type Phase = (typeof Phase)[keyof typeof Phase];
  let phase = $state<Phase>(Phase.loading);
  let agents = $state<Agent[]>([]);
  let sessions = $state<AgentSession[]>([]);
  let sessionTabsHandle = $state<{ closeSession: (session: AgentSession) => void } | null>(null);
  let activeId = $state<string | null>(null);
  // Which sessions are shown side by side in the terminal area. One id = the
  // classic single pane; more = a split view. Always a subset of `sessions`.
  let paneIds = $state<string[]>([]);
  let currentProject = $state<string>("");
  // Local branches when the project is a git repo - enables per-branch agents.
  let branches = $state<string[]>([]);
  // Branches only the remote has so far - offered by the branch switcher, which
  // lets git create the local tracking branch on the way in.
  let remoteBranches = $state<string[]>([]);
  // The project's checked-out branch - the top-bar branch pill (empty when the
  // project isn't a git repo or HEAD is detached).
  let currentBranch = $state("");
  // Whether the repo has a remote - a local-only repo has nothing to open in a
  // browser, so the pill's Ctrl-click-opens-on-remote affordance is suppressed.
  let hasRemote = $state(false);
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
  const projectLabel = $derived(shortDisplayName(currentProject, settings.labels));
  // A temp workspace that never earned a name: when its last session ends, the
  // window returns to the picker rather than respawning an agent. Its folder is
  // kept - only the unused-temp cleanup setting ever deletes it. One that was
  // auto-named keeps the normal last-session behavior instead.
  const isUnnamedTemp = $derived(isTemp && !currentLabel);
  const activeSession = $derived(sessions.find(session => session.id === activeId));
  // Active agent id - used to show only its relevant config files.
  const activeAgent = $derived(activeSession?.agent.id ?? "");
  // A pane can be removed only while more than one is shown; sessions not
  // currently shown are offered in the "add to split" menu.
  const isSplit = $derived(paneIds.length > 1);
  const splitCandidates = $derived(sessions.filter(session => !paneIds.includes(session.id)));

  // The terminal slots render in this order: the shown panes first, in `paneIds`
  // order (so a pane-header drag that reorders `paneIds` reorders the view), then
  // the hidden sessions. Keyed by id, so a reorder moves DOM nodes rather than
  // remounting - every terminal keeps its scrollback.
  const bySessionId = $derived(new Map(sessions.map(session => [session.id, session] as const)));
  const orderedSessions = $derived.by(() => {
    const shown = paneIds
      .map(id => bySessionId.get(id))
      .filter((s): s is AgentSession => s !== undefined);
    const hidden = sessions.filter(session => !paneIds.includes(session.id));
    return [...shown, ...hidden];
  });

  // ── Tab drag → reorder / split ──────────────────────────────────────────────
  // `DropSide` (which half of a pane a dragged tab lands on) and the drop's
  // layout math live in `@/lib/reorder` and `@/lib/pane-layout` - DOM-free and
  // unit-tested. (`Side` names the side panels - a different concern.)

  // The panes container, so a drop's pointer can be hit-tested against the panes.
  let elPanes = $state<HTMLElement>();
  // Live drag state from the tab strip; drives the "drop here" overlay + halves.
  let dragHint = $state<DragHint | null>(null);
  const isDragOverPanes = $derived(dragHint?.outside === true);

  // True while a split pane's header is dragged up over the tab strip - the mirror
  // gesture: the strip lights as a "drop → new tab" target and the drop pops the
  // pane out of the split (see the Terminal panes' `onremove`). Reported by the
  // dragged pane's Terminal via the same engine hint the tab-split path uses.
  let isPaneDragOverTabs = $state(false);

  // Which pane + side the drag is currently over - the highlighted drop half.
  const splitTarget = $derived.by(() => {
    if (!isDragOverPanes || !dragHint) {
      return null;
    }

    return paneDropAt({
      x: dragHint.pointerX,
      y: dragHint.pointerY
    });
  });
  function dropSideFor(id: string) {
    return splitTarget?.id === id ? splitTarget.side : null;
  }

  // The shown pane under a point, and which half of it - used both for the live
  // highlight and for the actual drop, so the two never disagree (DRY).
  function paneDropAt({ x, y }: {
    x: number;
    y: number;
  }) {
    const slots = [...elPanes?.querySelectorAll<HTMLElement>("[data-pane-id]") ?? []];
    const panes = slots
      .filter(slot => paneIds.includes(slot.dataset.paneId ?? ""))
      .map(slot => ({
        id: slot.dataset.paneId ?? "",
        rectangle: slot.getBoundingClientRect()
      }));
    return paneAtPoint({
      panes,
      x,
      y
    });
  }

  // Agent detection runs a subprocess per agent, so cap how long the boot waits
  // on it - a stall must never freeze the splash. An empty list just routes to
  // onboarding, and the redetect interval fills it in shortly after.
  const BOOT_DETECT_TIMEOUT_MS = 6_000;
  // The slow fallback poll behind the visibility-driven re-detect below.
  const AGENT_REDETECT_INTERVAL_MS = 30_000;
  // How long a split member takes to collapse out (out:collapsePane).
  const SPLIT_PANE_COLLAPSE_MS = 260;
  // The spring a pane joining a split enters with (animatePaneIn).
  const PANE_ENTER_ANIMATION = "pane-enter 340ms var(--spring)";
  // The tab strip's launch dropdown (SessionTabs owns the element).
  const ADD_SESSION_MENU_ID = "add-session-menu";

  onMount(async () => {
    try {
      await boot();
    } catch {
      // Never strand the user on the splash - fall back to the project picker.
      showToast("Startup hit a snag - pick a project to continue");
      phase = Phase.project;
    }
  });
  onDestroy(disposeNewProjectDetection);

  async function boot() {
    await initNewProjectDetection();
    // Detection is best-effort: a rejection or a stall both yield an empty
    // list rather than blocking the routing below.
    const detecting = Promise.race([
      settledValue({
        promise: agentsApi.detect(),
        fallback: []
      }),
      new Promise<Agent[]>(resolve => setTimeout(() => resolve([]), BOOT_DETECT_TIMEOUT_MS))
    ]);
    [agents] = await Promise.all([detecting, workspace.settings()]);

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
    const context = await workspace.context();
    if (context.hasProject) {
      await workspace.open(context.cwd); // records it in recent history
      startAgentFlow({ path: context.cwd });
      await loadBranches();
      return;
    }

    // Opt-in: show the project picker instead of starting in a temp workspace.
    const prefersPicker = settings.prefs.startMode === StartMode.enum.picker;
    if (prefersPicker) {
      phase = Phase.project;
      return;
    }

    // Default: start immediately in a throwaway workspace so there's no
    // blocking picker. The user can switch any time (Switch button).
    await startInTempWorkspace();
  }

  async function startInTempWorkspace() {
    startAgentFlow({
      path: await workspace.temp(),
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
      phase = Phase.project;
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
      await workspace.open(snapshot.project);
    } catch {
      // The project vanished while the window was reloading (deleted, moved,
      // unmounted). Boot falls through to the picker - so kill the snapshot's
      // still-live sessions rather than leave them running unreachably, which
      // would recreate the very invisible-agent incident restore exists to fix.
      await Promise.allSettled(snapshot.sessions.map(orphan => pty.kill(orphan.id)));
      clearSessionSnapshot();
      return false;
    }

    currentProject = snapshot.project;
    windows.registerProject(snapshot.project);
    sessions = snapshot.sessions;
    paneIds = snapshot.paneIds;
    activeId = snapshot.activeId;
    // Stamped as launched-now so a session that dies right after the restore
    // reads as a failed start (no respawn loop), like any fresh launch.
    for (const restored of snapshot.sessions) {
      sessionLaunchedAt.set(restored.id, Date.now());
    }

    phase = Phase.ready;
    await loadBranches();
    return true;
  }

  // Persist this window's pane mapping so a reload can re-attach it (above).
  // Never while still booting - the effect's first run lands before the restore
  // has read the snapshot, and would wipe it. Once live, an empty project or
  // session list clears the snapshot instead: nothing to re-attach.
  $effect(() => {
    if (phase === Phase.loading) {
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
    if (phase === Phase.loading) {
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
      promise: agentsApi.detect(),
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
    await recovery.arm(location.href);
  });

  // Watch the PTY stream once for the agent's multiple-choice prompts, so a tab
  // can flash red when one is pending on it (lib/stores/sessionAttention).
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
    unlistenSettings = await workspace.onChanged(async () => {
      await Promise.allSettled([workspace.settings()]);
    });
  }
  onMount(() => {
    subscribeToSettingsChanges();
    return () => unlistenSettings?.();
  });

  // Hand off affected agent sessions when the project's MCP servers change.
  let unlistenMcp: (() => void) | undefined;
  async function subscribeToMcpChanges() {
    unlistenMcp = await mcp.onChanged(change => restartForMcpChange(change));
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
    const workspace = displayName(currentProject, settings.labels);
    windows.setTitle(hasNamedProject ? `PADE - ${workspace}` : "PADE");
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

    feed.start(currentProject);
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

    untrack(loadBranches);
  });

  // ── Discord Rich Presence ────────────────────────────────────────────────────
  // Broadcast "Playing PADE" (opt-in), optionally naming the open project. The
  // state→bridge mapping lives in lib/discord-presence (SoC). Both flags resolve
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
      enabled: discordEnabled,
      showProject: discordShowProject,
      project: currentProject,
      // Read here so a rename (a new settings.labels entry) re-runs this effect
      // and re-publishes the friendly name, not the stale temp-<stamp> folder.
      labels: settings.labels,
      kind: discordProjectKind
    });
  });

  // Auto-name a temp workspace once the agent has produced real work
  // (lib/auto-name): after a few distinct files change, ask the agent (or a
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
  // when it has one, else the global default (lib/ai-naming-scope).
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
    const on = namesSessionsWithAi;
    applyNamingToAll({
      sessions: untrack(() => sessions),
      on
    });
  });

  // Send-from-IDE bridge (lib/send-shortcut): copy in any external editor, press
  // the global shortcut, and the clipboard lands in the active agent's input.
  onMount(() => {
    registerSendShortcut({
      activeId: () => activeId,
      activeLabel: () => activeSession?.agent.label ?? "agent"
    });
    return () => unregisterSendShortcut();
  });

  // Tab shortcuts (lib/tab-shortcuts): capture-phase so they win over a focused
  // agent terminal - new tab, launch menu, close, and next/previous cycling.
  onMount(() =>
    registerTabShortcuts({
      newTab,
      launchMenu: openLaunchMenu,
      closeTab: closeActiveTab,
      next: () => stepSession(1),
      previous: () => stepSession(-1),
      selectTab: selectTabByIndex,
      tabCount: () => sessions.length
    }));

  // Pane shortcuts (lib/pane-shortcuts), active while a tab is split into panes:
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
    unlistenCloseRequested = await windows.onCloseRequested(async () => {
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
    if (await windows.focusProject(target.path)) {
      return;
    }

    // Picking the project this window already has open is a return, not a
    // reopen - the live sessions stay exactly as they were.
    const isReturnToCurrent =
      normalizePath(target.path) === normalizePath(currentProject) && sessions.length > 0;
    if (isReturnToCurrent) {
      phase = Phase.ready;
      return;
    }

    // Switching this window to another project: the old project's agents don't
    // come along. Kill them before entering, so no session keeps running - and
    // cwd-locking - a workspace this window has left.
    await runExclusiveLeave(async () => {
      await closeWorkspaceGracefully();

      await workspace.open(target.path);
      await workspace.settings(); // pick up the updated recent history
      startAgentFlow({
        path: target.path,
        initialPrompt: target.initialPrompt,
        agentId: target.agent,
        isNewProject: target.created === true
      });
      await loadBranches();
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
    windows.registerProject(path);
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
  // start (lib/session-exit) and is not respawned into a loop.
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
      on: namesSessionsWithAi
    });
    sessionLaunchedAt.set(session.id, Date.now());
    activeId = session.id;
    paneIds = options.split ? [...paneIds, session.id] : [session.id];

    if (options.split) {
      animatePaneIn(session.id);
    }

    pendingPrompt = undefined;
    phase = Phase.ready;
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

  // Spring just the added pane in from its trailing edge (pane-enter). Fired on
  // the one slot that joined a split - never on a plain tab open / close / switch,
  // which stay instant - so it can't run on every show the way a CSS rule on
  // `.shown` would (that wiped the terminal in on any tab change). Clears the
  // inline animation once it settles so a later show/hide of the same slot can't
  // replay it. Mirrors the design's imperative `animatePaneIn`.
  async function animatePaneIn(id: string) {
    await tick();
    const elSlot = elPanes?.querySelector<HTMLElement>(`[data-pane-id="${id}"]`);
    if (!elSlot) {
      return;
    }

    elSlot.style.animation = "none";
    // Force a synchronous reflow between clearing and re-setting the animation so
    // re-adding the same pane restarts the run; getBoundingClientRect flushes
    // layout the same as reading offsetWidth, but as a call it isn't a bare
    // "unused expression" (nor an unused local, which a discard const would be).
    elSlot.getBoundingClientRect();
    elSlot.style.animation = PANE_ENTER_ANIMATION;
    elSlot.addEventListener("animationend", () => (elSlot.style.animation = ""), { once: true });
  }

  // Show an already-running session alongside the current pane(s) - a split-add,
  // so the newcomer springs in (animatePaneIn).
  function addPane(id: string) {
    if (!paneIds.includes(id)) {
      paneIds = [...paneIds, id];
      animatePaneIn(id);
    }

    activeId = id;
  }

  // Drop a pane from the split - never the last visible one.
  function removePane(id: string) {
    if (!isSplit) {
      return;
    }

    ({ paneIds, activeId } = layoutWithoutPane({
      layout: {
        sessions,
        paneIds,
        activeId
      },
      id
    }));
  }

  // ── Tab keyboard shortcuts (lib/tab-shortcuts) ───────────────────────────────
  // Actions the shortcut layer drives; each is a no-op outside the ready phase.

  // Another tab of the last session's kind (mirrors the "+" button).
  function newTab() {
    if (phase !== Phase.ready) {
      return;
    }

    const agent = sessions.at(-1)?.agent ?? bestAgent;
    if (agent) {
      launch({ agent });
    }
  }

  // Open the launch dropdown with the first agent focused, so
  // Enter fires it and Esc light-dismisses (native popover handles both).
  function openLaunchMenu() {
    if (phase !== Phase.ready) {
      return;
    }

    const elMenu = document.getElementById(ADD_SESSION_MENU_ID);
    if (!(elMenu instanceof HTMLElement) || elMenu.matches(":popover-open")) {
      return;
    }

    elMenu.showPopover();
    elMenu.querySelector<HTMLButtonElement>("button")?.focus();
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

  // Ctrl+number - jump straight to a tab by position (Ctrl+9 = the last one).
  // The matcher only ever hands back an index that exists, so the guard is
  // belt-and-suspenders; selection reuses the same make-active path as a click.
  function selectTabByIndex(index: number) {
    const session = sessions[index];
    if (session) {
      selectSession(session.id);
    }
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

  // Load branches for the repo the top bar points at - the workspace root, or
  // the nested checkout the member switcher selected - and the checked-out
  // branch its pill shows. A workspace with one member reads exactly as before.
  async function loadBranches() {
    const member = activeMemberIn(currentProject);
    if (!member) {
      branches = [];
      remoteBranches = [];
      currentBranch = "";
      hasRemote = false;
      return;
    }

    // Not a git repository (or git missing) reads as no branches and no remote.
    const [next, onlyRemote, heads, remote] = await Promise.all([
      settledValue({
        promise: vcs.branches(member),
        fallback: []
      }),
      settledValue({
        promise: vcs.remoteBranches(member),
        fallback: []
      }),
      settledValue<Record<string, string>>({
        promise: vcs.branchOf([member]),
        fallback: {}
      }),
      settledValue<string | null>({
        promise: vcs.remoteUrl(member),
        fallback: null
      })
    ]);
    // A project (or member) switch can outrun this read. Keep the branch list
    // attached to what it came from instead of flashing another repository's.
    if (member !== activeMemberIn(currentProject)) {
      return;
    }

    branches = next;
    remoteBranches = onlyRemote;
    currentBranch = heads[member] ?? "";
    hasRemote = remote !== null;
  }

  // Keep the branch pill honest: a branch switch, git init, or remote change in
  // the open project re-fetches what it shows (git://state carries no payload).
  let unlistenGitState: (() => void) | undefined;
  async function subscribeToGitState() {
    unlistenGitState = await vcs.onStateChanged(async () => {
      await Promise.all([loadBranches(), refreshMemberBranches()]);
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

    await Promise.all(ids.map(id => pty.kill(id)));

    for (const id of ids) {
      detachSession(id);
      closingByHand.delete(id);
    }
  }

  async function close(session: AgentSession) {
    closingByHand.add(session.id);
    await pty.kill(session.id);

    // `pty.kill` yields to the event loop. During that gap a project switch (or
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

  // The PTY exited on its own - e.g. the user pressed Ctrl-C to quit the agent.
  async function handleSessionExit(id: string) {
    const session = sessions.find(candidate => candidate.id === id);
    if (closingByHand.has(id) || !session) {
      return;
    }

    const wasLastSession = sessions.length === 1;
    const isExitFailedStart = didFailToStart({
      launchedAt: sessionLaunchedAt.get(id) ?? 0,
      now: Date.now()
    });
    detachSession(id);
    // Reap the backend record; the child is already gone, so a failure is moot.
    await Promise.allSettled([pty.kill(id)]);

    if (wasLastSession) {
      settleEmptyWorkspace({
        agent: session.agent,
        isFailedStart: isExitFailedStart
      });
    }
  }

  // The workspace's last session just ended (lib/session-exit decides what next).
  function settleEmptyWorkspace({ agent, isFailedStart }: {
    agent: Agent;
    isFailedStart: boolean;
  }) {
    const outcome = emptyWorkspaceOutcome({
      isUnnamedTemp,
      isFailedStart
    });
    if (outcome === EmptyWorkspaceOutcome.picker) {
      returnToPicker();
      return;
    }

    if (outcome === EmptyWorkspaceOutcome.respawn) {
      launch({ agent });
      return;
    }

    pendingPrompt = undefined;
    phase = Phase.onboarding;
  }

  // A project's MCP servers changed (a server added or removed - the backend
  // fires only on a real membership change, never a value-only edit). Each
  // affected agent first writes a handoff document, then a fresh instance of the
  // SAME agent launches from it. The fresh process reads the changed MCP config;
  // the handoff preserves work that an immediate conversation restart could
  // interrupt. Worktrees remain scoped to their own config directory.
  async function restartForMcpChange(change: McpChange) {
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
  }

  // Kill the targets and re-key them to fresh ids in place - each terminal
  // remounts, respawns its agent (resuming its conversation via the session id),
  // and the pane layout follows. The shared core behind every in-place restart
  // (spawn-time theme changes).
  async function restartSessions(targets: readonly AgentSession[]) {
    await Promise.all(
      targets.map(target => {
        closingByHand.add(target.id);
        return pty.kill(target.id);
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
  // lib/stores/agentRetheme: Claude repaints in place, the spawn-themed agents
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

  // Clear the recent-projects history from the switcher (pins survive).
  async function clearRecentProjects() {
    await workspace.clearRecent();
  }
  // Pin/unpin a project from the switcher; the bridge adopts the response.
  async function toggleProjectPin(target: {
    path: string;
    pinned: boolean;
  }) {
    await workspace.setPinned(target);
  }
  // Forget a project from the switcher's lists (folder untouched).
  async function removeRecentProject(projectPath: string) {
    await workspace.removeRecent(projectPath);
  }
  // Persist a drag-reordered pin order from the switcher.
  async function reorderPins(paths: string[]) {
    await workspace.setPinnedOrder(paths);
  }

  // "Delete directory" from the switcher - a destructive removal of a real
  // project's folder. The switcher owns the confirmation UI (so it stays open and
  // animates the row out); here we just release the folder (killing any session
  // holding it) and delete it, letting the refreshed settings flow back. Rejects
  // with its message so the switcher can surface it in the still-open prompt.
  async function deleteDirectory(projectPath: string) {
    await relocator.removeDirectory(projectPath);
  }

  function switchToPicker() {
    document.startViewTransition(async () => {
      phase = Phase.project;
      await tick();
    });
  }

  // Hand the project back: this window no longer claims it, so another window
  // (or this one, later) can open it fresh - no picker tries to focus us here.
  function releaseProject() {
    currentProject = "";
    branches = [];
    remoteBranches = [];
    currentBranch = "";
    pendingPrompt = undefined;
    windows.registerProject("");
  }

  // "Switch project" - a DELIBERATE leave to the picker. The project's agents
  // and task runners do not idle invisibly behind it: runners stop immediately,
  // then agents are killed once they reach an idle prompt.
  async function leaveToPicker() {
    await runExclusiveLeave(async () => {
      await closeWorkspaceGracefully();
      returnToPicker();
    });
  }

  // Hand the project back and show the picker. Every caller has already
  // killed/reaped the project's PTYs, so nothing keeps running in it.
  function returnToPicker() {
    releaseProject();
    switchToPicker();
  }

  // ── Relocate (move / rename) with lock handling ─────────────────────────────
  // The kill → backend-op → resume flow lives in lib/workspace-relocate; this
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
      await windows.registerProject(path);
      // The URL follows the project in an effect; a crash rebuild must reopen
      // the folder where it now is.
      await tick();
      await recovery.arm(location.href);
    },
    relaunch: launch
  });

  // ── Auto-handoff ───────────────────────────────────────────────────────────
  // Near-limit sessions hand off to a fresh agent; the machinery lives in
  // lib/stores/handoff. The scan runs inside this $effect so it re-fires as the
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
      await Promise.allSettled([pty.kill(id)]);
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
        args: await pty.resumeArgs({
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
  // room, the handoff flow above when it doesn't (lib/stores/usageResume).
  const automaticRecoveryHost = {
    sessions: () => sessions,
    isOptedOut: () => settings.prefs.autoResume === false,
    thresholdPercentage: () => effective.handoffPercentage,
    forceHandoff: session => autoHandoff.force(session)
  } satisfies Parameters<typeof createUsageResume>[0];
  const usageResume = createUsageResume(automaticRecoveryHost);
  $effect(() => usageResume.check());
  onDestroy(() => usageResume.dispose());

  // ── API-error auto-retry ───────────────────────────────────────────────────
  // A session stopped by a transient API error (overloaded, a 5xx, a dropped
  // connection) is nudged with "continue" every 30s, handing off through the
  // flow above when its context is too full to recover (lib/stores/apiErrorRetry).
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

  // The open side panel (lib/side-panels); `null` means none is open.
  let side = $state<Side | null>(Side.feed);
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
    // The chords themselves live in lib/window-shortcuts.
    const shortcut = windowShortcutFor({
      event: e,
      isDevelopment: import.meta.env.DEV
    });
    if (shortcut === null) {
      return;
    }

    e.preventDefault();

    if (shortcut === WindowShortcut.reload) {
      location.reload();
      return;
    }

    // Keeps the terminal from also receiving the chord.
    e.stopPropagation();

    if (shortcut === WindowShortcut.newWindow) {
      await windows.create({ mode: WindowMode.enum.empty });
      showToast("Opened a new window");
      return;
    }

    await windows.focusRelative(shortcut === WindowShortcut.previousWindow ? "previous" : "next");
  }}
/>

<!-- Font tokens bound declaratively; they cascade to every descendant. -->
<div style:--font-ui={effective.uiFamily} style:--font-monospace={effective.monospaceFamily} class="app-root">
  {#if phase === Phase.project}
    <ProjectPicker
      {agents}
      ondelete={relocator.remove}
      onmove={relocator.move}
      onopen={openProject}
      onrename={relocator.rename}
      onrenamefolder={relocator.renameFolder}
    />
  {:else if phase === Phase.onboarding}
    <Onboarding
      {agents}
      onpick={agent => launch({
        agent,
        initialPrompt: pendingPrompt
      })}
      onswitchproject={switchToPicker}
      path={currentProject}
    />
  {:else if phase === Phase.ready}
    <div class="shell">
      <header class="topbar">
        <!-- Chrome row: project menu on the left, actions pushed right. Session
             tabs live on their own full-width row below so they get the whole
             width and only spill to "+N" on genuine overflow. -->
        <div class="chrome">
          <AppMenu
            {isTemp}
            label={projectLabel}
            labels={settings.labels}
            onaddroot={async rootPath => await workspace.addRoot({
              path: rootPath,
              create: false
            })}
            onclearrecent={clearRecentProjects}
            ondelete={deleteDirectory}
            onopen={projectPath => openProject({ path: projectPath })}
            onremoverecent={removeRecentProject}
            onrenamefolder={relocator.renameFolder}
            onreorderpins={reorderPins}
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
            onswitch={leaveToPicker}
            ontogglepin={toggleProjectPin}
            path={currentProject}
            pinnedProjects={settings.pinnedProjects}
            recentProjects={settings.recentProjects}
            roots={settings.roots}
          />
          <!-- Only rendered when the workspace has more than one part; a plain
               single-repo project keeps the top bar exactly as it was. -->
          <MemberSwitcher root={currentProject} />
          <!-- The branch HEAD is on, and the menu that moves it: every local
               branch plus the ones only the remote has yet. Reports on the same
               member the branch list was read for. -->
          <BranchSwitcher
            {branches}
            current={currentBranch}
            cwd={activeMemberIn(currentProject)}
            {hasRemote}
            onopengit={() => (side = Side.vcs)}
            onopenremote={async () => {
              await openRepositoryTarget({
                project: currentProject,
                branch: currentBranch
              });
            }}
            onrefresh={async () => {
              await Promise.all([loadBranches(), refreshMemberBranches()]);
            }}
            {remoteBranches}
          />
          <span class="chrome-spacer"></span>

          <UsageMeter {sessions} />
          <DesignMenu agent={activeAgent} />
          <!-- Open a console editor (Neovim/Vim/Helix) in its own terminal tab,
               split beside the agent so you can watch and edit at once. GUI editors
               go through the OS (ide.open); these need a real TTY, which only a PADE
               terminal gives. -->
          <!-- The editor is resolved for the *project* (the shared editors-store
               key the Change Feed also reads - SSOT), while the launcher opens
               the active session's worktree when one is focused. -->
          <IdeMenu
            cwd={activeSession?.cwd ?? activeMemberIn(currentProject)}
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
          />

          <div class="segmented-control" aria-label="Side panels" role="tablist">
            {#each SIDE_PANEL_TABS as tab (tab.id)}
              <button
                aria-selected={side === tab.id}
                data-tooltip={tab.label}
                onclick={() => (side = side === tab.id ? null : tab.id)}
                role="tab"
              ><Icon name={tab.icon} /> <span>{tab.short}</span></button>
            {/each}
          </div>
        </div>

        <SessionTabs
          bind:this={sessionTabsHandle}
          {activeId}
          {agents}
          {branches}
          onclose={close}
          ondraghint={hint => (dragHint = hint)}
          onlaunch={agent => launch({ agent })}
          onlaunchbranch={async branch => {
            // Spawn an agent on its own git worktree for `branch`, isolated from
            // the other sessions. Uses the active session's agent (or the first).
            const agent = activeSession?.agent ?? bestAgent;
            const cwd = await vcs.worktreeAdd({
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
            animatePaneIn(drop.id);
          }}
          {paneIds}
          popPaneActive={isPaneDragOverTabs}
          {sessions}
        />
      </header>

      <main
        style:--side-panel-width="{sidePanelWidth}px"
        style:--side-panel-min-width="{SIDE_PANEL_MIN_WIDTH}px"
        style:--side-panel-max-fraction="{SIDE_PANEL_MAX_FRACTION * 100}%"
        class="body"
        class:with-side={side !== null}
        data-resizing={isResizingSidePanel || undefined}
      >
        <section bind:this={elPanes} class="pane terminal-pane" data-panes>
          <!-- A tab dragged over the panes reads as "open in split" - a dashed
               primary frame invites the drop; each pane then shows the left/right
               half the pointer is over (below). -->
          {#if isDragOverPanes}
            <div class="drop-overlay">
              <span class="drop-badge"><Icon name="columns" /> Drop to open in split view</span>
            </div>
          {/if}
          {#if orderedSessions.length > 0}
            <!-- xterm is the heaviest frontend dependency. Load it only when
                 the first live session needs a pane, not while the picker/onboarding
                 screen is starting. The keyed loop still owns the mounted terminals
                 after that, preserving their PTY attachment and scrollback. -->
            {#await import("@/panels/Terminal.svelte") then { default: Terminal }}
              {#each orderedSessions as session (session.id)}
                <div
                  class="terminal-slot"
                  class:shown={paneIds.includes(session.id)}
                  data-pane-id={session.id}
                  onfocusin={() => {
                    // Focusing a split pane makes it the active one, so activeId
                    // always names the last agent the user touched - the pane a
                    // window-refocus (Ctrl+Alt+[ / ]) then hands the keyboard back to.
                    if (paneIds.includes(session.id)) {
                      activeId = session.id;
                    }
                  }}
                  onoutroend={() => collapsingSplitPanes.delete(session.id)}
                  out:collapsePane={{ duration: collapsingSplitPanes.has(session.id) ? SPLIT_PANE_COLLAPSE_MS : 0 }}
                >
                  {#if dropSideFor(session.id) === DropSide.left}
                    <div class="drop-half left"></div>
                  {:else if dropSideFor(session.id) === DropSide.right}
                    <div class="drop-half right"></div>
                  {/if}
                  <Terminal
                    isActive={session.id === activeId && paneIds.includes(session.id)}
                    isRemovable={isSplit && paneIds.includes(session.id)}
                    isShown={paneIds.includes(session.id)}
                    ondraghint={hint => (isPaneDragOverTabs = hint?.outside === true)}
                    onexit={handleSessionExit}
                    onpopout={() => {
                      // The pane header was dragged onto the tab strip: pop it out
                      // of the split into its own tab, shown fullscreen and active
                      // (the mirror of dragging a tab down onto the panes). Unlike
                      // the pane's × it collapses the split to this session.
                      if (isSplit) {
                        selectSession(session.id);
                      }
                    }}
                    onremove={() => removePane(session.id)}
                    onreorder={ids => (paneIds = ids)}
                    {session}
                  />
                </div>
              {/each}
            {/await}
          {/if}

          <div class="add-pane-wrap menu-host">
            <button
              style:anchor-name="--pane-anchor"
              class="add-pane menu-trigger"
              aria-label="Split - add an agent instance"
              data-tooltip="Split - add an agent instance"
              popovertarget="pane-menu"
            ><Icon name="columns" /></button>
            <ul id="pane-menu" style:position-anchor="--pane-anchor" class="pane-menu popover-menu" popover>
              {#if splitCandidates.length > 0}
                <li class="menu-separator">Add to split</li>
                {#each splitCandidates as session (session.id)}
                  <li>
                    <button
                      class="menu-item"
                      onclick={() => addPane(session.id)}
                      popovertarget="pane-menu"
                      popovertargetaction="hide"
                    >
                      <Icon name="terminal" /> {session.agent.label}
                    </button>
                  </li>
                {/each}
              {/if}
              <li class="menu-separator">Launch a new instance</li>
              {#each agents as agent (agent.id)}
                <li>
                  <button
                    class="menu-item"
                    onclick={() => launch({
                      agent,
                      split: true
                    })}
                    popovertarget="pane-menu"
                    popovertargetaction="hide"
                  >{agent.label}</button>
                </li>
              {/each}
            </ul>
          </div>
        </section>

        {#if side !== null}
          <SidePanelResizer bind:width={sidePanelWidth} bind:isResizing={isResizingSidePanel} />
          <SidePanel agent={activeAgent} agents={realAgents} project={currentProject} {side} />
        {/if}
      </main>

      <RunnerDock activeSessionId={activeId} />

      {#if autoHandoff.note || usageResume.note || apiErrorRetry.note}
        <output class="handoff-note">
          <span class="handoff-dot"></span>
          {autoHandoff.note || usageResume.note || apiErrorRetry.note}
        </output>
      {/if}

      {#if toastText()}
        <!-- <output> already carries role=status - a transient bottom pill that
             auto-dismisses via the showToast timer. -->
        <output class="toast">
          <span class="toast-icon"><Icon name="external" /></span>
          {toastText()}
        </output>
      {/if}

      <SendSelectionButton {activeId} />
    </div>
  {:else}
    <!-- Loading phase: a calm branded ground so a booting window is never a
         blank void (also the safety net if a boot step stalls). -->
    <div class="booting">
      <span class="brand" aria-label="PADE is starting"><Logo size={64} /></span>
    </div>
  {/if}

  <!-- One top-level bubble for every truncation tooltip - rendered here so it
       escapes any panel/popover clipping or scroll-width inflation. -->
  <OverflowTooltipLayer />
</div>

<style>
  .app-root {
    block-size: 100%;
  }

  /* Booting ground - a centered, gently pulsing brand mark on the app surface
     so a loading window reads as PADE, never a blank white void. */
  .booting {
    display: grid;
    place-items: center;
    block-size: 100%;
    background: radial-gradient(120% 120% at 50% 0%, var(--surface-1), var(--surface));

    .brand {
      display: inline-flex;
      animation: pulse 1400ms var(--ease) infinite;
    }
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

  /* First row: project menu + usage/design/IDE/panel actions. Its own labels
     fold to icons when it gets tight, independent of the tabs row below. */
  .chrome {
    display: flex;
    gap: clamp(8px, 1vw, 12px);
    align-items: center;
    min-inline-size: 0;
  }

  /* Pushes the action cluster to the right edge of the chrome row. */
  .chrome-spacer {
    flex: 1 1 0;
    min-inline-size: 0;
  }

  .segmented-control {
    display: inline-flex;
    flex-shrink: 0;
    gap: 2px;
    padding: 3px;
    border-radius: var(--radius-full);
    background: var(--surface-2);

    button {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      padding: 6px 12px;
      border: none;
      border-radius: var(--radius-full);
      background: transparent;
      color: var(--on-surface-variant);
      font: inherit;
      font-weight: 600;
      font-size: 12px;
      cursor: pointer;
      transition: background 200ms var(--ease), color 200ms var(--ease);
    }

    /* Selected tab - matched by state, not by qualifying the button type. */
    [aria-selected="true"] {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
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

  .pane {
    overflow: hidden;
    min-block-size: 0;
    min-inline-size: 0;
  }

  /* All sessions stay mounted so their scrollback survives switching; only the
     sessions in the current split are laid out (side by side). A hidden slot is
     NOT `display:none` - that would zero its ResizeObserver and shrink the
     background PTY to a 2×1 grid, which some agents (Codex) exit on. Instead it
     is lifted out of flow over the whole pane and turned invisible, so it keeps
     a real, full-pane layout size: the background PTY stays sized to exactly what
     its tab will show, and switching tabs needs no refit at all. */
  .terminal-pane {
    position: relative;
    display: flex;
    align-items: stretch;
  }

  .terminal-slot {
    position: relative;
    display: flex;
    flex: 1;
    flex-direction: column;
    min-block-size: 0;
    min-inline-size: 0;
    border-inline-end: 1px solid var(--outline);

    /* A pane appearing is deliberately instant - opening, closing, or switching a
       tab must not animate the terminal (it would read as a needless wipe). Only a
       genuine split-ADD springs its newcomer in, fired imperatively on that one
      slot via animatePaneIn (pane-enter), never here on every `.shown`. */
    &:not(.shown) {
      position: absolute;
      inset: 0;
      visibility: hidden;
      pointer-events: none;
    }
  }

  /* Dashed primary frame over the whole panes area while a tab is dragged onto
     it - the invitation to open the session as a split. */
  .drop-overlay {
    position: absolute;
    inset: 8px;
    z-index: 60;
    display: grid;
    place-items: center;
    border: 2px dashed var(--primary);
    border-radius: var(--radius-medium);
    background: color-mix(in oklab, var(--primary) 8%, transparent);
    pointer-events: none;
    animation: panel-swap 160ms var(--ease);

    .drop-badge {
      display: inline-flex;
      gap: 8px;
      align-items: center;
      padding-block: 6px;
      padding-inline: 14px;
      border-radius: var(--radius-full);
      background: var(--primary-container);
      color: var(--on-primary-container);
      font-weight: 700;
      font-size: 13px;
      box-shadow: 0 6px 20px var(--shadow-color);
    }
  }

  /* The half of a pane the pointer is over - where the dropped session will land
     (left = before this pane, right = after). A primary wash plus a solid edge. */
  .drop-half {
    position: absolute;
    inset-block: 0;
    z-index: 58;
    inline-size: 50%;
    background: color-mix(in oklab, var(--primary) 16%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--primary) 40%, transparent);
    pointer-events: none;

    &.left {
      inset-inline-start: 0;
      border-inline-start: 3px solid var(--primary);
    }

    &.right {
      inset-inline-end: 0;
      border-inline-end: 3px solid var(--primary);
    }
  }

  /* Thin strip at the right of the terminal row that opens the split menu. */
  .add-pane-wrap {
    display: flex;
    flex: none;
    align-items: stretch;
  }

  .add-pane {
    display: inline-flex;
    justify-content: center;
    align-items: flex-start;
    inline-size: 44px;
    padding-block-start: 12px;
    border: none;
    border-inline-start: 1px solid var(--outline);
    background: var(--surface-1);
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: color 150ms var(--ease), background 150ms var(--ease);

    &:hover {
      background: var(--surface-2);
      color: var(--on-surface);
    }
  }

  /* Shell and rows come from the shared .popover-menu and .menu-item; the split
     menu opens to the left since its trigger sits at the right edge. */
  .pane-menu {
    min-inline-size: 220px;
    position-area: bottom span-left;
  }

  @media (width <= 720px) {
    .body.with-side {
      grid-template-rows: 1fr 40%;
      grid-template-columns: 1fr;
    }
  }

  /* Auto-handoff banner - a calm status pill while a session hands off. */
  .handoff-note {
    position: fixed;
    inset-block-start: 60px;
    inset-inline-start: 50%;
    z-index: 80;
    display: inline-flex;
    gap: 9px;
    align-items: center;
    max-inline-size: min(560px, 90vw);
    padding: 10px 18px;
    border-radius: var(--radius-full);
    background: var(--primary-container);
    color: var(--on-primary-container);
    font-weight: 600;
    font-size: 13px;
    box-shadow: 0 10px 30px var(--shadow-color);
    transform: translateX(-50%);
    animation: pop-in 220ms var(--ease);

    .handoff-dot {
      flex: none;
      block-size: 8px;
      inline-size: 8px;
      border-radius: var(--radius-full);
      background: var(--primary);
      animation: pulse 1100ms var(--ease) infinite;
    }
  }

  /* Transient status toast - sits just above the send FAB, auto-dismissed. */
  .toast {
    position: fixed;
    inset-block-end: 72px;
    inset-inline-start: 50%;
    z-index: 85;
    display: inline-flex;
    gap: 9px;
    align-items: center;
    padding-block: 10px;
    padding-inline: 18px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-full);
    background: var(--surface-3);
    color: var(--on-surface);
    font-weight: 600;
    font-size: 13px;
    box-shadow: 0 12px 34px var(--shadow-color);
    transform: translateX(-50%);
    animation: pop-in 240ms var(--spring);

    .toast-icon {
      display: inline-flex;
      color: var(--primary);
    }
  }
</style>
