<script lang="ts">
  import { agents as agentsApi, pty, vcs, workspace } from "./lib/bridge";
  import Icon from "./lib/Icon.svelte";
  import IdeMenu from "./lib/IdeMenu.svelte";
  import { effective } from "./lib/prefs.svelte";
  import type { Agent, AgentSession, Settings } from "./lib/types";
  import ChangeFeed from "./panels/ChangeFeed.svelte";
  import Onboarding from "./panels/Onboarding.svelte";
  import ProjectPicker from "./panels/ProjectPicker.svelte";
  import Terminal from "./panels/Terminal.svelte";
  import { onMount } from "svelte";

  type Phase = "loading" | "project" | "onboarding" | "ready";
  let phase = $state<Phase>("loading");
  let agents = $state<Agent[]>([]);
  let settings = $state<Settings>({
    roots: [],
    defaultAgent: null,
    projectAgents: {},
    recentProjects: [],
    ownedWorkspaces: [],
    prefs: {}
  });
  let sessions = $state<AgentSession[]>([]);
  let activeId = $state<string | null>(null);
  let currentProject = $state<string>("");
  // Local branches when the project is a git repo — enables per-branch agents.
  let branches = $state<string[]>([]);
  // Carried through the agent picker so a new-project prompt survives onboarding.
  let pendingPrompt = $state<string | undefined>();

  // Agents excluding the always-present shell fallback — this count decides
  // whether we auto-launch or onboard.
  const realAgents = $derived(agents.filter(a => a.id !== "shell"));
  // The current directory, shown as the last couple of segments so it's legible
  // without eating the whole topbar (full path in the tooltip).
  const shortDir = $derived(
    currentProject.split(/[\\/]/).filter(Boolean).slice(-2).join("/")
  );
  // Temp workspaces live under the config dir as .../workspaces/temp-<stamp>.
  const isTemp = $derived(/[\\/]workspaces[\\/]temp-\d+$/.test(currentProject));
  // Active agent id — used to show only its relevant config files.
  const activeAgent = $derived(sessions.find(s => s.id === activeId)?.agent.id ?? "");

  onMount(async () => {
    const [ctx, detected, saved] = await Promise.all([
      workspace.context(),
      agentsApi.detect(),
      workspace.settings()
    ]);
    agents = detected;
    settings = saved;

    if (ctx.hasProject) {
      await workspace.open(ctx.cwd); // records it in recent history
      startAgentFlow(ctx.cwd);
      await loadBranches();
    } else if (saved.prefs.startMode === "picker") {
      // Opt-in: show the project picker instead of starting in a temp workspace.
      phase = "project";
    } else {
      // Default: start immediately in a throwaway workspace so there's no
      // blocking picker. The user can switch any time (Switch button).
      const temp = await workspace.temp();
      startAgentFlow(temp);
    }
  });

  // Show the project picker on demand to switch project / open recent / create.
  function switchProject() {
    phase = "project";
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
    phase = "onboarding";
  }

  function launch(opts: {
    agent: Agent;
    initialPrompt?: string;
    cwd?: string;
    branch?: string;
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
    pendingPrompt = undefined;
    phase = "ready";
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

    if (activeId === session.id) {
      activeId = sessions.at(-1)?.id ?? null;
    }

    if (sessions.length === 0) {
      phase = realAgents.length > 1 ? "onboarding" : "loading";

      if (phase === "loading") {
        launch({ agent: agents[0] });
      }
    }
  }

  // Side panels (lazy-loaded for tree-shaking).
  type Side = "feed" | "vcs" | "config" | null;
  let side = $state<Side>("feed");
  function toggleSide(panel: Exclude<Side, null>) {
    side = side === panel ? null : panel;
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
<svelte:window onfocus={() => void redetectAgents()} />

<!-- Font tokens bound declaratively; they cascade to every descendant. -->
<div style:--font-ui={effective.uiFamily} style:--font-mono={effective.monoFamily} class="app-root">
  {#if phase === "project"}
    <ProjectPicker {agents} onopen={openProject} />
  {:else if phase === "onboarding"}
    <Onboarding
      {agents} onpick={a => launch({
        agent: a,
        initialPrompt: pendingPrompt
      })} />
  {:else if phase === "ready"}
    <div class="shell">
      <header class="topbar">
        <span class="brand">◆ ADE</span>
        {#if currentProject}
          <button class="project-name" data-tooltip={currentProject} onclick={switchProject}>
            {#if isTemp}
              <span class="temp-badge">temp</span>
            {/if}
            <span class="dir">{shortDir}</span>
            <span class="switch-hint">switch</span>
          </button>
        {/if}

        <nav class="tabs" aria-label="Agent sessions">
          {#each sessions as s (s.id)}
            <div class="tab" class:active={s.id === activeId}>
              <button class="pick" onclick={() => (activeId = s.id)}>{s.agent.label}</button>
              <button
                class="x"
                aria-label="Close session"
                data-tooltip="Close session"
                onclick={() => close(s)}
              >×</button>
            </div>
          {/each}

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

        <div class="spacer"></div>

        <IdeMenu />

        <div class="seg" aria-label="Side panel" role="tablist">
          <button aria-selected={side === "feed"} onclick={() => toggleSide("feed")} role="tab">
            <Icon name="feed" /> Change Feed
          </button>
          <button aria-selected={side === "vcs"} onclick={() => toggleSide("vcs")} role="tab">
            <Icon name="git" /> Git
          </button>
          <button aria-selected={side === "config"} onclick={() => toggleSide("config")} role="tab">
            <Icon name="sliders" /> Config
          </button>
        </div>
      </header>

      <main class="body" class:with-side={side !== null}>
        <section class="pane term-pane">
          {#each sessions as s (s.id)}
            <div class="term-slot" class:hidden={s.id !== activeId}>
              <Terminal session={s} />
            </div>
          {/each}
        </section>

        {#if side !== null}
          <aside class="pane side-pane">
            {#if side === "feed"}
              <ChangeFeed />
            {:else if side === "vcs"}
              {#await import("./panels/VcsPanel.svelte") then { default: VcsPanel }}
                <VcsPanel />
              {/await}
            {:else if side === "config"}
              {#await import("./panels/ConfigPanel.svelte") then { default: ConfigPanel }}
                <ConfigPanel agent={activeAgent} />
              {/await}
            {/if}
          </aside>
        {/if}
      </main>

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
    gap: 12px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 16px;
    border-block-end: 1px solid var(--outline);
    background: var(--surface-1);
  }

  .brand {
    color: var(--primary);
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .project-name {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    padding-block: 4px;
    padding-inline: 10px;
    border: none;
    border-inline-start: 1px solid var(--outline);
    background: transparent;
    color: var(--on-surface-var);
    font-family: var(--font-mono);
    font-size: 13px;
    cursor: pointer;

    &:hover {
      color: var(--on-surface);
    }

    &:hover .switch-hint {
      opacity: 100%;
    }

    .dir {
      overflow: hidden;
      max-inline-size: 32ch;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .temp-badge {
      padding-inline: 6px;
      border-radius: 999px;
      background: var(--surface-3);
      color: var(--on-surface-var);
      font-size: 10px;
      letter-spacing: 0.06em;
      text-transform: uppercase;
    }

    .switch-hint {
      color: var(--primary);
      font-family: var(--font-ui);
      font-size: 11px;
      opacity: 0%;
      transition: opacity 150ms var(--ease);
    }
  }

  .spacer {
    flex: 1;
  }

  .tabs {
    display: flex;
    gap: 4px;
    align-items: center;

    .tab {
      display: flex;
      align-items: center;
      overflow: hidden;
      border-radius: 999px;
      background: var(--surface-2);

      &.active {
        background: var(--primary-container);
      }

      &.active .pick {
        color: var(--on-primary-container);
        font-weight: 600;
      }
    }

    .pick {
      padding-block: 6px;
      padding-inline: 14px 6px;
      border: none;
      background: transparent;
      color: var(--on-surface-var);
      font: inherit;
      font-size: 13px;
      cursor: pointer;
    }

    .x {
      padding-block: 6px;
      padding-inline: 6px 12px;
      border: none;
      background: transparent;
      color: var(--on-surface-var);
      font-size: 15px;
      line-height: 1;
      cursor: pointer;

      &:hover {
        color: var(--crit);
      }
    }
  }

  /* Pure-CSS dropdown via <details> (rule 9). */
  .add-btn {
    display: grid;
    place-items: center;
    block-size: 30px;
    inline-size: 30px;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-var);
    font-size: 18px;
    cursor: pointer;

    &:hover {
      color: var(--primary);
    }
  }

  /* Native popover (light-dismiss on outside click) anchored to its button. */
  .menu {
    position: absolute;
    inset: auto;
    position-area: bottom span-right;
    min-inline-size: 180px;
    margin-block: 6px 0;
    margin-inline: 0;
    padding: 6px;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
    background: var(--surface-2);
    list-style: none;
    box-shadow: 0 8px 24px color-mix(in sRGB, var(--on-surface) 20%, transparent);

    li button {
      display: flex;
      gap: 8px;
      align-items: center;
      inline-size: 100%;
      padding: 8px 10px;
      border: none;
      border-radius: var(--r-sm);
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      font-size: 13px;
      text-align: start;
      cursor: pointer;

      &:hover {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }
    }

    .menu-sep {
      margin-block: 6px 2px;
      padding-inline: 10px;
      color: var(--on-surface-var);
      font-size: 11px;
      letter-spacing: 0.06em;
      text-transform: uppercase;
    }
  }

  .seg {
    display: inline-flex;
    padding: 3px;
    border-radius: 999px;
    background: var(--surface-2);

    button {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      padding: 6px 14px;
      border: none;
      border-radius: 999px;
      background: transparent;
      color: var(--on-surface-var);
      font: inherit;
      font-weight: 600;
      font-size: 13px;
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
    border-inline-start: 1px solid var(--outline);
    background: var(--surface);
  }

  /* All sessions stay mounted so their scrollback survives switching; only the
     active one is shown. */
  .term-pane {
    position: relative;
  }

  .term-slot {
    position: absolute;
    inset: 0;
  }

  .term-slot.hidden {
    visibility: hidden;
    pointer-events: none;
  }

  @media (width <= 720px) {
    .body.with-side {
      grid-template-rows: 1fr 40%;
      grid-template-columns: 1fr;
    }
  }

  .send-fab {
    position: fixed;
    inset-block-end: 20px;
    inset-inline-start: 50%;
    display: inline-flex;
    gap: 10px;
    align-items: center;
    padding: 12px 20px;
    border: none;
    border-radius: 999px;
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 600;
    box-shadow: 0 6px 20px color-mix(in sRGB, var(--primary) 40%, transparent);
    cursor: pointer;
    translate: -50% 0;
    animation: pop 200ms var(--ease);

    .preview {
      overflow: hidden;
      max-inline-size: 40ch;
      font-family: var(--font-mono);
      font-weight: 400;
      font-size: 12px;
      text-overflow: ellipsis;
      white-space: nowrap;
      opacity: 85%;
    }
  }

  @keyframes pop {
    from {
      opacity: 0%;
      translate: -50% 8px;
    }

    to {
      opacity: 100%;
      translate: -50% 0;
    }
  }
</style>
