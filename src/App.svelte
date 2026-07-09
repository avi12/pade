<script lang="ts">
  import { agents as agentsApi, pty, workspace } from "./lib/bridge";
  import IdeMenu from "./lib/IdeMenu.svelte";
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
    prefs: {}
  });
  let sessions = $state<AgentSession[]>([]);
  let activeId = $state<string | null>(null);
  let currentProject = $state<string>("");
  // Carried through the agent picker so a new-project prompt survives onboarding.
  let pendingPrompt = $state<string | undefined>();

  // Agents excluding the always-present shell fallback — this count decides
  // whether we auto-launch or onboard.
  const realAgents = $derived(agents.filter(a => a.id !== "shell"));
  const projectName = $derived(currentProject.split(/[\\/]/).filter(Boolean).at(-1) ?? "");

  onMount(async () => {
    const [ctx, detected, saved] = await Promise.all([
      workspace.context(),
      agentsApi.detect(),
      workspace.settings()
    ]);
    agents = detected;
    settings = saved;

    if (ctx.hasProject) {
      startAgentFlow(ctx.cwd);
    } else {
      phase = "project";
    } // launched with no project → pick one
  });

  async function openProject(target: {
    path: string;
    initialPrompt?: string;
  }) {
    await workspace.open(target.path);
    startAgentFlow(target.path, target.initialPrompt);
  }

  // Decide how to enter a project: honor a saved per-project/default agent,
  // else auto-launch a lone agent, else onboard. (Reused for every entry path.)
  function startAgentFlow(path: string, initialPrompt?: string) {
    currentProject = path;
    const prefId = settings.projectAgents[path] ?? settings.defaultAgent ?? null;
    const preferred = prefId ? agents.find(a => a.id === prefId) : undefined;
    if (preferred) {
      return launch(preferred, initialPrompt);
    }

    if (realAgents.length === 1) {
      return launch(realAgents[0], initialPrompt);
    }

    if (realAgents.length === 0) {
      return launch(agents[0], initialPrompt);
    } // shell

    pendingPrompt = initialPrompt;
    phase = "onboarding";
  }

  function launch(agent: Agent, initialPrompt?: string) {
    const session: AgentSession = {
      id: crypto.randomUUID(),
      agent,
      initialPrompt
    };
    sessions.push(session);
    activeId = session.id;
    pendingPrompt = undefined;
    phase = "ready";
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
        launch(agents[0]);
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

    await pty.write(activeId, selection);
    selection = "";
    window.getSelection()?.removeAllRanges();
  }
</script>

<svelte:document onselectionchange={readSelection} />

{#if phase === "project"}
  <ProjectPicker {agents} onopen={openProject} />
{:else if phase === "onboarding"}
  <Onboarding {agents} onpick={a => launch(a, pendingPrompt)} />
{:else if phase === "ready"}
  <div class="shell">
    <header class="topbar">
      <span class="brand">◆ ADE</span>
      {#if currentProject}
        <span class="project-name" title={currentProject}>{projectName}</span>
      {/if}

      <nav class="tabs" aria-label="Agent sessions">
        {#each sessions as s (s.id)}
          <div class="tab" class:active={s.id === activeId}>
            <button class="pick" onclick={() => (activeId = s.id)}>{s.agent.label}</button>
            <button class="x" onclick={() => close(s)} title="Close session">×</button>
          </div>
        {/each}

        <details class="add">
          <summary title="Add an agent">+</summary>
          <ul>
            {#each agents as a (a.id)}
              <li><button onclick={() => launch(a)}>{a.label}</button></li>
            {/each}
          </ul>
        </details>
      </nav>

      <div class="spacer"></div>

      <IdeMenu />

      <div class="seg" aria-label="Side panel" role="tablist">
        <button aria-selected={side === "feed"} onclick={() => toggleSide("feed")} role="tab">Change Feed</button>
        <button aria-selected={side === "vcs"} onclick={() => toggleSide("vcs")} role="tab">Git</button>
        <button aria-selected={side === "config"} onclick={() => toggleSide("config")} role="tab">Config</button>
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
              <ConfigPanel />
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

<style>
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
    padding-inline: 10px;
    border-inline-start: 1px solid var(--outline);
    color: var(--on-surface-var);
    font-family: var(--font-mono);
    font-size: 13px;
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
  .add {
    position: relative;

    summary {
      display: grid;
      place-items: center;
      block-size: 30px;
      inline-size: 30px;
      border-radius: 999px;
      background: var(--surface-2);
      color: var(--on-surface-var);
      list-style: none;
      font-size: 18px;
      cursor: pointer;
      user-select: none;
    }

    summary::-webkit-details-marker {
      display: none;
    }

    summary:hover {
      color: var(--primary);
    }

    ul {
      position: absolute;
      inset-block-start: calc(100% + 6px);
      inset-inline-start: 0;
      z-index: 10;
      min-inline-size: 180px;
      margin: 0;
      padding: 6px;
      border: 1px solid var(--outline);
      border-radius: var(--r-md);
      background: var(--surface-2);
      list-style: none;
      box-shadow: 0 8px 24px color-mix(in sRGB, var(--on-surface) 20%, transparent);
    }

    li button {
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
  }

  .seg {
    display: inline-flex;
    padding: 3px;
    border-radius: 999px;
    background: var(--surface-2);

    button {
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
