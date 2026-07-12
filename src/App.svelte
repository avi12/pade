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
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import IdeMenu from "@/lib/IdeMenu.svelte";
  import { isTemporaryWorkspace } from "@/lib/paths";
  import { effective } from "@/lib/prefs.svelte";
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
  import { SHELL_AGENT_ID, StartMode } from "@/lib/types";
  import type { Agent, AgentSession, Settings, TaskGroup } from "@/lib/types";
  import UsageMeter from "@/lib/UsageMeter.svelte";
  import { FolderPath, parseInput } from "@/lib/validate";
  import { createRelocator } from "@/lib/workspaceRelocate";
  import ChangeFeed from "@/panels/ChangeFeed.svelte";
  import Onboarding from "@/panels/Onboarding.svelte";
  import ProjectPicker from "@/panels/ProjectPicker.svelte";
  import Terminal from "@/panels/Terminal.svelte";
  import { onDestroy, onMount } from "svelte";

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
  // else auto-launch a lone agent, else onboard. (Reused for every entry path.)
  function startAgentFlow(path: string, initialPrompt?: string) {
    currentProject = path;
    // Let other windows' pickers focus this one instead of reopening the project.
    void windows.registerProject(path);
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
    dropSessionLabel(session.id);
    dropNaming(session.id);

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
    <ProjectPicker {agents} onmove={relocator.move} onopen={openProject} onrename={relocator.rename} />
  {:else if phase === Phase.onboarding}
    <Onboarding
      {agents} onpick={a => launch({
        agent: a,
        initialPrompt: pendingPrompt
      })} />
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
            onswitch={switchProject}
            path={currentProject}
            recentProjects={settings.recentProjects}
          />
          <span class="chrome-spacer"></span>

          <UsageMeter />
          <DesignMenu agent={activeAgent} />
          <IdeMenu />

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
          onlaunch={a => launch({ agent: a })}
          onlaunchbranch={launchOnBranch}
          onselect={selectSession}
          {paneIds}
          {sessions}
        />
      </header>

      <main class="body" class:with-side={side !== null}>
        <section class="pane term-pane">
          {#each sessions as s (s.id)}
            <div class="term-slot" class:shown={paneIds.includes(s.id)}>
              <Terminal
                onremove={() => removePane(s.id)}
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
        <button class="send-fab" onclick={sendToAgent}>
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
      <span class="brand" aria-label="PADE is starting">◆</span>
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
      color: var(--primary);
      font-weight: 700;
      font-size: 44px;
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
