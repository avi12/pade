<script lang="ts">
  import AppMenu from "@/lib/AppMenu.svelte";
  import { createAutoNamer } from "@/lib/auto-name";
  import {
    agents as agentsApi,
    feed,
    ide,
    pty,
    vcs,
    windows,
    workspace
  } from "@/lib/bridge";
  import DesignMenu from "@/lib/DesignMenu.svelte";
  import { updateDiscordPresence } from "@/lib/discord-presence";
  import type { DragHint } from "@/lib/drag-reorder";
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import IdeMenu from "@/lib/IdeMenu.svelte";
  import Logo from "@/lib/Logo.svelte";
  import { collapsePane } from "@/lib/motion";
  import { registerPaneShortcuts } from "@/lib/pane-shortcuts";
  import { isTemporaryWorkspace, normalizePath } from "@/lib/paths";
  import { appearance, effective } from "@/lib/prefs.svelte";
  import { DropSide, paneDropSide, paneInsertIndex } from "@/lib/reorder";
  import RunnerDock from "@/lib/RunnerDock.svelte";
  import { registerSendShortcut, unregisterSendShortcut } from "@/lib/send-shortcut";
  import { clearSessionSnapshot, restoreLiveSnapshot, saveSessionSnapshot } from "@/lib/session-restore";
  import SessionTabs from "@/lib/SessionTabs.svelte";
  import { createApiErrorRetry, dropApiError } from "@/lib/stores/apiErrorRetry.svelte";
  import { createAutoHandoff } from "@/lib/stores/handoff.svelte";
  import { ensureRunnerListeners, startRunner } from "@/lib/stores/runners.svelte";
  import {
    dropChoiceAttention,
    ensureChoiceAttention,
    reconcileChoiceAttention
  } from "@/lib/stores/sessionAttention.svelte";
  import { dropSessionLabel } from "@/lib/stores/sessionLabels.svelte";
  import { dropNaming } from "@/lib/stores/sessionNaming.svelte";
  import { dropSessionStatus, isSessionIdle, whenSessionIdle } from "@/lib/stores/sessions.svelte";
  import { panelCount, panelRefresh } from "@/lib/stores/sidePanel.svelte";
  import { initTaskRunDetection } from "@/lib/stores/taskRuns.svelte";
  import { showToast, toastText } from "@/lib/stores/toast.svelte";
  import { createUsageResume, dropUsageLimit } from "@/lib/stores/usageResume.svelte";
  import { registerTabShortcuts } from "@/lib/tab-shortcuts";
  import { SHELL_AGENT_ID, StartMode } from "@/lib/types";
  import type {
    Agent,
    AgentSession,
    Ide,
    OpenTarget,
    Settings,
    TaskGroup
  } from "@/lib/types";
  import UsageMeter from "@/lib/UsageMeter.svelte";
  import { FolderPath, parseInput } from "@/lib/validate";
  import { createRelocator } from "@/lib/workspace-relocate";
  import ChangeFeed from "@/panels/ChangeFeed.svelte";
  import Onboarding from "@/panels/Onboarding.svelte";
  import ProjectPicker from "@/panels/ProjectPicker.svelte";
  import Terminal from "@/panels/Terminal.svelte";
  import { onDestroy, onMount, tick } from "svelte";
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
  let settings = $state<Settings>({
    roots: [],
    defaultAgent: null,
    projectAgents: {},
    recentProjects: [],
    pinnedProjects: [],
    ownedWorkspaces: [],
    labels: {},
    prefs: {}
  });
  let sessions = $state<AgentSession[]>([]);
  let activeId = $state<string | null>(null);
  // Which sessions are shown side by side in the terminal area. One id = the
  // classic single pane; more = a split view. Always a subset of `sessions`.
  let paneIds = $state<string[]>([]);
  let currentProject = $state<string>("");
  // Local branches when the project is a git repo — enables per-branch agents.
  let branches = $state<string[]>([]);
  // Carried through the agent picker so a new-project prompt survives onboarding.
  let pendingPrompt = $state<string | undefined>();

  // Agents excluding the always-present shell fallback — this count decides
  // whether we auto-launch or onboard.
  const realAgents = $derived(agents.filter(a => a.id !== SHELL_AGENT_ID));
  // The current directory, shown as the last couple of segments so it's legible
  // without eating the whole topbar (full path in the tooltip).
  const shortDir = $derived(
    currentProject.split(/[\\/]/).filter(Boolean).slice(-2).join("/")
  );
  // Temp workspaces live under the config dir as .../workspaces/temp-<stamp>.
  const isTemp = $derived(isTemporaryWorkspace(currentProject));
  // Friendly auto-derived name for the current workspace, if one was assigned.
  const currentLabel = $derived(settings.labels[currentProject]);
  // A temp workspace that never earned a name is still a throwaway: when its
  // last session ends, the window returns to the picker and the folder is
  // deleted (see discardTempWorkspace). One that was auto-named holds real
  // work, so it keeps the normal last-session behavior instead.
  const isDiscardableTemp = $derived(isTemp && !currentLabel);
  // Active agent id — used to show only its relevant config files.
  const activeAgent = $derived(sessions.find(s => s.id === activeId)?.agent.id ?? "");
  // A pane can be removed only while more than one is shown; sessions not
  // currently shown are offered in the "add to split" menu.
  const canRemovePane = $derived(paneIds.length > 1);
  const splitCandidates = $derived(sessions.filter(s => !paneIds.includes(s.id)));

  // The terminal slots render in this order: the shown panes first, in `paneIds`
  // order (so a pane-header drag that reorders `paneIds` reorders the view), then
  // the hidden sessions. Keyed by id, so a reorder moves DOM nodes rather than
  // remounting — every terminal keeps its scrollback.
  const bySessionId = $derived(new Map(sessions.map(s => [s.id, s] as const)));
  const orderedSessions = $derived.by(() => {
    const shown = paneIds
      .map(id => bySessionId.get(id))
      .filter((s): s is AgentSession => s !== undefined);
    const hidden = sessions.filter(s => !paneIds.includes(s.id));
    return [...shown, ...hidden];
  });

  // ── Tab drag → reorder / split ──────────────────────────────────────────────
  // `DropSide` (which half of a pane a dragged tab lands on) and the drop's index
  // math live in `@/lib/reorder` — DOM-free and unit-tested. (The `Side` above
  // names the side panels — a different concern, no bare literals shared.)

  // The panes container, so a drop's pointer can be hit-tested against the panes.
  let panesElement = $state<HTMLElement>();
  // Live drag state from the tab strip; drives the "drop here" overlay + halves.
  let dragHint = $state<DragHint | null>(null);
  const dragOverPanes = $derived(dragHint?.outside === true);

  // True while a split pane's header is dragged up over the tab strip — the mirror
  // gesture: the strip lights as a "drop → new tab" target and the drop pops the
  // pane out of the split (see the Terminal panes' `onremove`). Reported by the
  // dragged pane's Terminal via the same engine hint the tab-split path uses.
  let paneDragOverTabs = $state(false);

  // Which pane + side the drag is currently over — the highlighted drop half.
  const splitTarget = $derived.by(() => (dragOverPanes && dragHint
    ? paneDropAt({
      x: dragHint.pointerX,
      y: dragHint.pointerY
    })
    : null));
  function dropSideFor(id: string): DropSide | null {
    return splitTarget?.id === id ? splitTarget.side : null;
  }

  // The shown pane under a point, and which half of it — used both for the live
  // highlight and for the actual drop, so the two never disagree (DRY).
  function paneDropAt({ x, y }: {
    x: number;
    y: number;
  }): {
    id: string;
    side: DropSide;
  } | null {
    const container = panesElement;
    if (!container) {
      return null;
    }

    for (const slot of container.querySelectorAll<HTMLElement>("[data-pane-id]")) {
      const id = slot.getAttribute("data-pane-id");
      if (id === null || !paneIds.includes(id)) {
        continue;
      }

      const rect = slot.getBoundingClientRect();
      const isInside = rect.width > 0
        && x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
      if (isInside) {
        return {
          id,
          side: paneDropSide({
            pointerX: x,
            left: rect.left,
            width: rect.width
          })
        };
      }
    }

    return null;
  }

  // A tab dropped over the panes: show its session as a split pane, on the side of
  // the target pane the pointer landed on (repositioning it if already shown).
  function splitDrop(drop: {
    id: string;
    pointerX: number;
    pointerY: number;
  }) {
    const target = paneDropAt({
      x: drop.pointerX,
      y: drop.pointerY
    });
    if (!target) {
      return;
    }

    const base = paneIds.filter(id => id !== drop.id);
    const insertAt = paneInsertIndex({
      paneIds,
      draggedId: drop.id,
      targetId: target.id,
      side: target.side
    });

    paneIds = [...base.slice(0, insertAt), drop.id, ...base.slice(insertAt)];
    activeId = drop.id;
    void animatePaneIn(drop.id);
  }

  // A tab-strip drag committed a new order for the visible pills. They are a
  // prefix of `sessions` (the overflow dots/+N are the tail), so slot the reordered
  // ids back into the positions the visible set held and leave the tail put.
  function reorderSessions(orderedIds: string[]) {
    const inOrder = new Set(orderedIds);
    const queue = [...orderedIds];
    sessions = sessions.map(session => {
      if (!inOrder.has(session.id)) {
        return session;
      }

      const nextId = queue.shift();
      return (nextId ? bySessionId.get(nextId) : undefined) ?? session;
    });
  }

  // How a spawned window routes off its query string (window_create encodes the
  // target here). A closed set defined once so no bare literal leaks into boot.
  const WindowMode = {
    empty: "empty",
    temp: "temp",
    open: "open"
  } as const;
  type WindowMode = (typeof WindowMode)[keyof typeof WindowMode];

  // Agent detection runs a subprocess per agent, so cap how long the boot waits
  // on it — a stall must never freeze the splash. An empty list just routes to
  // onboarding, and the redetect interval fills it in shortly after.
  const BOOT_DETECT_TIMEOUT_MS = 6_000;

  onMount(async () => {
    try {
      // Detection is best-effort: a rejection or a stall both yield an empty
      // list rather than blocking the routing below.
      const detecting = Promise.race([
        agentsApi.detect().catch((): Agent[] => []),
        new Promise<Agent[]>(resolve => setTimeout(() => resolve([]), BOOT_DETECT_TIMEOUT_MS))
      ]);
      const [detected, saved] = await Promise.all([detecting, workspace.settings()]);
      agents = detected;
      settings = saved;

      // An accidental reload (F5, a crash recovery) re-attaches this window to
      // the agents still running in the backend — before any query routing,
      // whose `w=` went stale the moment the user moved on inside the window
      // (the incident: a `?w=empty` window living in a project rebooted to the
      // picker and orphaned its live PTYs, unreachable).
      const reattached = await reattachAfterReload();
      if (reattached) {
        return;
      }

      // A spawned window carries a `w=` query that overrides the normal cold-start.
      // The plain main window (no query) keeps today's launch_context behavior.
      const query = new URLSearchParams(location.search);
      const routed = await routeFromQuery(query);
      if (routed) {
        return;
      }

      const ctx = await workspace.context();
      const prefersPicker = saved.prefs.startMode === StartMode.enum.picker;
      if (ctx.hasProject) {
        await workspace.open(ctx.cwd); // records it in recent history
        startAgentFlow({ path: ctx.cwd });
        await loadBranches();
      } else if (prefersPicker) {
        // Opt-in: show the project picker instead of starting in a temp workspace.
        phase = Phase.project;
      } else {
        // Default: start immediately in a throwaway workspace so there's no
        // blocking picker. The user can switch any time (Switch button).
        const temp = await workspace.temp();
        startAgentFlow({ path: temp });
      }
    } catch {
      // Never strand the user on the splash — fall back to the project picker.
      showToast("Startup hit a snag — pick a project to continue.");
      phase = Phase.project;
    }
  });

  // Boot a spawned window from its `w=` query. Returns true when it handled the
  // launch (so the default launch_context path is skipped), false otherwise.
  async function routeFromQuery(query: URLSearchParams): Promise<boolean> {
    const mode = query.get("w");
    if (mode === WindowMode.temp) {
      const temp = await workspace.temp();
      startAgentFlow({ path: temp });
      return true;
    }

    if (mode === WindowMode.empty) {
      phase = Phase.project;
      return true;
    }

    if (mode === WindowMode.open) {
      // query.get("path") is a trust boundary — validate before opening.
      const path = parseInput({
        schema: FolderPath,
        raw: query.get("path")
      });
      if (path) {
        await openProject({ path });
        return true;
      }
    }

    return false;
  }

  // Re-attach the live sessions an accidental reload orphaned. The snapshot
  // (sessionStorage — survives a reload, dies with the window) says which panes
  // this window was showing; `pty_list` says which of them the backend still
  // hosts, and only that intersection is restored — a deliberate leave killed
  // its PTYs, so nothing survives it and boot proceeds normally. Each restored
  // pane re-attaches through Terminal's existing path: spawn is a no-op for a
  // running session and `pty_history` replays the conversation.
  async function reattachAfterReload(): Promise<boolean> {
    const snapshot = await restoreLiveSnapshot();
    if (!snapshot) {
      return false;
    }

    try {
      await workspace.open(snapshot.project);
    } catch {
      // The project vanished while the window was reloading (deleted, moved,
      // unmounted). Boot falls through to the picker — so kill the snapshot's
      // still-live sessions rather than leave them running unreachably, which
      // would recreate the very invisible-agent incident restore exists to fix.
      await Promise.all(
        snapshot.sessions.map(orphan => pty.kill(orphan.id).catch(() => {}))
      );
      clearSessionSnapshot();
      return false;
    }

    currentProject = snapshot.project;
    void windows.registerProject(snapshot.project);
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
  // Never while still booting — the effect's first run lands before the restore
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
  // stale the moment the user moved on from what the window was spawned for —
  // a `?w=empty` picker window living inside a project rebooted to the picker
  // (the incident), a `?w=temp` one minted a second throwaway. Keep the query
  // telling the truth — the project on screen, else the picker — so even a
  // snapshot-less reload (all sessions had exited) lands on the right screen.
  $effect(() => {
    if (phase === Phase.loading) {
      return;
    }

    const query = currentProject === ""
      ? `?w=${WindowMode.empty}`
      : `?w=${WindowMode.open}&path=${encodeURIComponent(currentProject)}`;
    if (location.search !== query) {
      history.replaceState(null, "", query);
    }
  });

  // Re-detect installed agents so the picker reflects an agent the user just
  // installed or removed — when the app becomes visible again (they switched back
  // from installing one, see the `visibilitychange` below) and on a slow poll as a
  // fallback. Detection spawns a process per agent, so it must stay off the drag
  // path: page visibility never changes while dragging a window that stays on
  // screen, unlike window focus which a title-bar drag churns and lagged.
  async function redetectAgents() {
    agents = await agentsApi.detect();
  }
  onMount(() => {
    const interval = setInterval(() => void redetectAgents(), 30_000);
    return () => clearInterval(interval);
  });

  // Subscribe once to the backend task-runner stream so the dock updates live.
  onMount(() => void ensureRunnerListeners());

  // Watch the PTY stream once for the agent's multiple-choice prompts, so a tab
  // can flash red when one is pending on it (lib/stores/sessionAttention).
  onMount(() => void ensureChoiceAttention());

  // Reflect known tasks the agent runs as "running" in the Tasks panel.
  onMount(() => void initTaskRunDetection());

  // Watch the open project's files app-wide — not only while the Change Feed is
  // open — so the Tasks panel auto-updates on a manifest/script edit, and the
  // task-run and auto-name subscribers see changes from the moment a project
  // opens. Keyed on the project so it re-roots on an in-window switch; the
  // backend is idempotent, so a repeat call for the same root is a no-op.
  $effect(() => {
    if (currentProject) {
      void feed.start(currentProject);
    }
  });

  // Keep every installed agent's own theme config in step with ADE's scheme —
  // re-forced on project open and on every scheme flip. The terminal protocol
  // can't carry it (ConPTY eats the OSC 11 query and the ?997 report), but
  // Claude re-reads .claude/settings.local.json live, so a flip re-themes even
  // a running session. See theming.rs.
  $effect(() => {
    if (currentProject) {
      void agentsApi.syncTheme({
        workspace: currentProject,
        scheme: appearance.scheme
      });
    }
  });

  // ── Discord Rich Presence ────────────────────────────────────────────────────
  // Broadcast "Playing PADE" (opt-in), optionally naming the open project. The
  // state→bridge mapping lives in lib/discord-presence (SoC). Reads settings.prefs
  // (refreshed on boot and every project open), the same source the other
  // picker-managed prefs use here, so a toggle in the picker takes effect the moment
  // a project opens. The two flags are $derived so the effect only re-sends when
  // presence, the project-name toggle, or the open project actually change — not on
  // every unrelated settings reassignment (a pin, an editor rule).
  const discordEnabled = $derived(settings.prefs.discordPresence === true);
  const discordShowProject = $derived(settings.prefs.discordShowProject !== false);
  // The open project's language, shown VS-Code-style as a small overlay icon +
  // status line. Fetched only while presence will actually show it, and refreshed
  // on a project switch.
  let discordProjectKind = $state<string | undefined>(undefined);
  async function loadDiscordKind(project: string): Promise<void> {
    try {
      const kinds = await ide.projectKinds([project]);
      discordProjectKind = kinds[project];
    } catch {
      discordProjectKind = undefined;
    }
  }
  $effect(() => {
    if (discordEnabled && discordShowProject && currentProject) {
      void loadDiscordKind(currentProject);
    } else {
      discordProjectKind = undefined;
    }
  });
  $effect(() => {
    void updateDiscordPresence({
      enabled: discordEnabled,
      showProject: discordShowProject,
      project: currentProject,
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
    activeAgentCommand: () => sessions.find(s => s.id === activeId)?.agent.command ?? "",
    applySettings(next) {
      settings = next;
    }
  });
  onMount(() => {
    void autoNamer.start();
    return () => autoNamer.dispose();
  });

  // Send-from-IDE bridge (lib/send-shortcut): copy in any external editor, press
  // the global shortcut, and the clipboard lands in the active agent's input.
  onMount(() => {
    void registerSendShortcut({
      activeId: () => activeId,
      activeLabel: () => sessions.find(s => s.id === activeId)?.agent.label ?? "agent"
    });
    return () => void unregisterSendShortcut();
  });

  // Tab shortcuts (lib/tab-shortcuts): capture-phase so they win over a focused
  // agent terminal — new tab, launch menu, close, and next/previous cycling.
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
  // nth, and Ctrl+Alt+W closes the active pane's session — the slot then animates
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

  async function openEmptyWindow() {
    await windows.create({ mode: WindowMode.empty });
    showToast("Opened a new window");
  }

  // Closing PADE (the title-bar X) is a deliberate leave too: intercept the
  // close, let every agent reach an idle prompt, kill them, and only then let
  // the window go (Tauri destroys it once the handler settles unprevented).
  // While a leave is already in flight the gate skips the wait, so a second
  // X-click closes immediately — graceful, never a trap; the backend's
  // exit-time kill_all reaps whatever that force-close leaves behind.
  let unlistenCloseRequested: (() => void) | undefined;
  async function interceptWindowClose() {
    unlistenCloseRequested = await windows.onCloseRequested(async () => {
      await runExclusiveLeave(closeAllSessionsGracefully);
    });
  }
  onMount(() => {
    void interceptWindowClose();
    return () => unlistenCloseRequested?.();
  });

  // One deliberate leave at a time. The graceful wait can hold the UI open for
  // up to GRACEFUL_LEAVE_TIMEOUT_MS while it stays fully interactive, so a
  // second switch/leave starting inside that window would interleave two
  // kill → open → launch sequences over the same session list. Later attempts
  // are dropped, not queued — the first leave is already taking the user away.
  let leaveInFlight = false;
  async function runExclusiveLeave(leave: () => Promise<void>): Promise<void> {
    if (leaveInFlight) {
      return;
    }

    leaveInFlight = true;
    try {
      await leave();
    } finally {
      leaveInFlight = false;
    }
  }

  async function openProject(target: OpenTarget) {
    // If another window already has this project open, focus it instead of
    // opening a second copy here — the picker window stays put.
    if (await windows.focusProject(target.path)) {
      return;
    }

    // Picking the project this window already has open is a return, not a
    // reopen — the live sessions stay exactly as they were.
    const isReturnToCurrent =
      normalizePath(target.path) === normalizePath(currentProject) && sessions.length > 0;
    if (isReturnToCurrent) {
      phase = Phase.ready;
      return;
    }

    // Switching this window to another project: the old project's agents don't
    // come along. Kill them before entering, so no session keeps running — and
    // cwd-locking — a workspace this window has left. Leaving a never-named
    // temp workspace behind also discards it, exactly like ending its last
    // session would (see discardTempWorkspace).
    await runExclusiveLeave(async () => {
      const previousProject = currentProject;
      const leavesDiscardableTemp = isDiscardableTemp;
      await closeAllSessionsGracefully();

      await workspace.open(target.path);
      settings = await workspace.settings(); // pick up the updated recent history
      startAgentFlow({
        path: target.path,
        initialPrompt: target.initialPrompt,
        agentId: target.agent
      });
      await loadBranches();

      if (leavesDiscardableTemp) {
        try {
          settings = await workspace.delete(previousProject);
        } catch {
          showToast("Couldn't delete the temp workspace folder.");
        }
      }
    });
  }

  // Decide how to enter a project: honor a saved per-project/default agent,
  // else launch the best installed agent outright — opening a project always
  // lands in the workspace, never on a blocking chooser. The agent picker
  // still appears after a hand-closed or exited last session, where "what
  // next?" is a genuine question. (Reused for every entry path.)
  function startAgentFlow({ path, initialPrompt, agentId }: {
    path: string;
    initialPrompt?: string;
    agentId?: string;
  }) {
    currentProject = path;
    // Let other windows' pickers focus this one instead of reopening the project.
    void windows.registerProject(path);
    // A create-form agent pick wins outright; otherwise honor the per-project
    // override, then the workspace default.
    const prefId = agentId ?? settings.projectAgents[path] ?? settings.defaultAgent ?? null;
    const preferred = prefId ? agents.find(a => a.id === prefId) : undefined;
    // Detection order is the registry's priority order, so the first real
    // agent is the best installed one; the shell fallback carries otherwise.
    launch({
      agent: preferred ?? realAgents[0] ?? agents[0],
      initialPrompt
    });
  }

  // A self-exit within RESPAWN_MIN_LIFETIME_MS of launch reads as a failed start,
  // so we don't respawn-loop on it.
  const sessionLaunchedAt = new SvelteMap<string, number>();
  const RESPAWN_MIN_LIFETIME_MS = 2_000;
  // A hand-close also kills the PTY, firing the exit event; the exit handler skips
  // these so the two teardown paths don't race.
  const closingByHand = new SvelteSet<string>();

  // A pane that was one of several in a live split when its session detached
  // collapses its width so the survivors glide across to fill (out:collapsePane).
  // Every other removal — a sole-pane tab close above all — swaps instantly: with
  // nothing beside it, a collapsing pane would only crush the surviving terminal
  // to near-zero width and let it reflow back out, a needless squeeze. So just
  // genuine split members are recorded here (and dropped again on `outroend`);
  // the rest get a zero-duration outro.
  const collapsingSplitPanes = new SvelteSet<string>();

  function launch(opts: {
    agent: Agent;
    initialPrompt?: string;
    cwd?: string;
    branch?: string;
    /** Extra command args — the project path when running a terminal editor. */
    args?: string[];
    /** Add alongside the current panes (split) instead of replacing them. */
    split?: boolean;
  }): string {
    const session: AgentSession = {
      id: crypto.randomUUID(),
      agent: opts.agent,
      initialPrompt: opts.initialPrompt,
      cwd: opts.cwd,
      branch: opts.branch,
      args: opts.args
    };
    sessions.push(session);
    sessionLaunchedAt.set(session.id, Date.now());
    activeId = session.id;
    paneIds = opts.split ? [...paneIds, session.id] : [session.id];

    if (opts.split) {
      void animatePaneIn(session.id);
    }

    pendingPrompt = undefined;
    phase = Phase.ready;
    return session.id;
  }

  // A tab click shows that session as the sole pane (classic single view).
  // Collapsing a split is deliberately instant (as the design is): a View
  // Transition here would scale the surviving pane's snapshot as it grows to
  // fill, ghosting the old half-width terminal text over the reflowed full-width
  // grid — the platform morph can't track a terminal that repaints on resize.
  function selectSession(id: string) {
    activeId = id;
    paneIds = [id];
  }

  // A split pane's header was dragged onto the tab strip: pop that pane out of
  // the split into its own tab — the split collapses to it and it shows
  // fullscreen as the active tab (the mirror of dragging a tab down into the
  // panes to split it). Distinct from the pane's × button below, which only
  // drops the pane from the split and keeps a remaining pane active.
  function popPaneToTab(id: string) {
    if (paneIds.length <= 1) {
      return;
    }

    selectSession(id);
  }

  // Spring just the added pane in from its trailing edge (pane-enter). Fired on
  // the one slot that joined a split — never on a plain tab open / close / switch,
  // which stay instant — so it can't run on every show the way a CSS rule on
  // `.shown` would (that wiped the terminal in on any tab change). Clears the
  // inline animation once it settles so a later show/hide of the same slot can't
  // replay it. Mirrors the design's imperative `animatePaneIn`.
  async function animatePaneIn(id: string) {
    await tick();
    const slot = panesElement?.querySelector<HTMLElement>(`[data-pane-id="${id}"]`);
    if (!slot) {
      return;
    }

    slot.style.animation = "none";
    void slot.offsetWidth; // reflow so re-adding the same pane restarts the run
    slot.style.animation = "pane-enter 340ms var(--spring)";
    slot.addEventListener("animationend", () => (slot.style.animation = ""), { once: true });
  }

  // Show an already-running session alongside the current pane(s) — a split-add,
  // so the newcomer springs in (animatePaneIn).
  function addPane(id: string) {
    if (!paneIds.includes(id)) {
      paneIds = [...paneIds, id];
      void animatePaneIn(id);
    }

    activeId = id;
  }

  // Drop a pane from the split — never the last visible one.
  function removePane(id: string) {
    if (!canRemovePane) {
      return;
    }

    paneIds = paneIds.filter(paneId => paneId !== id);

    if (activeId === id) {
      activeId = paneIds.at(-1) ?? null;
    }
  }

  // ── Tab keyboard shortcuts (lib/tab-shortcuts) ───────────────────────────────
  // Actions the shortcut layer drives; each is a no-op outside the ready phase.

  // Ctrl+T — another tab of the last session's kind (mirrors the "+" button).
  function newTab() {
    if (phase !== Phase.ready) {
      return;
    }

    const agent = sessions.at(-1)?.agent ?? realAgents[0] ?? agents[0];
    if (agent) {
      launch({ agent });
    }
  }

  // Ctrl+Shift+T — open the launch dropdown with the first agent focused, so
  // Enter fires it and Esc light-dismisses (native popover handles both).
  function openLaunchMenu() {
    if (phase !== Phase.ready) {
      return;
    }

    const menu = document.getElementById("add-menu");
    if (!(menu instanceof HTMLElement) || menu.matches(":popover-open")) {
      return;
    }

    menu.showPopover();
    menu.querySelector<HTMLButtonElement>("button")?.focus();
  }

  // Ctrl+W / Ctrl+F4 — close the active session.
  function closeActiveTab() {
    const active = sessions.find(s => s.id === activeId);
    if (active) {
      void close(active);
    }
  }

  // Ctrl+number — jump straight to a tab by position (Ctrl+9 = the last one).
  // The matcher only ever hands back an index that exists, so the guard is
  // belt-and-suspenders; selection reuses the same make-active path as a click.
  function selectTabByIndex(index: number) {
    const session = sessions[index];
    if (session) {
      selectSession(session.id);
    }
  }

  // Ctrl+Tab / Alt+Arrow — cycle the active tab, wrapping at the ends.
  function stepSession(delta: number) {
    if (sessions.length === 0) {
      return;
    }

    const index = sessions.findIndex(s => s.id === activeId);
    const nextIndex = (index + delta + sessions.length) % sessions.length;
    selectSession(sessions[nextIndex].id);
  }

  // Load branches for the current repo (empty when not a git project).
  async function loadBranches() {
    branches = await vcs.branches().catch(() => []);
  }

  // The caller kills the PTY and decides the empty-workspace policy.
  function detachSession(id: string) {
    if (paneIds.length > 1 && paneIds.includes(id)) {
      collapsingSplitPanes.add(id);
    }

    sessions = sessions.filter(s => s.id !== id);
    paneIds = paneIds.filter(paneId => paneId !== id);
    sessionLaunchedAt.delete(id);
    dropSessionStatus(id);
    dropSessionLabel(id);
    dropNaming(id);
    dropUsageLimit(id);
    dropApiError(id);
    dropChoiceAttention(id);

    if (activeId === id) {
      activeId = paneIds.at(-1) ?? sessions.at(-1)?.id ?? null;
    }

    if (paneIds.length === 0 && activeId) {
      paneIds = [activeId];
    }
  }

  // How long a deliberate leave waits for a busy agent to reach an idle prompt
  // before killing it anyway — graceful, but never a trap behind a wedged agent.
  const GRACEFUL_LEAVE_TIMEOUT_MS = 30_000;

  // A DELIBERATE leave (switching project, going back to the picker) ends every
  // session — but gracefully: wait for each to reach an idle prompt first
  // (sessionStatus ready, the output-quiet signal — never child-process
  // counting, which mis-reads persistent MCP servers) so nothing mid-flight is
  // severed; the agent auto-saves, so /resume covers continuity. The wait runs
  // before any phase change, while the Terminals are still mounted and
  // publishing status. An accidental reload records no leave — its sessions
  // stay alive and the next boot re-attaches them (session-restore).
  async function closeAllSessionsGracefully() {
    const hasBusySession = sessions.some(s => !isSessionIdle(s.id));
    if (hasBusySession) {
      showToast("Waiting for the agent to finish before leaving…");
    }

    await Promise.all(
      sessions.map(s => whenSessionIdle({
        id: s.id,
        timeoutMs: GRACEFUL_LEAVE_TIMEOUT_MS
      }))
    );
    await closeAllSessions();
  }

  // Tear down every session at once — the project-switch path. Each PTY is
  // killed (reaping its child, so the old workspace's cwd lock is released)
  // and its exit event is claimed as a hand-close so the exit handler never
  // races this with a respawn or a discard.
  async function closeAllSessions() {
    const ids = sessions.map(s => s.id);
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
    const wasLastSession = sessions.length === 1;
    detachSession(session.id);
    closingByHand.delete(session.id);

    // Hand-closing the last tab returns to the picker — never a silent respawn
    // (a self-exit does respawn; see handleSessionExit). A still-unnamed temp
    // workspace has nothing worth keeping, so it is discarded outright.
    if (wasLastSession) {
      if (isDiscardableTemp) {
        await discardTempWorkspace();
        return;
      }

      pendingPrompt = undefined;
      phase = Phase.onboarding;
    }
  }

  // The PTY exited on its own — e.g. the user pressed Ctrl-C to quit the agent.
  async function handleSessionExit(id: string) {
    const isClosingByHand = closingByHand.has(id);
    if (isClosingByHand) {
      return;
    }

    const session = sessions.find(s => s.id === id);
    if (!session) {
      return;
    }

    const { agent } = session;
    const wasLastSession = sessions.length === 1;
    const startedAt = sessionLaunchedAt.get(id) ?? 0;
    const failedToStart = Date.now() - startedAt <= RESPAWN_MIN_LIFETIME_MS;
    detachSession(id);
    await pty.kill(id).catch(() => {}); // reap the backend record; the child is already gone

    if (!wasLastSession) {
      return;
    }

    // The agent quitting in a still-unnamed temp workspace ends the throwaway
    // session: no respawn, no agent picker — back to the project picker.
    if (isDiscardableTemp) {
      await discardTempWorkspace();
      return;
    }

    const shouldRespawn = !failedToStart;
    if (shouldRespawn) {
      launch({ agent });
    } else {
      pendingPrompt = undefined;
      phase = Phase.onboarding;
    }
  }

  // Clear the recent-projects history from the switcher (pins survive).
  async function clearRecentProjects() {
    settings = await workspace.clearRecent();
  }
  // Pin/unpin a project from the switcher; the parent stays the settings owner.
  async function toggleProjectPin(target: {
    path: string;
    pinned: boolean;
  }) {
    settings = await workspace.setPinned(target);
  }
  // Forget a project from the switcher's lists (folder untouched).
  async function removeRecentProject(projectPath: string) {
    settings = await workspace.removeRecent(projectPath);
  }
  // Persist a drag-reordered pin order from the switcher.
  async function reorderPins(paths: string[]) {
    settings = await workspace.setPinnedOrder(paths);
  }

  // "Delete directory" from the switcher — a destructive removal of a real
  // project's folder. The switcher owns the confirmation UI (so it stays open and
  // animates the row out); here we just release the folder (killing any session
  // holding it) and delete it, letting the refreshed settings flow back. Rejects
  // with its message so the switcher can surface it in the still-open prompt.
  async function deleteDirectory(projectPath: string) {
    settings = await relocator.removeDirectory(projectPath);
  }

  function switchToPicker() {
    document.startViewTransition(async () => {
      phase = Phase.project;
      await tick();
    });
  }

  // Hand the project back: this window no longer claims it, so another window
  // (or this one, later) can open it fresh — no picker tries to focus us here.
  function releaseProject() {
    currentProject = "";
    branches = [];
    pendingPrompt = undefined;
    void windows.registerProject("");
  }

  // "Switch project" — a DELIBERATE leave to the picker. The project's agents
  // don't idle on invisibly behind it: each is killed once it reaches an idle
  // prompt (closeAllSessionsGracefully), and a never-named temp workspace is
  // discarded exactly as ending its last session would.
  async function leaveToPicker() {
    await runExclusiveLeave(async () => {
      await closeAllSessionsGracefully();

      if (isDiscardableTemp) {
        await discardTempWorkspace();
        return;
      }

      releaseProject();
      switchToPicker();
    });
  }

  // Ending the last session of a never-named temp workspace throws the whole
  // workspace away: this window hands itself back to the project picker and the
  // folder is deleted. The backend releases the cwd lock first (workspace_delete
  // chdirs the process out), and every PTY under it is already dead — both close
  // paths kill/reap theirs before calling here.
  async function discardTempWorkspace() {
    const path = currentProject;
    releaseProject();
    switchToPicker();

    try {
      settings = await workspace.delete(path);
    } catch {
      showToast("Couldn't delete the temp workspace folder.");
    }
  }

  // ── Relocate (move / rename) with lock handling ─────────────────────────────
  // The kill → backend-op → resume flow lives in lib/workspace-relocate; this
  // shell only lends it the session list and takes back the results.
  const relocator = createRelocator({
    sessions: () => sessions,
    currentProject: () => currentProject,
    removeSessions(ids) {
      sessions = sessions.filter(s => !ids.has(s.id));
      paneIds = paneIds.filter(id => !ids.has(id));

      if (activeId && ids.has(activeId)) {
        activeId = sessions.at(-1)?.id ?? null;
      }
    },
    applySettings(next) {
      settings = next;
    },
    setCurrentProject(path) {
      currentProject = path;
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
    slugSource: () => currentLabel ?? shortDir,
    projectDir: () => currentProject,
    removeSession(id) {
      sessions = sessions.filter(s => s.id !== id);
      paneIds = paneIds.filter(paneId => paneId !== id);
    },
    launchSuccessor: launch
  });
  $effect(() => autoHandoff.check());
  onDestroy(() => autoHandoff.dispose());

  // ── Usage-limit auto-resume ────────────────────────────────────────────────
  // A session stopped by an exhausted usage window resumes the moment the
  // window resets — "continue" into the same session while its context has
  // room, the handoff flow above when it doesn't (lib/stores/usageResume).
  const usageResume = createUsageResume({
    sessions: () => sessions,
    isOptedOut: () => settings.prefs.autoResume === false,
    forceHandoff: session => autoHandoff.force(session)
  });
  $effect(() => usageResume.check());
  onDestroy(() => usageResume.dispose());

  // ── API-error auto-retry ───────────────────────────────────────────────────
  // A session stopped by a transient API error (overloaded, a 5xx, a dropped
  // connection) is nudged with "continue" every 30s, handing off through the
  // flow above when its context is too full to recover (lib/stores/apiErrorRetry).
  const apiErrorRetry = createApiErrorRetry({
    sessions: () => sessions,
    isOptedOut: () => settings.prefs.autoResume === false,
    forceHandoff: session => autoHandoff.force(session)
  });
  $effect(() => apiErrorRetry.check());
  onDestroy(() => apiErrorRetry.dispose());

  // ── Multiple-choice attention ───────────────────────────────────────────────
  // A running agent that puts up a multiple-choice question flashes its tab red
  // (SessionTabs) until the user looks at it or answers. The reconcile runs in an
  // $effect so it re-fires as focus and per-session status change, clearing the
  // flag on the active tab and once a session goes back to working (answered).
  $effect(() => {
    for (const s of sessions) {
      reconcileChoiceAttention({
        id: s.id,
        isActive: s.id === activeId
      });
    }
  });

  // Side panels (lazy-loaded for tree-shaking). A closed set of panel ids
  // defined once; `null` means no panel is open.
  const Side = {
    feed: "feed",
    vcs: "vcs",
    tasks: "tasks",
    config: "config"
  } as const;
  type Side = (typeof Side)[keyof typeof Side] | null;
  let side = $state<Side>(Side.feed);
  function toggleSide(panel: Exclude<Side, null>) {
    side = side === panel ? null : panel;
  }

  // One source of truth for the side panels: the segmented control shows the
  // short label (full name in a tooltip), the shared aside header shows the full
  // label. Count + refresh are published per-panel via the sidePanel store.
  const PANEL_TABS = [
    {
      id: Side.feed,
      icon: "feed",
      short: "Feed",
      label: "Change Feed"
    },
    {
      id: Side.vcs,
      icon: "git",
      short: "Git",
      label: "Version control"
    },
    {
      id: Side.tasks,
      icon: "terminal",
      short: "Tasks",
      label: "Tasks"
    },
    {
      id: Side.config,
      icon: "sliders",
      short: "Config",
      label: "Agent config"
    }
  ] as const;
  const sideTitle = $derived(PANEL_TABS.find(tab => tab.id === side)?.label ?? "");

  // Highlight → agent bridge: a selection in a side panel is injected into the
  // active session's input.
  let selection = $state("");
</script>

<svelte:document
  onselectionchange={() => {
    const sel = getSelection();
    const text = sel?.toString().trim() ?? "";
    const inSidePanel =
      sel?.anchorNode instanceof Node &&
        !!document.querySelector(".side-pane")?.contains(sel.anchorNode);
    selection = text && inSidePanel ? text : "";
  }}
  onvisibilitychange={() => {
    if (!document.hidden) {
      void redetectAgents();
    }
  }}
/>
<!-- Window-level shortcuts, handled in the capture phase: a focused xterm calls
     stopPropagation on keys it handles, so a bubble-phase listener never sees the
     combo while the terminal has focus. -->
<svelte:window
  onkeydowncapture={e => {
    // Dev-only: F5 / Ctrl+Shift+R reloads the WebView — the escape hatch when
    // Vite's HMR socket drops (a Tauri window wires no reload of its own). Ctrl+R
    // is left to the terminal's shell reverse-search. Stripped from prod builds.
    if (import.meta.env.DEV) {
      const isReload =
        e.key === "F5" || (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "r");
      if (isReload) {
        e.preventDefault();
        location.reload();
        return;
      }
    }

    // Ctrl+Shift+Alt+[ / ] cycles to the previous / next open PADE window. Uses
    // e.code, not e.key: holding Shift rewrites "[" to "{", so the layout-position
    // code is the modifier-independent match.
    const isCyclePrevWindow =
      e.ctrlKey && e.shiftKey && e.altKey && e.code === "BracketLeft";
    const isCycleNextWindow =
      e.ctrlKey && e.shiftKey && e.altKey && e.code === "BracketRight";
    if (isCyclePrevWindow || isCycleNextWindow) {
      e.preventDefault();
      e.stopPropagation();
      void windows.focusRelative(isCyclePrevWindow ? "previous" : "next");
      return;
    }

    // Ctrl+Shift+N spawns a fresh empty window (mirrors the app-menu shortcut
    // chip). stopPropagation keeps the terminal from also receiving it.
    const isNewWindow = e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "n";
    if (!isNewWindow) {
      return;
    }

    e.preventDefault();
    e.stopPropagation();
    void openEmptyWindow();
  }}
/>

<!-- Font tokens bound declaratively; they cascade to every descendant. -->
<div style:--font-ui={effective.uiFamily} style:--font-monospace={effective.monoFamily} class="app-root">
  {#if phase === Phase.project}
    <ProjectPicker
      {agents}
      ondelete={relocator.remove}
      onmove={relocator.move}
      onopen={openProject}
      onrename={relocator.rename}
    />
  {:else if phase === Phase.onboarding}
    <Onboarding
      {agents}
      onpick={a => launch({
        agent: a,
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
            label={currentLabel ?? shortDir}
            labels={settings.labels}
            onclearrecent={clearRecentProjects}
            ondelete={deleteDirectory}
            onopen={projectPath => openProject({ path: projectPath })}
            onremoverecent={removeRecentProject}
            onreorderpins={reorderPins}
            onswitch={leaveToPicker}
            ontogglepin={toggleProjectPin}
            path={currentProject}
            pinnedProjects={settings.pinnedProjects}
            recentProjects={settings.recentProjects}
          />
          <span class="chrome-spacer"></span>

          <UsageMeter {sessions} />
          <DesignMenu agent={activeAgent} />
          <!-- Open a console editor (Neovim/Vim/Helix) in its own terminal tab,
               split beside the agent so you can watch and edit at once. GUI editors
               go through the OS (ide.open); these need a real TTY, which only a PADE
               terminal gives. -->
          <!-- The editor is resolved for the *project* (the shared editors-store
               key the Change Feed also reads — SSOT), while the launcher opens
               the active session's worktree when one is focused. -->
          <IdeMenu
            cwd={sessions.find(session => session.id === activeId)?.cwd ?? currentProject}
            onterminaleditor={(editor: Ide) =>
              launch({
                agent: {
                  id: `editor-${editor.id}`,
                  label: editor.label,
                  command: editor.command
                },
                // Inherit the active session's worktree, if any, else the project dir.
                cwd: sessions.find(session => session.id === activeId)?.cwd,
                // Open the working directory in the editor.
                args: ["."],
                split: true
              })}
            project={currentProject}
          />

          <div class="seg" aria-label="Side panels" role="tablist">
            {#each PANEL_TABS as tab (tab.id)}
              <button
                aria-selected={side === tab.id}
                data-tooltip={tab.label}
                onclick={() => toggleSide(tab.id)}
                role="tab"
              ><Icon name={tab.icon} /> <span>{tab.short}</span></button>
            {/each}
          </div>
        </div>

        <SessionTabs
          {activeId}
          {agents}
          {branches}
          onclose={close}
          ondraghint={hint => (dragHint = hint)}
          onlaunch={a => launch({ agent: a })}
          onlaunchbranch={async branch => {
            // Spawn an agent on its own git worktree for `branch`, isolated from
            // the other sessions. Uses the active session's agent (or the first).
            const agent = sessions.find(s => s.id === activeId)?.agent ?? realAgents[0] ?? agents[0];
            const cwd = await vcs.worktreeAdd({
              branch,
              create: false
            });
            launch({
              agent,
              cwd,
              branch
            });
          }}
          onreorder={reorderSessions}
          onselect={selectSession}
          onsplit={splitDrop}
          {paneIds}
          popPaneActive={paneDragOverTabs}
          {sessions}
        />
      </header>

      <main class="body" class:with-side={side !== null}>
        <section bind:this={panesElement} class="pane term-pane" data-panes>
          <!-- A tab dragged over the panes reads as "open in split" — a dashed
               primary frame invites the drop; each pane then shows the left/right
               half the pointer is over (below). -->
          {#if dragOverPanes}
            <div class="drop-overlay">
              <span class="drop-badge"><Icon name="columns" /> Drop to open in split view</span>
            </div>
          {/if}
          {#each orderedSessions as s (s.id)}
            <div
              class="term-slot"
              class:shown={paneIds.includes(s.id)}
              data-pane-id={s.id}
              onoutroend={() => collapsingSplitPanes.delete(s.id)}
              out:collapsePane={{ duration: collapsingSplitPanes.has(s.id) ? 260 : 0 }}
            >
              {#if dropSideFor(s.id) === DropSide.left}
                <div class="drop-half left"></div>
              {:else if dropSideFor(s.id) === DropSide.right}
                <div class="drop-half right"></div>
              {/if}
              <Terminal
                active={s.id === activeId && paneIds.includes(s.id)}
                ondraghint={hint => (paneDragOverTabs = hint?.outside === true)}
                onexit={handleSessionExit}
                onpopout={() => popPaneToTab(s.id)}
                onremove={() => removePane(s.id)}
                onreorder={ids => (paneIds = ids)}
                removable={canRemovePane && paneIds.includes(s.id)}
                session={s}
              />
            </div>
          {/each}

          <div class="add-pane-wrap menu-host">
            <button
              style:anchor-name="--pane-anchor"
              class="add-pane menu-trigger"
              aria-label="Split — add an agent instance"
              data-tooltip="Split — add an agent instance"
              popovertarget="pane-menu"
            ><Icon name="columns" /></button>
            <ul id="pane-menu" style:position-anchor="--pane-anchor" class="menu pane-menu" popover>
              {#if splitCandidates.length > 0}
                <li class="menu-sep">Add to split</li>
                {#each splitCandidates as s (s.id)}
                  <li>
                    <button onclick={() => addPane(s.id)} popovertarget="pane-menu" popovertargetaction="hide">
                      <Icon name="terminal" /> {s.agent.label}
                    </button>
                  </li>
                {/each}
              {/if}
              <li class="menu-sep">Launch a new instance</li>
              {#each agents as a (a.id)}
                <li>
                  <button
                    onclick={() => launch({
                      agent: a,
                      split: true
                    })}
                    popovertarget="pane-menu"
                    popovertargetaction="hide"
                  >{a.label}</button>
                </li>
              {/each}
            </ul>
          </div>
        </section>

        {#if side !== null}
          <aside class="pane side-pane">
            <header class="panel-head">
              <div class="panel-title">
                <h2>{sideTitle}</h2>
                {#if panelCount() !== null}
                  <span class="panel-count">{formatCount(panelCount() ?? 0)}</span>
                {/if}
              </div>
              {#if panelRefresh()}
                <button
                  class="panel-refresh"
                  aria-label="Refresh"
                  data-tooltip="Refresh"
                  onclick={() => panelRefresh()?.()}
                >
                  <Icon name="refresh" />
                </button>
              {/if}
            </header>

            <div class="panel-body">
              {#if side === Side.feed}
                <ChangeFeed project={currentProject} />
              {:else if side === Side.vcs}
                {#await import("@/panels/VcsPanel.svelte") then { default: VcsPanel }}
                  <VcsPanel />
                {/await}
              {:else if side === Side.tasks}
                {#await import("@/panels/TasksPanel.svelte") then { default: TasksPanel }}
                  <!-- Run a project task as a streaming runner in the dock (not a
                       throwaway terminal tab), so its output stays visible and can
                       be piped into an agent. -->
                  <TasksPanel
                    onrun={async (task: {
                      label: string;
                      command: string;
                      cwd: string;
                      kind: TaskGroup["kind"];
                    }) => await startRunner(task)}
                  />
                {/await}
              {:else if side === Side.config}
                {#await import("@/panels/ConfigPanel.svelte") then { default: ConfigPanel }}
                  <ConfigPanel agent={activeAgent} />
                {/await}
              {/if}
            </div>
          </aside>
        {/if}
      </main>

      <RunnerDock activeSessionId={activeId} />

      {#if autoHandoff.note || usageResume.note || apiErrorRetry.note}
        <output class="handoff-note">
          <span class="hdot"></span>
          {autoHandoff.note || usageResume.note || apiErrorRetry.note}
        </output>
      {/if}

      {#if toastText()}
        <!-- <output> already carries role=status — a transient bottom pill that
             auto-dismisses via the showToast timer. -->
        <output class="toast">
          <span class="tdot"><Icon name="external" /></span>
          {toastText()}
        </output>
      {/if}

      {#if selection}
        <button
          class="send-fab"
          onclick={async () => {
            if (!selection || !activeId) {
              return;
            }

            await pty.write({
              id: activeId,
              data: selection
            });
            selection = "";
            getSelection()?.removeAllRanges();
          }}
        >
          ◆ Send to agent
          <!-- Truncation is pure CSS (.preview: max-inline-size + ellipsis). -->
          <span class="preview">{selection}</span>
        </button>
      {/if}
    </div>
  {:else}
    <!-- Loading phase: a calm branded ground so a booting window is never a
         blank void (also the safety net if a boot step stalls). -->
    <div class="booting">
      <span class="brand" aria-label="PADE is starting"><Logo size={64} /></span>
    </div>
  {/if}
</div>

<style>
  .app-root {
    block-size: 100%;
  }

  /* Booting ground — a centered, gently pulsing brand mark on the app surface
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

  /* Native popover (light-dismiss on outside click) anchored to its button. */
  .menu {
    position: absolute;
    inset: auto;
    min-inline-size: 220px;
    margin-block: 6px 0;
    margin-inline: 0;
    padding: 6px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);
    list-style: none;
    box-shadow: 0 16px 40px var(--shadow-color);
    position-area: bottom span-right;

    li button {
      display: flex;
      gap: 9px;
      align-items: center;
      inline-size: 100%;
      padding: 8px 10px;
      border: none;
      border-radius: var(--radius-small);
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      font-size: 13px;
      text-align: start;
      cursor: pointer;
      transition: color 120ms var(--ease), background 120ms var(--ease);

      &:hover {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }
    }

    .menu-sep {
      margin-block: 6px 2px;
      padding-block: 2px 4px;
      padding-inline: 10px;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }
  }

  .seg {
    display: inline-flex;
    flex-shrink: 0;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
    background: var(--surface-2);

    button {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      padding: 6px 12px;
      border: none;
      border-radius: 999px;
      background: transparent;
      color: var(--on-surface-variant);
      font: inherit;
      font-weight: 600;
      font-size: 12px;
      cursor: pointer;
      transition: background 200ms var(--ease), color 200ms var(--ease);
    }

    /* Selected tab — matched by state, not by qualifying the button type. */
    [aria-selected="true"] {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .body {
    display: grid;
    flex: 1;

    /* Explicit full-height row track: without it the implicit row is `auto`
       (content-sized), so the terminal shrinks with the window but never grows
       back — the resize observer only fires on the shrink. */
    grid-template-rows: 1fr;
    grid-template-columns: 1fr;
    min-block-size: 0;
    transition: grid-template-columns 250ms var(--ease);

    &.with-side {
      grid-template-columns: 1fr minmax(320px, 420px);
    }
  }

  .pane {
    overflow: hidden;
    min-block-size: 0;
    min-inline-size: 0;
  }

  .side-pane {
    display: flex;
    flex-direction: column;
    border-inline-start: 1px solid var(--outline);
    background: var(--surface);
    animation: panel-in 340ms var(--ease);
  }

  /* One shared header for every panel (DRY) — title + optional count + optional
     refresh. The panels below own only their scroll body. */
  .panel-head {
    display: flex;
    flex-shrink: 0;
    gap: 8px;
    align-items: center;
    padding-block: 12px 10px;
    padding-inline: 16px;
  }

  .panel-title {
    display: flex;
    gap: 9px;
    align-items: center;

    h2 {
      margin: 0;
      font-weight: 700;
      font-size: 15px;
    }
  }

  .panel-count {
    padding: 2px 9px;
    border-radius: 999px;
    background: var(--primary-container);
    color: var(--on-primary-container);
    font-weight: 700;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }

  .panel-refresh {
    display: grid;
    place-items: center;
    block-size: 28px;
    inline-size: 28px;
    margin-inline-start: auto;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: color 140ms var(--ease), background 140ms var(--ease);

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
    }
  }

  .panel-body {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-block-size: 0;
  }

  /* All sessions stay mounted so their scrollback survives switching; only the
     sessions in the current split are laid out (side by side), the rest collapse
     out of flow. */
  .term-pane {
    position: relative;
    display: flex;
    align-items: stretch;
  }

  .term-slot {
    position: relative;
    display: none;
    flex: 1;
    flex-direction: column;
    min-block-size: 0;
    min-inline-size: 0;
    border-inline-end: 1px solid var(--outline);

    /* A pane appearing is deliberately instant — opening, closing, or switching a
       tab must not animate the terminal (it would read as a needless wipe). Only a
       genuine split-ADD springs its newcomer in, fired imperatively on that one
       slot via animatePaneIn (pane-enter), never here on every `.shown`. */
    &.shown {
      display: flex;
    }
  }

  /* Dashed primary frame over the whole panes area while a tab is dragged onto
     it — the invitation to open the session as a split. */
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
      border-radius: 999px;
      background: var(--primary-container);
      color: var(--on-primary-container);
      font-weight: 700;
      font-size: 13px;
      box-shadow: 0 6px 20px var(--shadow-color);
    }
  }

  /* The half of a pane the pointer is over — where the dropped session will land
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

  /* The split menu opens to the left since its trigger sits at the right edge. */
  .pane-menu {
    position-area: bottom span-left;
  }

  @media (width <= 720px) {
    .body.with-side {
      grid-template-rows: 1fr 40%;
      grid-template-columns: 1fr;
    }
  }

  /* Auto-handoff banner — a calm status pill while a session hands off. */
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
    border-radius: 999px;
    background: var(--primary-container);
    color: var(--on-primary-container);
    font-weight: 600;
    font-size: 13px;
    box-shadow: 0 10px 30px var(--shadow-color);
    transform: translateX(-50%);
    animation: pop-in 220ms var(--ease);

    .hdot {
      flex: none;
      block-size: 8px;
      inline-size: 8px;
      border-radius: 999px;
      background: var(--primary);
      animation: pulse 1100ms var(--ease) infinite;
    }
  }

  /* Transient status toast — sits just above the send FAB, auto-dismissed. */
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
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface);
    font-weight: 600;
    font-size: 13px;
    box-shadow: 0 12px 34px var(--shadow-color);
    transform: translateX(-50%);
    animation: pop-in 240ms var(--spring);

    .tdot {
      display: inline-flex;
      color: var(--primary);
    }
  }

  /* FAB entrance: drops in from below and bounces up into place. Bakes the
     translateX(-50%) centering into every step since it animates transform. */
  @keyframes send-pop {
    0% {
      opacity: 0%;
      transform: translateX(-50%) translateY(12px) scale(0.88);
    }

    65% {
      opacity: 100%;
      transform: translateX(-50%) translateY(-4px) scale(1.03);
    }

    100% {
      opacity: 100%;
      transform: translateX(-50%) translateY(0) scale(1);
    }
  }

  .send-fab {
    position: fixed;
    inset-block-end: 26px;
    inset-inline-start: 50%;
    z-index: 80;
    display: inline-flex;
    gap: 10px;
    align-items: center;
    max-inline-size: min(560px, 90vw);
    padding: 12px 20px;
    border: none;
    border-radius: 999px;
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 700;
    font-size: 14px;
    box-shadow: 0 10px 30px var(--primary-shadow);
    cursor: pointer;
    transform: translateX(-50%);
    animation: send-pop 220ms var(--ease);

    .preview {
      overflow: hidden;
      font-family: var(--font-monospace);
      font-weight: 500;
      font-size: 12px;
      text-overflow: ellipsis;
      white-space: nowrap;
      opacity: 85%;
    }
  }
</style>
