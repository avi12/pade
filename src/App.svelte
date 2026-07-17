<script lang="ts">
  import AppMenu from "@/lib/AppMenu.svelte";
  import { createAutoNamer } from "@/lib/autoName";
  import {
    agents as agentsApi,
    pty,
    vcs,
    windows,
    workspace
  } from "@/lib/bridge";
  import DesignMenu from "@/lib/DesignMenu.svelte";
  import type { DragHint } from "@/lib/dragReorder";
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import IdeMenu from "@/lib/IdeMenu.svelte";
  import Logo from "@/lib/Logo.svelte";
  import { isTemporaryWorkspace } from "@/lib/paths";
  import { effective } from "@/lib/prefs.svelte";
  import { DropSide, paneInsertIndex } from "@/lib/reorder";
  import RunnerDock from "@/lib/RunnerDock.svelte";
  import { registerSendShortcut, unregisterSendShortcut } from "@/lib/sendShortcut";
  import SessionTabs from "@/lib/SessionTabs.svelte";
  import { createAutoHandoff } from "@/lib/stores/handoff.svelte";
  import { ensureRunnerListeners, startRunner } from "@/lib/stores/runners.svelte";
  import { dropSessionLabel } from "@/lib/stores/sessionLabels.svelte";
  import { dropNaming } from "@/lib/stores/sessionNaming.svelte";
  import { dropSessionStatus } from "@/lib/stores/sessions.svelte";
  import { panelCount, panelRefresh } from "@/lib/stores/sidePanel.svelte";
  import { initTaskRunDetection } from "@/lib/stores/taskRuns.svelte";
  import { showToast, toastText } from "@/lib/stores/toast.svelte";
  import { registerTabShortcuts } from "@/lib/tabShortcuts";
  import { SHELL_AGENT_ID, StartMode } from "@/lib/types";
  import type {
    Agent,
    AgentSession,
    Ide,
    Settings,
    TaskGroup
  } from "@/lib/types";
  import UsageMeter from "@/lib/UsageMeter.svelte";
  import { FolderPath, parseInput } from "@/lib/validate";
  import { createRelocator } from "@/lib/workspaceRelocate";
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
          side: x < rect.left + rect.width / 2 ? DropSide.left : DropSide.right
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
        startAgentFlow(ctx.cwd);
        await loadBranches();
      } else if (prefersPicker) {
        // Opt-in: show the project picker instead of starting in a temp workspace.
        phase = Phase.project;
      } else {
        // Default: start immediately in a throwaway workspace so there's no
        // blocking picker. The user can switch any time (Switch button).
        const temp = await workspace.temp();
        startAgentFlow(temp);
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
      startAgentFlow(temp);
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

  // Reflect known tasks the agent runs as "running" in the Tasks panel.
  onMount(() => void initTaskRunDetection());

  // Auto-name a temp workspace once the agent has produced real work
  // (lib/autoName): after a few distinct files change, ask the agent (or a
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

  // Send-from-IDE bridge (lib/sendShortcut): copy in any external editor, press
  // the global shortcut, and the clipboard lands in the active agent's input.
  onMount(() => {
    void registerSendShortcut({
      activeId: () => activeId,
      activeLabel: () => sessions.find(s => s.id === activeId)?.agent.label ?? "agent"
    });
    return () => void unregisterSendShortcut();
  });

  // Tab shortcuts (lib/tabShortcuts): capture-phase so they win over a focused
  // agent terminal — new tab, launch menu, close, and next/previous cycling.
  onMount(() =>
    registerTabShortcuts({
      newTab,
      launchMenu: openLaunchMenu,
      closeTab: closeActiveTab,
      next: () => stepSession(1),
      previous: () => stepSession(-1)
    }));

  async function openEmptyWindow() {
    await windows.create({ mode: WindowMode.empty });
    showToast("Opened a new window");
  }

  async function openProject(target: {
    path: string;
    initialPrompt?: string;
  }) {
    // If another window already has this project open, focus it instead of
    // opening a second copy here — the picker window stays put.
    if (await windows.focusProject(target.path)) {
      return;
    }

    await workspace.open(target.path);
    settings = await workspace.settings(); // pick up the updated recent history
    startAgentFlow(target.path, target.initialPrompt);
    await loadBranches();
  }

  // Decide how to enter a project: honor a saved per-project/default agent,
  // else launch the best installed agent outright — opening a project always
  // lands in the workspace, never on a blocking chooser. The agent picker
  // still appears after a hand-closed or exited last session, where "what
  // next?" is a genuine question. (Reused for every entry path.)
  function startAgentFlow(path: string, initialPrompt?: string) {
    currentProject = path;
    // Let other windows' pickers focus this one instead of reopening the project.
    void windows.registerProject(path);
    const prefId = settings.projectAgents[path] ?? settings.defaultAgent ?? null;
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

  function launch(opts: {
    agent: Agent;
    initialPrompt?: string;
    cwd?: string;
    branch?: string;
    /** Extra command args — the project path when running a terminal editor. */
    args?: string[];
    /** Add alongside the current panes (split) instead of replacing them. */
    split?: boolean;
  }) {
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
    pendingPrompt = undefined;
    phase = Phase.ready;
  }

  // A tab click shows that session as the sole pane (classic single view).
  function selectSession(id: string) {
    activeId = id;
    paneIds = [id];
  }

  // Show an already-running session alongside the current pane(s).
  function addPane(id: string) {
    if (!paneIds.includes(id)) {
      paneIds = [...paneIds, id];
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

  // ── Tab keyboard shortcuts (lib/tabShortcuts) ───────────────────────────────
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
    sessions = sessions.filter(s => s.id !== id);
    paneIds = paneIds.filter(paneId => paneId !== id);
    sessionLaunchedAt.delete(id);
    dropSessionStatus(id);
    dropSessionLabel(id);
    dropNaming(id);

    if (activeId === id) {
      activeId = paneIds.at(-1) ?? sessions.at(-1)?.id ?? null;
    }

    if (paneIds.length === 0 && activeId) {
      paneIds = [activeId];
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

  // The picker's "this project" tag is only meaningful when we came from an active
  // project (the workspace) — not from onboarding, a bare launch into the picker,
  // or a discarded temp workspace (whose project no longer exists).
  let pickerHasActiveProject = $state(false);

  function switchToPicker({ fromActiveProject = phase === Phase.ready } = {}) {
    pickerHasActiveProject = fromActiveProject;
    document.startViewTransition(async () => {
      phase = Phase.project;
      await tick();
    });
  }

  // Ending the last session of a never-named temp workspace throws the whole
  // workspace away: this window hands itself back to the project picker and the
  // folder is deleted. The backend releases the cwd lock first (workspace_delete
  // chdirs the process out), and every PTY under it is already dead — both close
  // paths kill/reap theirs before calling here.
  async function discardTempWorkspace() {
    const path = currentProject;
    currentProject = "";
    branches = [];
    pendingPrompt = undefined;
    // Drop this window's claim on the project so no picker tries to focus it.
    void windows.registerProject("");
    switchToPicker({ fromActiveProject: false });

    try {
      settings = await workspace.delete(path);
    } catch {
      showToast("Couldn't delete the temp workspace folder.");
    }
  }

  // ── Relocate (move / rename) with lock handling ─────────────────────────────
  // The kill → backend-op → resume flow lives in lib/workspaceRelocate; this
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
    isOptedOut: () => settings.prefs.autoHandoff === false,
    slugSource: () => currentLabel ?? shortDir,
    removeSession(id) {
      sessions = sessions.filter(s => s.id !== id);
      paneIds = paneIds.filter(paneId => paneId !== id);
    },
    launchSuccessor: launch
  });
  $effect(() => autoHandoff.check());
  onDestroy(() => autoHandoff.dispose());

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
      hasActiveProject={pickerHasActiveProject}
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
            onswitch={switchToPicker}
            path={currentProject}
            recentProjects={settings.recentProjects}
          />
          <span class="chrome-spacer"></span>

          <UsageMeter {sessions} />
          <DesignMenu agent={activeAgent} />
          <!-- Open a console editor (Neovim/Vim/Helix) in its own terminal tab,
               split beside the agent so you can watch and edit at once. GUI editors
               go through the OS (ide.open); these need a real TTY, which only a PADE
               terminal gives. -->
          <IdeMenu
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
            <div class="term-slot" class:shown={paneIds.includes(s.id)} data-pane-id={s.id}>
              {#if dropSideFor(s.id) === DropSide.left}
                <div class="drop-half left"></div>
              {:else if dropSideFor(s.id) === DropSide.right}
                <div class="drop-half right"></div>
              {/if}
              <Terminal
                active={s.id === activeId && paneIds.includes(s.id)}
                onexit={handleSessionExit}
                onremove={() => removePane(s.id)}
                onreorder={ids => (paneIds = ids)}
                removable={canRemovePane && paneIds.includes(s.id)}
                session={s}
              />
            </div>
          {/each}

          <div class="add-pane-wrap">
            <button
              style:anchor-name="--pane-anchor"
              class="add-pane"
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

      {#if autoHandoff.note}
        <output class="handoff-note">
          <span class="hdot"></span>
          {autoHandoff.note}
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

    &.shown {
      display: flex;
      animation: panel-swap 260ms var(--ease);
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
      max-inline-size: 40ch;
      font-family: var(--font-monospace);
      font-weight: 500;
      font-size: 12px;
      text-overflow: ellipsis;
      white-space: nowrap;
      opacity: 85%;
    }
  }
</style>
