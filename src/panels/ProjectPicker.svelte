<script lang="ts">
  import { onMount } from "svelte";
  import { workspace } from "../lib/bridge";
  import type { Agent, ProjectEntry, Settings } from "../lib/types";

  // Shown when the app wasn't launched inside a project. Manage root folders,
  // browse the projects inside them, open one, or create a new one — then hand
  // the chosen project path (and optional first prompt) back to the app.
  let {
    agents,
    onopen,
  }: {
    agents: Agent[];
    onopen: (p: { path: string; initialPrompt?: string }) => void;
  } = $props();

  let settings = $state<Settings>({ roots: [], defaultAgent: null, projectAgents: {}, prefs: {} });
  let projectsByRoot = $state<Record<string, ProjectEntry[]>>({});
  let newRoot = $state("");
  let createIn = $state("");
  let createName = $state("");
  let createPrompt = $state("");

  const realAgents = $derived(agents.filter((a) => a.id !== "shell"));

  async function refresh() {
    settings = await workspace.settings();
    projectsByRoot = Object.fromEntries(
      await Promise.all(settings.roots.map(async (r) => [r, await scan(r)] as const)),
    );
  }
  const scan = (root: string) => workspace.scan(root).catch(() => [] as ProjectEntry[]);

  async function addRoot() {
    const path = newRoot.trim();
    if (!path) return;
    settings = await workspace.addRoot(path);
    projectsByRoot = { ...projectsByRoot, [path]: await scan(path) };
    newRoot = "";
  }
  async function removeRoot(path: string) {
    settings = await workspace.removeRoot(path);
    const { [path]: _drop, ...rest } = projectsByRoot;
    projectsByRoot = rest;
  }

  async function setMaster(agentId: string) {
    settings = await workspace.setDefaultAgent(agentId);
  }

  async function create() {
    if (!createIn || !createName.trim()) return;
    const path = await workspace.create(createIn, createName.trim());
    onopen({ path, initialPrompt: createPrompt.trim() || undefined });
  }

  onMount(refresh);
</script>

<div class="picker">
  <div class="inner">
    <header>
      <span class="brand">◆ ADE</span>
      <h1>Open a project</h1>
      <p class="lede">
        Add the folders your projects live in, then open one — or start a new
        project with a first prompt for the agent.
      </p>
    </header>

    {#if realAgents.length > 1}
      <section class="master">
        <h2>Default agent</h2>
        <p class="hint">Used for every project unless overridden. Switches all at once.</p>
        <div class="chips">
          {#each realAgents as a (a.id)}
            <button
              class="chip"
              class:on={settings.defaultAgent === a.id}
              onclick={() => setMaster(a.id)}
            >{a.label}</button>
          {/each}
        </div>
      </section>
    {/if}

    <section class="roots">
      <h2>Root folders</h2>
      <form class="addrow" onsubmit={(e) => { e.preventDefault(); addRoot(); }}>
        <input
          type="text"
          bind:value={newRoot}
          placeholder="C:\repositories  ·  paste a folder path"
          spellcheck="false"
        />
        <button type="submit" disabled={!newRoot.trim()}>Add root</button>
      </form>

      {#each settings.roots as root (root)}
        <div class="root">
          <div class="root-head">
            <code class="rootpath">{root}</code>
            <button class="remove" title="Remove root" onclick={() => removeRoot(root)}>×</button>
          </div>
          <ul class="projects">
            {#each projectsByRoot[root] ?? [] as p (p.path)}
              <li>
                <button class="project" onclick={() => onopen({ path: p.path })}>
                  <span class="pname">{p.name}</span>
                  {#if p.isGit}<span class="git">git</span>{/if}
                </button>
              </li>
            {:else}
              <li class="none">No projects found in this folder.</li>
            {/each}
          </ul>
        </div>
      {/each}
    </section>

    {#if settings.roots.length}
      <section class="create">
        <h2>New project</h2>
        <div class="createform">
          <select bind:value={createIn} aria-label="Root folder">
            <option value="" disabled>Choose a root…</option>
            {#each settings.roots as root (root)}
              <option value={root}>{root}</option>
            {/each}
          </select>
          <input type="text" bind:value={createName} placeholder="project-name" spellcheck="false" />
          <textarea
            bind:value={createPrompt}
            rows="3"
            placeholder="First prompt for the agent (optional) — e.g. “scaffold a SvelteKit app with auth”"
          ></textarea>
          <button class="go" disabled={!createIn || !createName.trim()} onclick={create}>
            Create &amp; open
          </button>
        </div>
      </section>
    {/if}
  </div>
</div>

<style>
  .picker {
    block-size: 100%;
    overflow-y: auto;
    background: radial-gradient(120% 70% at 50% 0%, var(--surface-1), var(--surface));
  }
  .inner { inline-size: min(680px, 100%); margin-inline: auto; padding: 48px 24px 64px; display: flex; flex-direction: column; gap: 28px; }

  header {
    .brand { font-weight: 700; color: var(--primary); letter-spacing: 0.02em; }
    h1 { margin: 12px 0 8px; font-size: clamp(26px, 4vw, 36px); letter-spacing: -0.02em; text-wrap: balance; }
    .lede { margin: 0; color: var(--on-surface-var); max-inline-size: 52ch; }
  }

  h2 { margin: 0 0 4px; font-size: 15px; }
  .hint { margin: 0 0 12px; font-size: 13px; color: var(--on-surface-var); }

  .chips, .addrow, .createform { display: flex; gap: 8px; flex-wrap: wrap; }
  .chip {
    font: inherit; font-size: 13px; font-weight: 600;
    padding: 8px 16px; border-radius: 999px; cursor: pointer;
    background: var(--surface-2); color: var(--on-surface-var); border: 1px solid transparent;
    &.on { background: var(--primary-container); color: var(--on-primary-container); border-color: var(--primary); }
  }

  input, select, textarea {
    font: inherit; font-size: 14px;
    color: var(--on-surface); background: var(--surface-2);
    border: 1px solid var(--outline); border-radius: var(--r-md);
    padding: 10px 12px;
  }
  input[type="text"] { flex: 1; min-inline-size: 220px; font-family: var(--font-mono); font-size: 13px; }
  textarea { inline-size: 100%; resize: vertical; }

  button {
    font: inherit; font-weight: 600; cursor: pointer;
    background: var(--primary); color: var(--on-primary);
    border: none; border-radius: var(--r-md); padding: 10px 18px;
    &:disabled { opacity: 0.5; cursor: default; }
  }

  .roots .root { margin-block-start: 14px; }
  .root-head { display: flex; align-items: center; gap: 8px; }
  .rootpath { font-family: var(--font-mono); font-size: 12px; color: var(--on-surface-var); }
  .remove {
    margin-inline-start: auto; background: transparent; color: var(--on-surface-var);
    padding: 2px 8px; font-size: 16px;
    &:hover { color: var(--crit); }
  }

  .projects { list-style: none; margin: 8px 0 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 8px; }
  .project {
    inline-size: 100%; text-align: start; display: flex; align-items: center; gap: 8px;
    background: var(--surface-2); color: var(--on-surface); border-radius: var(--r-md); padding: 12px 14px;
    &:hover { background: var(--primary-container); color: var(--on-primary-container); }
    .pname { font-weight: 600; font-size: 14px; }
    .git { margin-inline-start: auto; font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: var(--tertiary); }
  }
  .none { list-style: none; font-size: 13px; color: var(--on-surface-var); }

  .createform { flex-direction: column; }
  .createform .go { align-self: start; }
</style>
