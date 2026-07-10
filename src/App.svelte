<script lang="ts">
  import AppMenu from "@/lib/AppMenu.svelte";
  import {
    agents as agentsApi,
    feed,
    pty,
    usage,
    vcs,
    windows,
    workspace
  } from "@/lib/bridge";
  import DesignMenu from "@/lib/DesignMenu.svelte";
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import IdeMenu from "@/lib/IdeMenu.svelte";
  import { isTemporaryWorkspace } from "@/lib/paths";
  import { effective } from "@/lib/prefs.svelte";
  import RunnerDock from "@/lib/RunnerDock.svelte";
  import { contextPct, dropContext } from "@/lib/stores/context.svelte";
  import { ensureRunnerListeners, startRunner } from "@/lib/stores/runners.svelte";
  import { dropSessionStatus, sessionStatus } from "@/lib/stores/sessions.svelte";
  import { panelCount, panelRefresh } from "@/lib/stores/sidePanel.svelte";
  import { ChangeKind, SessionStatus, SHELL_AGENT_ID, StartMode } from "@/lib/types";
  import type {
    Agent,
    AgentSession,
    ChangeEvent,
    Settings,
    TaskGroup
  } from "@/lib/types";
  import UsageMeter from "@/lib/UsageMeter.svelte";
  import { FolderPath, parseInput } from "@/lib/validate";
  import ChangeFeed from "@/panels/ChangeFeed.svelte";
  import Onboarding from "@/panels/Onboarding.svelte";
  import ProjectPicker from "@/panels/ProjectPicker.svelte";
  import Terminal from "@/panels/Terminal.svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { readText } from "@tauri-apps/plugin-clipboard-manager";
  import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
  import { onDestroy, onMount } from "svelte";
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
  // Active agent id — used to show only its relevant config files.
  const activeAgent = $derived(sessions.find(s => s.id === activeId)?.agent.id ?? "");
  // A pane can be removed only while more than one is shown; sessions not
  // currently shown are offered in the "add to split" menu.
  const canRemovePane = $derived(paneIds.length > 1);
  const splitCandidates = $derived(sessions.filter(s => !paneIds.includes(s.id)));

  // ── Session-tab overflow ────────────────────────────────────────────────────
  // The tab strip is bounded to the width the nav gives it. Tabs that fit render
  // as full pills; the next few collapse to status dots; the remainder live
  // behind a "+N" popover. Pill widths come from an off-layout mirror row
  // (re-measured on session change / reflow) so collapsing a tab never changes
  // the numbers we packed against.
  const TAB_GAP = 6; // px between tab items — mirrors the flex gap
  const DOT_SLOT = 22 + TAB_GAP; // a collapsed status-dot button + its gap
  const MORE_SLOT = 34 + TAB_GAP; // the "+N" overflow button + its gap
  let stripEl = $state<HTMLElement>();
  let measureEl = $state<HTMLElement>();
  let stripWidth = $state(0);
  const tabWidths = new SvelteMap<string, number>();

  // Read each mirror pill's natural width into a fresh map (index-aligned with
  // `sessions`, since the mirror renders them in order).
  function measureTabs() {
    const mirror = measureEl;
    if (!mirror) {
      return;
    }

    tabWidths.clear();
    sessions.forEach((session, index) => {
      const element = mirror.children[index];
      if (element instanceof HTMLElement) {
        tabWidths.set(session.id, element.offsetWidth);
      }
    });
  }

  // Sync the strip's available width, then re-measure the pills.
  function remeasureTabStrip() {
    const strip = stripEl;
    if (strip) {
      stripWidth = strip.clientWidth;
    }

    measureTabs();
  }

  // Re-measure after the mirror re-renders for a changed session set.
  $effect(() => {
    void sessions.length;
    measureTabs();
  });

  // Track the strip's available width and re-measure on any reflow (font load,
  // window resize); both the strip and the mirror are observed.
  $effect(() => {
    const strip = stripEl;
    if (!strip) {
      return;
    }

    const observer = new ResizeObserver(remeasureTabStrip);
    observer.observe(strip);

    if (measureEl) {
      observer.observe(measureEl);
    }

    remeasureTabStrip();
    return () => observer.disconnect();
  });

  // Greedy three-tier packing: full pills → status dots → "+N" overflow.
  type TabPack = {
    visible: string[];
    dots: string[];
    more: string[];
  };
  const tabPack = $derived.by<TabPack>(() => {
    const order = sessions;
    function widthOf(id: string): number {
      return tabWidths.get(id) ?? 0;
    }

    const total = order.reduce((sum, s, index) => sum + widthOf(s.id) + (index ? TAB_GAP : 0), 0);
    // Everything fits (or we haven't measured yet) — all as full pills.
    if (stripWidth === 0 || total <= stripWidth) {
      return {
        visible: order.map(s => s.id),
        dots: [],
        more: []
      };
    }

    // We know we'll overflow, so reserve room for the "+N" button.
    const budget = stripWidth - MORE_SLOT;
    const visible: string[] = [];
    let used = 0;
    for (const session of order) {
      const next = used + widthOf(session.id) + (visible.length ? TAB_GAP : 0);
      if (next > budget) {
        break;
      }

      visible.push(session.id);
      used = next;
    }

    // Always keep at least one pill so the bar is never only a "+N".
    if (visible.length === 0 && order.length > 0) {
      visible.push(order[0].id);
      used = widthOf(order[0].id);
    }

    const rest = order.slice(visible.length);
    const dots: string[] = [];
    let dotRoom = budget - used;
    for (const session of rest) {
      if (dotRoom < DOT_SLOT) {
        break;
      }

      dots.push(session.id);
      dotRoom -= DOT_SLOT;
    }

    return {
      visible,
      dots,
      more: rest.slice(dots.length).map(s => s.id)
    };
  });

  const bySessionId = $derived(new Map(sessions.map(s => [s.id, s] as const)));
  function tabsFor(ids: string[]): AgentSession[] {
    return ids
      .map(id => bySessionId.get(id))
      .filter((s): s is AgentSession => s !== undefined);
  }
  const visibleSessions = $derived(tabsFor(tabPack.visible));
  const dotSessions = $derived(tabsFor(tabPack.dots));
  const moreSessions = $derived(tabsFor(tabPack.more));
  const hasMoreSessions = $derived(moreSessions.length > 0);
  const overflowHasActive = $derived(
    activeId !== null && (tabPack.dots.includes(activeId) || tabPack.more.includes(activeId))
  );

  // How a spawned window routes off its query string (window_create encodes the
  // target here). A closed set defined once so no bare literal leaks into boot.
  const WindowMode = {
    empty: "empty",
    temp: "temp",
    open: "open"
  } as const;
  type WindowMode = (typeof WindowMode)[keyof typeof WindowMode];

  onMount(async () => {
    const [detected, saved] = await Promise.all([
      agentsApi.detect(),
      workspace.settings()
    ]);
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

  // Show the project picker on demand to switch project / open recent / create.
  function switchProject() {
    phase = Phase.project;
  }

  // Re-detect installed agents so the picker reflects an agent the user just
  // installed or removed — on window focus (they alt-tab back from a terminal)
  // and on a slow poll as a fallback.
  async function redetectAgents() {
    agents = await agentsApi.detect();
  }
  onMount(() => {
    const interval = setInterval(() => void redetectAgents(), 5000);
    return () => clearInterval(interval);
  });

  // Subscribe once to the backend task-runner stream so the dock updates live.
  onMount(() => void ensureRunnerListeners());

  // Auto-name a temp workspace once the agent has produced real work. After a few
  // distinct files change, ask the agent (or a heuristic) for a friendly label
  // and apply it. Fires once per workspace; never blocks or renames on disk.
  const AUTONAME_AFTER = 3;
  const touchedByProject = new SvelteMap<string, SvelteSet<string>>();
  const namedProjects = new SvelteSet<string>();
  let unlistenFeed: UnlistenFn | undefined;
  onMount(async () => {
    await feed.start(); // idempotent — safe even if the Change Feed panel is closed
    unlistenFeed = await feed.onChange(event => void maybeAutoName(event));
  });
  onDestroy(() => unlistenFeed?.());

  // Normalize a path for comparison — watcher and workspace paths can differ in
  // separator/casing on Windows.
  function normPath(path: string): string {
    return path.replaceAll("\\", "/").toLowerCase();
  }

  async function maybeAutoName(event: ChangeEvent) {
    const proj = currentProject;
    const autoNameDisabled = settings.prefs.autoNameTemp === false;
    if (!isTemp || autoNameDisabled) {
      return;
    }

    const alreadyNamed = namedProjects.has(proj) || Boolean(settings.labels[proj]);
    if (event.kind === ChangeKind.enum.deleted || alreadyNamed) {
      return;
    }

    const base = normPath(proj);
    const touched = normPath(event.path);
    if (!touched.startsWith(base)) {
      return;
    }

    const rel = touched.slice(base.length).replace(/^\//, "");
    // Skip dotfiles/dot-dirs (e.g. .git, .claude) — not signal for a name.
    if (!rel || rel.split("/").some(seg => seg.startsWith("."))) {
      return;
    }

    const set = touchedByProject.get(proj) ?? new SvelteSet<string>();
    set.add(touched);
    touchedByProject.set(proj, set);

    if (set.size < AUTONAME_AFTER) {
      return;
    }

    namedProjects.add(proj); // guard so the naming call runs exactly once
    const agent = sessions.find(s => s.id === activeId)?.agent.command ?? "";
    const name = await workspace.autoname({
      path: proj,
      agent
    }).catch(() => null);
    if (name && currentProject === proj) {
      settings = await workspace.setLabel({
        path: proj,
        name
      });
    }
  }

  // Transient status toast — a bottom-center pill that auto-dismisses. Reused by
  // the send-from-IDE bridge and window-open actions; one timer, so a new toast
  // resets the countdown rather than stacking.
  const TOAST_MS = 2400;
  let toast = $state("");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(message: string) {
    toast = message;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast = "";
    }, TOAST_MS);
  }
  onDestroy(() => clearTimeout(toastTimer));

  // Send-from-IDE bridge: highlight + copy a snippet in any external editor,
  // then press this global shortcut to inject the clipboard into the active
  // Claude Code input — works regardless of which IDE the project is open in.
  const SEND_SHORTCUT = "CommandOrControl+Alt+S";
  onMount(() => {
    void setupSendShortcut();
    return () => void unregister(SEND_SHORTCUT).catch(() => {});
  });
  async function setupSendShortcut() {
    await unregister(SEND_SHORTCUT).catch(() => {}); // clean re-register on HMR
    await register(SEND_SHORTCUT, async event => {
      if (event.state !== "Pressed") {
        return;
      }

      const text = (await readText()).trim();
      if (!text || !activeId) {
        return;
      }

      await pty.write({
        id: activeId,
        data: text
      });
      await getCurrentWindow().setFocus();
      const label = sessions.find(s => s.id === activeId)?.agent.label ?? "agent";
      showToast(`Sent selection to ${label}`);
    });
  }

  // Ctrl+Shift+N spawns a fresh empty window (mirrors the app-menu shortcut chip).
  function onWindowKey(event: KeyboardEvent) {
    const isNewWindow = event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "n";
    if (!isNewWindow) {
      return;
    }

    event.preventDefault();
    void openEmptyWindow();
  }
  async function openEmptyWindow() {
    await windows.create({ mode: WindowMode.empty });
    showToast("Opened a new window");
  }

  async function openProject(target: {
    path: string;
    initialPrompt?: string;
  }) {
    await workspace.open(target.path);
    settings = await workspace.settings(); // pick up the updated recent history
    startAgentFlow(target.path, target.initialPrompt);
    await loadBranches();
  }

  // Decide how to enter a project: honor a saved per-project/default agent,
  // else auto-launch a lone agent, else onboard. (Reused for every entry path.)
  function startAgentFlow(path: string, initialPrompt?: string) {
    currentProject = path;
    const prefId = settings.projectAgents[path] ?? settings.defaultAgent ?? null;
    const preferred = prefId ? agents.find(a => a.id === prefId) : undefined;
    if (preferred) {
      return launch({
        agent: preferred,
        initialPrompt
      });
    }

    if (realAgents.length === 1) {
      return launch({
        agent: realAgents[0],
        initialPrompt
      });
    }

    if (realAgents.length === 0) {
      return launch({
        agent: agents[0],
        initialPrompt
      }); // shell
    }

    pendingPrompt = initialPrompt;
    phase = Phase.onboarding;
  }

  function launch(opts: {
    agent: Agent;
    initialPrompt?: string;
    cwd?: string;
    branch?: string;
    /** Add alongside the current panes (split) instead of replacing them. */
    split?: boolean;
  }) {
    const session: AgentSession = {
      id: crypto.randomUUID(),
      agent: opts.agent,
      initialPrompt: opts.initialPrompt,
      cwd: opts.cwd,
      branch: opts.branch
    };
    sessions.push(session);
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

  // Load branches for the current repo (empty when not a git project).
  async function loadBranches() {
    branches = await vcs.branches().catch(() => []);
  }

  // Spawn an agent on its own git worktree for `branch`, isolated from the
  // other sessions. Uses the active session's agent (or the first available).
  async function launchOnBranch(branch: string) {
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
  }

  async function close(session: AgentSession) {
    await pty.kill(session.id);
    sessions = sessions.filter(s => s.id !== session.id);
    paneIds = paneIds.filter(id => id !== session.id);
    dropSessionStatus(session.id);

    if (activeId === session.id) {
      activeId = paneIds.at(-1) ?? sessions.at(-1)?.id ?? null;
    }

    // Keep at least one pane visible while any session remains.
    if (paneIds.length === 0 && activeId) {
      paneIds = [activeId];
    }

    // Closing the last session shows the agent picker (project stays open) —
    // never silently spawn a replacement.
    const wasLastSession = sessions.length === 0;
    if (wasLastSession) {
      pendingPrompt = undefined;
      phase = Phase.onboarding;
    }
  }

  // ── Relocate (move / rename) with lock handling ─────────────────────────────
  // Moving or renaming a workspace fs::renames its folder — which fails while a
  // live agent holds it as cwd (Windows lock). Kill the sessions under it
  // (remembering the live ones), run the backend op (which also re-points every
  // external reference — agent memory dirs, IDE recents…), then resume the live
  // ones on the new path with a "continue" prompt. Idle/exited sessions stay closed.
  async function relocateWorkspace({ from, run }: {
    from: string;
    run: () => Promise<string>;
  }): Promise<string> {
    const base = normPath(from);
    function isUnder(dir: string): boolean {
      const norm = normPath(dir);
      return norm === base || norm.startsWith(`${base}/`);
    }

    function remapUnder(dir: string, target: string): string {
      return target + dir.slice(from.length);
    }

    const locking = sessions.filter(s => isUnder(s.cwd ?? currentProject));
    // Capture the live ones + where they were working, to resume after the move.
    const toResume = locking
      .filter(s => sessionStatus(s.id) !== SessionStatus.enum.exited)
      .map(s => ({
        agent: s.agent,
        oldDir: s.cwd ?? currentProject
      }));

    // Release the lock: kill every session under the dir.
    for (const session of locking) {
      await pty.kill(session.id);
      dropSessionStatus(session.id);
      dropContext(session.id);
    }

    const lockingIds = new Set(locking.map(s => s.id));
    sessions = sessions.filter(s => !lockingIds.has(s.id));
    paneIds = paneIds.filter(id => !lockingIds.has(id));

    if (activeId && lockingIds.has(activeId)) {
      activeId = sessions.at(-1)?.id ?? null;
    }

    // Run the backend move/rename (also re-points every external reference).
    const newPath = await run();
    settings = await workspace.settings();

    if (isUnder(currentProject)) {
      currentProject = remapUnder(currentProject, newPath);
    }

    // Resume the live sessions on the new path, seeded to continue.
    toResume.forEach((entry, index) => launch({
      agent: entry.agent,
      cwd: remapUnder(entry.oldDir, newPath),
      initialPrompt: "continue\r",
      split: index > 0
    }));

    return newPath;
  }

  function moveWorkspace(target: {
    from: string;
    destDir: string;
  }): Promise<string> {
    return relocateWorkspace({
      from: target.from,
      run: () => workspace.move(target)
    });
  }

  function renameWorkspace(target: {
    from: string;
    newName: string;
  }): Promise<string> {
    return relocateWorkspace({
      from: target.from,
      run: () => workspace.rename(target)
    });
  }

  // ── Auto-handoff ───────────────────────────────────────────────────────────
  // When an agent nears its context window, hand off to a fresh one: ask it to
  // write a continue-*.md, end the session, and start a successor seeded to
  // resume from that doc. Opt-out via prefs.autoHandoff; fires once per session.
  const CONTEXT_HANDOFF_PCT = 90;
  const HANDOFF_DOC_TIMEOUT_MS = 120_000;
  const HANDOFF_SETTLE_MS = 3_000;
  const USAGE_EXHAUSTED_PCT = 95;
  const handingOff = new SvelteSet<string>();
  let handoffNote = $state("");

  $effect(() => {
    const optedOut = settings.prefs.autoHandoff === false;
    if (optedOut) {
      return;
    }

    for (const session of sessions) {
      const pct = contextPct({ id: session.id });
      const nearLimit = pct !== null && pct >= CONTEXT_HANDOFF_PCT;
      const idle = sessionStatus(session.id) === SessionStatus.enum.ready;
      const already = handingOff.has(session.id);
      if (nearLimit && idle && !already) {
        handingOff.add(session.id);
        void handoff(session);
      }
    }
  });

  // Only cycle when there's quota to spare — a handoff itself costs tokens. An
  // unknown quota (tier-only) counts as "enough" so the feature still works.
  async function hasEnoughUsage(agent: string): Promise<boolean> {
    const quota = await usage.get(agent).catch(() => null);
    if (!quota || quota.usedPct == null) {
      return true;
    }

    return quota.usedPct < USAGE_EXHAUSTED_PCT;
  }

  // A filesystem-safe slug for the handoff doc, from the workspace label/dir.
  function handoffSlug(): string {
    const slug = (currentLabel ?? shortDir)
      .replaceAll(/[^a-z0-9-]+/gi, "-")
      .replaceAll(/^-+|-+$/g, "")
      .toLowerCase();
    return slug || "session";
  }

  function handoffPrompt(doc: string): string {
    return `\nYour context window is nearly full. Please write a concise handoff to ${doc} — the current state, what you've completed, and the exact next steps to continue — then stop.\r`;
  }

  // In-flight waitForFile resources. A handoff can pend up to 120s, so its
  // feed listener + timers must be torn down if the component unmounts first —
  // otherwise the watcher subscription and timers leak. Tracked here so onDestroy
  // can clear every still-pending wait.
  const pendingUnlistens = new SvelteSet<UnlistenFn>();
  const pendingTimers = new SvelteSet<ReturnType<typeof setTimeout>>();
  onDestroy(() => {
    for (const unlisten of pendingUnlistens) {
      unlisten();
    }

    for (const timer of pendingTimers) {
      clearTimeout(timer);
    }

    pendingUnlistens.clear();
    pendingTimers.clear();
  });

  // Track one timer in the pending set and return its id, so every timer we
  // create is registered for teardown in exactly one place.
  function trackTimer(handler: () => void, delayMs: number): ReturnType<typeof setTimeout> {
    const timer = setTimeout(handler, delayMs);
    pendingTimers.add(timer);
    return timer;
  }

  // Resolve once the watcher sees `name` written (plus a short settle) or on timeout.
  function waitForFile(name: string): Promise<void> {
    return new Promise(resolve => {
      let unlisten: UnlistenFn | undefined;
      let settleTimer: ReturnType<typeof setTimeout> | undefined;
      // Single teardown path: drop the listener + both timers from the pending
      // set, cancel them, then resolve. Used by every exit (match, settle, timeout).
      function finish() {
        if (unlisten) {
          pendingUnlistens.delete(unlisten);
          unlisten();
        }

        for (const timer of [deadlineTimer, settleTimer]) {
          if (timer !== undefined) {
            pendingTimers.delete(timer);
            clearTimeout(timer);
          }
        }

        resolve();
      }

      // Read by finish() only at call time (a timer fires well after this line),
      // so a const in the closure is safe.
      const deadlineTimer = trackTimer(finish, HANDOFF_DOC_TIMEOUT_MS);
      const target = name.toLowerCase();
      // Kick off the async subscription from this sync executor (the one place a
      // .then/IIFE is warranted — rule 6).
      void (async () => {
        unlisten = await feed.onChange(event => {
          const seen = event.path.replaceAll("\\", "/").toLowerCase().endsWith(target);
          if (!seen) {
            return;
          }

          // Restart the short settle window on each matching change; finish only
          // fires once it goes quiet (or the deadline hits first).
          if (settleTimer !== undefined) {
            pendingTimers.delete(settleTimer);
            clearTimeout(settleTimer);
          }

          settleTimer = trackTimer(finish, HANDOFF_SETTLE_MS);
        });
        pendingUnlistens.add(unlisten);
      })();
    });
  }

  async function handoff(session: AgentSession) {
    const enough = await hasEnoughUsage(session.agent.id);
    if (!enough) {
      return; // stay marked so we don't re-check each tick; skip this cycle
    }

    const doc = `continue-${handoffSlug()}.md`;
    handoffNote = `Context nearly full — handing ${session.agent.label} off to a fresh agent…`;

    // 1. Ask the agent to write the handoff doc, then wait for it to land.
    await pty.write({
      id: session.id,
      data: handoffPrompt(doc)
    });
    await waitForFile(doc);

    // 2. End the session, 3. start the successor seeded to continue.
    const { agent, cwd } = session;
    await pty.kill(session.id);
    sessions = sessions.filter(s => s.id !== session.id);
    paneIds = paneIds.filter(id => id !== session.id);
    dropSessionStatus(session.id);
    dropContext(session.id);
    handingOff.delete(session.id);
    launch({
      agent,
      cwd,
      initialPrompt: `Read ${doc} and continue the work where the previous session left off.\r`
    });
    handoffNote = "";
  }

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

  // The shared side-panel header renders each panel's title here; the live count
  // and refresh action are published by the active panel via the sidePanel store.
  const SIDE_TITLES: Record<Exclude<Side, null>, string> = {
    [Side.feed]: "Change Feed",
    [Side.vcs]: "Version control",
    [Side.tasks]: "Tasks",
    [Side.config]: "Agent config"
  };
  const sideTitle = $derived(side ? SIDE_TITLES[side] : "");

  // Run a project task as a streaming runner in the dock (not a throwaway
  // terminal tab), so its output stays visible and can be piped into an agent.
  async function runTask(task: {
    label: string;
    command: string;
    cwd: string;
    kind: TaskGroup["kind"];
  }) {
    await startRunner(task);
  }

  // Highlight → agent bridge: a selection in a side panel is injected into the
  // active session's input.
  let selection = $state("");
  function readSelection() {
    const sel = window.getSelection();
    const text = sel?.toString().trim() ?? "";
    const inSidePanel =
      sel?.anchorNode instanceof Node &&
        !!document.querySelector(".side-pane")?.contains(sel.anchorNode);
    selection = text && inSidePanel ? text : "";
  }
  async function sendToAgent() {
    if (!selection || !activeId) {
      return;
    }

    await pty.write({
      id: activeId,
      data: selection
    });
    selection = "";
    window.getSelection()?.removeAllRanges();
  }
</script>

<svelte:document onselectionchange={readSelection} />
<svelte:window onfocus={() => void redetectAgents()} onkeydown={onWindowKey} />

<!-- Font tokens bound declaratively; they cascade to every descendant. -->
<div style:--font-ui={effective.uiFamily} style:--font-monospace={effective.monoFamily} class="app-root">
  {#if phase === Phase.project}
    <ProjectPicker {agents} onmove={moveWorkspace} onopen={openProject} onrename={renameWorkspace} />
  {:else if phase === Phase.onboarding}
    <Onboarding
      {agents} onpick={a => launch({
        agent: a,
        initialPrompt: pendingPrompt
      })} />
  {:else if phase === Phase.ready}
    <div class="shell">
      <header class="topbar">
        <AppMenu
          {isTemp}
          label={currentLabel ?? shortDir}
          labels={settings.labels}
          onswitch={switchProject}
          path={currentProject}
          recentProjects={settings.recentProjects}
        />
        <span class="divider"></span>

        {#snippet fullTab(s: AgentSession)}
          <div class="tab" class:active={s.id === activeId} class:shown={paneIds.includes(s.id)}>
            <button class="pick" onclick={() => selectSession(s.id)}>
              <span class="dot {sessionStatus(s.id)}"></span>
              {s.agent.label}
            </button>
            <button
              class="x"
              aria-label="Close session"
              data-tooltip="Close session"
              onclick={() => close(s)}
            >×</button>
          </div>
        {/snippet}

        <nav class="tabs" aria-label="Agent sessions">
          <div bind:this={stripEl} class="tab-strip">
            {#each visibleSessions as s (s.id)}
              {@render fullTab(s)}
            {/each}

            {#each dotSessions as s (s.id)}
              <button
                class="tab-dot"
                class:active={s.id === activeId}
                aria-label={s.agent.label}
                data-tooltip={s.agent.label}
                onclick={() => selectSession(s.id)}
              ><span class="dot {sessionStatus(s.id)}"></span></button>
            {/each}

            {#if hasMoreSessions}
              <span class="more-wrap">
                <button
                  style:anchor-name="--more-anchor"
                  class="more-btn"
                  class:active={overflowHasActive}
                  aria-label="Show remaining sessions"
                  popovertarget="more-menu"
                >+{moreSessions.length}</button>
                <ul id="more-menu" style:position-anchor="--more-anchor" class="menu more-menu" popover>
                  {#each moreSessions as s (s.id)}
                    <li class="more-item" class:active={s.id === activeId}>
                      <button
                        class="more-pick"
                        onclick={() => selectSession(s.id)}
                        popovertarget="more-menu"
                        popovertargetaction="hide"
                      >
                        <span class="dot {sessionStatus(s.id)}"></span>
                        <span class="more-label">{s.agent.label}</span>
                      </button>
                      <button
                        class="more-x"
                        aria-label="Close session"
                        data-tooltip="Close session"
                        onclick={() => close(s)}
                      >×</button>
                    </li>
                  {/each}
                </ul>
              </span>
            {/if}
          </div>

          <!-- Off-layout mirror: every tab at full width, purely for measuring. -->
          <span bind:this={measureEl} class="tab-measure" aria-hidden="true">
            {#each sessions as s (s.id)}
              {@render fullTab(s)}
            {/each}
          </span>

          <button
            style:anchor-name="--add-anchor"
            class="add-btn"
            aria-label="Add an agent"
            data-tooltip="Add an agent"
            popovertarget="add-menu"
          >+</button>
          <ul id="add-menu" style:position-anchor="--add-anchor" class="menu" popover>
            {#each agents as a (a.id)}
              <li>
                <button onclick={() => launch({ agent: a })} popovertarget="add-menu" popovertargetaction="hide">
                  {a.label}
                </button>
              </li>
            {/each}
            {#if branches.length > 0}
              <li class="menu-sep">On a branch — new worktree</li>
              {#each branches as b (b)}
                <li>
                  <button
                    onclick={async () => await launchOnBranch(b)}
                    popovertarget="add-menu"
                    popovertargetaction="hide"
                  ><Icon name="git" /> {b}</button>
                </li>
              {/each}
            {/if}
          </ul>
        </nav>

        <UsageMeter />
        <DesignMenu agent={activeAgent} />
        <IdeMenu />

        <div class="seg" aria-label="Side panel" role="tablist">
          <button aria-selected={side === Side.feed} onclick={() => toggleSide(Side.feed)} role="tab">
            <Icon name="feed" /> Change Feed
          </button>
          <button aria-selected={side === Side.vcs} onclick={() => toggleSide(Side.vcs)} role="tab">
            <Icon name="git" /> Git
          </button>
          <button aria-selected={side === Side.tasks} onclick={() => toggleSide(Side.tasks)} role="tab">
            <Icon name="terminal" /> Tasks
          </button>
          <button aria-selected={side === Side.config} onclick={() => toggleSide(Side.config)} role="tab">
            <Icon name="sliders" /> Config
          </button>
        </div>
      </header>

      <main class="body" class:with-side={side !== null}>
        <section class="pane term-pane">
          {#each sessions as s (s.id)}
            <div class="term-slot" class:shown={paneIds.includes(s.id)}>
              {#if canRemovePane && paneIds.includes(s.id)}
                <button
                  class="remove-pane"
                  aria-label="Remove from split"
                  data-tooltip="Remove from split"
                  onclick={() => removePane(s.id)}
                ><Icon name="close" /></button>
              {/if}
              <Terminal session={s} />
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
                <ChangeFeed />
              {:else if side === Side.vcs}
                {#await import("@/panels/VcsPanel.svelte") then { default: VcsPanel }}
                  <VcsPanel />
                {/await}
              {:else if side === Side.tasks}
                {#await import("@/panels/TasksPanel.svelte") then { default: TasksPanel }}
                  <TasksPanel onrun={runTask} />
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

      {#if handoffNote}
        <output class="handoff-note">
          <span class="hdot"></span>
          {handoffNote}
        </output>
      {/if}

      {#if toast}
        <!-- <output> already carries role=status — a transient bottom pill that
             auto-dismisses via the showToast timer. -->
        <output class="toast">
          <span class="tdot"><Icon name="external" /></span>
          {toast}
        </output>
      {/if}

      {#if selection}
        <button class="send-fab" onclick={sendToAgent}>
          ◆ Send to agent
          <!-- Truncation is pure CSS (.preview: max-inline-size + ellipsis). -->
          <span class="preview">{selection}</span>
        </button>
      {/if}
    </div>
  {/if}
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
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 16px;
    border-block-end: 1px solid var(--outline);
    background: var(--surface-1);
  }

  /* Thin vertical rule between the app menu and the sessions nav (canvas uses a
     span, not a border on the neighbouring element). */
  .divider {
    flex-shrink: 0;
    block-size: 20px;
    inline-size: 1px;
    background: var(--outline);
  }

  .tabs {
    position: relative;
    display: flex;
    flex: 1 1 0;
    gap: 6px;
    align-items: center;
    min-inline-size: 0;

    /* The visible, bounded strip — pills/dots/+N clip here rather than wrap. */
    .tab-strip {
      display: flex;
      flex: 1;
      gap: 6px;
      align-items: center;
      overflow: hidden;
      min-inline-size: 0;
    }

    /* Off-layout copy of every full pill, measured to drive the packing. */
    .tab-measure {
      position: absolute;
      inset-block-start: 0;
      inset-inline-start: 0;
      display: flex;
      gap: 6px;
      visibility: hidden;
      pointer-events: none;
    }

    /* A session collapsed to just its status dot. */
    .tab-dot {
      display: inline-grid;
      flex: none;
      place-items: center;
      block-size: 22px;
      inline-size: 22px;
      border: none;
      border-radius: 999px;
      background: var(--surface-2);
      cursor: pointer;
      transition: background 150ms var(--ease);

      &.active {
        background: var(--primary-container);
      }

      &:hover {
        background: var(--surface-3);
      }
    }

    .more-wrap {
      flex: none;
    }

    /* The "+N" overflow trigger. */
    .more-btn {
      display: inline-flex;
      flex: none;
      align-items: center;
      block-size: 22px;
      padding-inline: 9px;
      border: none;
      border-radius: 999px;
      background: var(--surface-2);
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 11px;
      font-variant-numeric: tabular-nums;
      cursor: pointer;
      transition: color 150ms var(--ease), background 150ms var(--ease);

      &.active {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }

      &:hover {
        background: var(--surface-3);
      }
    }

    .tab {
      display: inline-flex;
      align-items: center;
      overflow: hidden;
      border-radius: 999px;
      background: var(--surface-2);
      animation: spring-in 320ms var(--ease);

      &.active {
        background: var(--primary-container);
      }

      &.active .pick {
        color: var(--on-primary-container);
        font-weight: 600;
      }
    }

    .pick {
      display: inline-flex;
      gap: 7px;
      align-items: center;
      padding-block: 6px;
      padding-inline: 12px 4px;
      border: none;
      background: transparent;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 12px;
      cursor: pointer;
    }

    /* Per-session status dot — mirrors the SessionBadge states. */
    .dot {
      flex: none;
      block-size: 8px;
      inline-size: 8px;
      border-radius: 999px;
      background: var(--on-surface-variant);

      &.working {
        background: var(--primary);
        animation: pulse 1100ms var(--ease) infinite;
      }

      &.ready {
        background: var(--tertiary);
        box-shadow: 0 0 0 4px var(--tertiary-wash);
      }
    }

    .x {
      display: inline-flex;
      justify-content: center;
      align-items: center;
      block-size: 26px;
      inline-size: 24px;
      border: none;
      border-end-start-radius: 0;
      border-end-end-radius: 999px;
      border-start-end-radius: 999px;
      border-start-start-radius: 0;
      background: transparent;
      color: var(--on-surface-variant);
      font-size: 15px;
      line-height: 1;
      opacity: 60%;
      cursor: pointer;
      transition: color 150ms var(--ease), background 150ms var(--ease), opacity 150ms var(--ease);

      &:hover {
        background: var(--critical-wash);
        color: var(--critical);
        opacity: 100%;
      }
    }
  }

  .add-btn {
    display: grid;
    place-items: center;
    block-size: 30px;
    inline-size: 30px;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-variant);
    font-size: 18px;
    cursor: pointer;
    transition: color 150ms var(--ease), background 150ms var(--ease);

    &:hover {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
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

  /* Overflow-session popover: a compact two-column grid of the collapsed tabs. */
  .more-menu {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 4px;
    inline-size: min(360px, 80vw);
    min-inline-size: 0;

    .more-item {
      display: flex;
      align-items: center;
      border-radius: var(--radius-small);

      &.active {
        background: var(--primary-container);
      }

      &.active .more-pick {
        color: var(--on-primary-container);
      }
    }

    .more-pick {
      display: flex;
      flex: 1;
      gap: 8px;
      align-items: center;
      inline-size: auto;
      min-inline-size: 0;
      font-family: var(--font-monospace);
      font-size: 12px;
    }

    .more-label {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .more-x {
      flex: none;
      justify-content: center;
      inline-size: 26px;
      padding: 0;
      color: var(--on-surface-variant);
      font-size: 15px;

      &:hover {
        background: var(--critical-wash);
        color: var(--critical);
      }
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
    animation: panel-in 320ms var(--ease);
  }

  /* One shared header for every panel (DRY) — title + optional count + optional
     refresh. The panels below own only their scroll body. */
  .panel-head {
    display: flex;
    flex-shrink: 0;
    gap: 8px;
    align-items: center;
    padding-block: 12px;
    padding-inline: 16px;
    border-block-end: 1px solid var(--outline);
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

  .remove-pane {
    position: absolute;
    inset-block-start: 8px;
    inset-inline-end: 8px;
    z-index: 5;
    display: inline-flex;
    justify-content: center;
    align-items: center;
    block-size: 24px;
    inline-size: 24px;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: color 150ms var(--ease), background 150ms var(--ease);

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
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
    align-items: center;
    inline-size: 44px;
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
    box-shadow: 0 10px 30px color-mix(in sRGB, var(--primary) 45%, transparent);
    cursor: pointer;
    transform: translateX(-50%);
    animation: pop-in 220ms var(--ease);

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
