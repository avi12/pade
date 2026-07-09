<script lang="ts">
  import { onMount } from "svelte";
  import Terminal from "./panels/Terminal.svelte";
  import ChangeFeed from "./panels/ChangeFeed.svelte";
  import Onboarding from "./panels/Onboarding.svelte";
  import ProjectPicker from "./panels/ProjectPicker.svelte";
  import IdeMenu from "./lib/IdeMenu.svelte";
  import { agents as agentsApi, pty, workspace } from "./lib/bridge";
  import type { Agent, AgentSession, Settings } from "./lib/types";

  type Phase = "loading" | "project" | "onboarding" | "ready";
  let phase = $state<Phase>("loading");
  let agents = $state<Agent[]>([]);
  let settings = $state<Settings>({ roots: [], defaultAgent: null, projectAgents: {} });
  let sessions = $state<AgentSession[]>([]);
  let activeId = $state<string | null>(null);
  let currentProject = $state<string>("");
  // Carried through the agent picker so a new-project prompt survives onboarding.
  let pendingPrompt = $state<string | undefined>();

  // Agents excluding the always-present shell fallback — this count decides
  // whether we auto-launch or onboard.
  const realAgents = $derived(agents.filter((a) => a.id !== "shell"));
  const projectName = $derived(currentProject.split(/[\\/]/).filter(Boolean).at(-1) ?? "");

  onMount(async () => {
    const [ctx, detected, saved] = await Promise.all([
      workspace.context(),
      agentsApi.detect(),
      workspace.settings(),
    ]);
    agents = detected;
    settings = saved;
    if (ctx.hasProject) startAgentFlow(ctx.cwd);
    else phase = "project"; // launched with no project → pick one
  });

  async function openProject(p: { path: string; initialPrompt?: string }) {
    await workspace.open(p.path);
    startAgentFlow(p.path, p.initialPrompt);
  }

  // Decide how to enter a project: honor a saved per-project/default agent,
  // else auto-launch a lone agent, else onboard. (Reused for every entry path.)
  function startAgentFlow(path: string, initialPrompt?: string) {
    currentProject = path;
    const prefId = settings.projectAgents[path] ?? settings.defaultAgent ?? null;
    const preferred = prefId ? agents.find((a) => a.id === prefId) : undefined;
    if (preferred) return launch(preferred, initialPrompt);
    if (realAgents.length === 1) return launch(realAgents[0], initialPrompt);
    if (realAgents.length === 0) return launch(agents[0], initialPrompt); // shell
    pendingPrompt = initialPrompt;
    phase = "onboarding";
  }

  function launch(agent: Agent, initialPrompt?: string) {
    const session: AgentSession = { id: crypto.randomUUID(), agent, initialPrompt };
    sessions.push(session);
    activeId = session.id;
    pendingPrompt = undefined;
    phase = "ready";
  }

  async function close(session: AgentSession) {
    await pty.kill(session.id);
    sessions = sessions.filter((s) => s.id !== session.id);
    if (activeId === session.id) activeId = sessions.at(-1)?.id ?? null;
    if (sessions.length === 0) {
      phase = realAgents.length > 1 ? "onboarding" : "loading";
      if (phase === "loading") launch(agents[0]);
    }
  }

  // Side panels (lazy-loaded for tree-shaking).
  type Side = "feed" | "vcs" | "config" | null;
  let side = $state<Side>("feed");
  const toggleSide = (p: Exclude<Side, null>) => (side = side === p ? null : p);

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
    if (!selection || !activeId) return;
    await pty.write(activeId, selection);
    selection = "";
    window.getSelection()?.removeAllRanges();
  }
</script>

<svelte:document onselectionchange={readSelection} />

{#if phase === "project"}
  <ProjectPicker {agents} onopen={openProject} />
{:else if phase === "onboarding"}
  <Onboarding {agents} onpick={(a) => launch(a, pendingPrompt)} />
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
            <button class="x" title="Close session" onclick={() => close(s)}>×</button>
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

      <div class="seg" role="tablist" aria-label="Side panel">
        <button role="tab" aria-selected={side === "feed"} onclick={() => toggleSide("feed")}>Change Feed</button>
        <button role="tab" aria-selected={side === "vcs"} onclick={() => toggleSide("vcs")}>Git</button>
        <button role="tab" aria-selected={side === "config"} onclick={() => toggleSide("config")}>Config</button>
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
        <span class="preview">{selection.length > 40 ? selection.slice(0, 40) + "…" : selection}</span>
      </button>
    {/if}
  </div>
{/if}

<style>
  .shell { display: flex; flex-direction: column; block-size: 100%; }

  .topbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-block: 8px;
    padding-inline: 16px;
    background: var(--surface-1);
    border-block-end: 1px solid var(--outline);
  }
  .brand { font-weight: 700; color: var(--primary); letter-spacing: 0.02em; }
  .project-name {
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--on-surface-var);
    padding-inline: 10px;
    border-inline-start: 1px solid var(--outline);
  }
  .spacer { flex: 1; }

  .tabs {
    display: flex;
    align-items: center;
    gap: 4px;

    .tab {
      display: flex;
      align-items: center;
      background: var(--surface-2);
      border-radius: 999px;
      overflow: hidden;

      &.active { background: var(--primary-container); }
      &.active .pick { color: var(--on-primary-container); font-weight: 600; }
    }
    .pick {
      font: inherit;
      font-size: 13px;
      color: var(--on-surface-var);
      background: transparent;
      border: none;
      padding: 6px 6px 6px 14px;
      cursor: pointer;
    }
    .x {
      font-size: 15px;
      line-height: 1;
      color: var(--on-surface-var);
      background: transparent;
      border: none;
      padding: 6px 12px 6px 6px;
      cursor: pointer;

      &:hover { color: var(--crit); }
    }
  }

  /* Pure-CSS dropdown via <details> (rule 9). */
  .add {
    position: relative;

    summary {
      list-style: none;
      display: grid;
      place-items: center;
      inline-size: 30px;
      block-size: 30px;
      border-radius: 999px;
      background: var(--surface-2);
      color: var(--on-surface-var);
      font-size: 18px;
      cursor: pointer;
      user-select: none;
    }
    summary::-webkit-details-marker { display: none; }
    summary:hover { color: var(--primary); }

    ul {
      position: absolute;
      inset-block-start: calc(100% + 6px);
      inset-inline-start: 0;
      z-index: 10;
      min-inline-size: 180px;
      margin: 0;
      padding: 6px;
      list-style: none;
      background: var(--surface-2);
      border: 1px solid var(--outline);
      border-radius: var(--r-md);
      box-shadow: 0 8px 24px color-mix(in srgb, var(--on-surface) 20%, transparent);
    }
    li button {
      inline-size: 100%;
      text-align: start;
      font: inherit;
      font-size: 13px;
      color: var(--on-surface);
      background: transparent;
      border: none;
      padding: 8px 10px;
      border-radius: var(--r-sm);
      cursor: pointer;

      &:hover { background: var(--primary-container); color: var(--on-primary-container); }
    }
  }

  .seg {
    display: inline-flex;
    background: var(--surface-2);
    border-radius: 999px;
    padding: 3px;

    button {
      font: inherit;
      font-size: 13px;
      font-weight: 600;
      color: var(--on-surface-var);
      background: transparent;
      border: none;
      padding: 6px 14px;
      border-radius: 999px;
      cursor: pointer;
      transition: background 0.2s var(--ease), color 0.2s var(--ease);

      &[aria-selected="true"] {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }
    }
  }

  .body {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr;
    min-block-size: 0;

    &.with-side { grid-template-columns: 1fr minmax(320px, 420px); }
  }
  .pane { min-block-size: 0; min-inline-size: 0; overflow: hidden; }
  .side-pane { border-inline-start: 1px solid var(--outline); background: var(--surface); }

  /* All sessions stay mounted so their scrollback survives switching; only the
     active one is shown. */
  .term-pane { position: relative; }
  .term-slot { position: absolute; inset: 0; }
  .term-slot.hidden { visibility: hidden; pointer-events: none; }

  @media (max-width: 720px) {
    .body.with-side { grid-template-columns: 1fr; grid-template-rows: 1fr 40%; }
  }

  .send-fab {
    position: fixed;
    inset-block-end: 20px;
    inset-inline-start: 50%;
    translate: -50% 0;
    display: inline-flex;
    align-items: center;
    gap: 10px;
    font: inherit;
    font-weight: 600;
    color: var(--on-primary);
    background: var(--primary);
    border: none;
    padding: 12px 20px;
    border-radius: 999px;
    box-shadow: 0 6px 20px color-mix(in srgb, var(--primary) 40%, transparent);
    cursor: pointer;
    animation: pop 0.2s var(--ease);

    .preview {
      font-family: var(--font-mono);
      font-size: 12px;
      font-weight: 400;
      opacity: 0.85;
      max-inline-size: 40ch;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }
  @keyframes pop {
    from { opacity: 0; translate: -50% 8px; }
    to { opacity: 1; translate: -50% 0; }
  }
</style>
