<script lang="ts">
  import { ide, os, workspace } from "../lib/bridge";
  import Icon from "../lib/Icon.svelte";
  import type {
    Agent,
    Ide,
    ProjectEntry,
    Settings,
    StartMode
  } from "../lib/types";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";

  // Shown when the app wasn't launched inside a project. Manage root folders,
  // browse the projects inside them, open one, or create a new one — then hand
  // the chosen project path (and optional first prompt) back to the app.
  const {
    agents,
    onopen
  }: {
    agents: Agent[];
    onopen: (target: {
      path: string;
      initialPrompt?: string;
    }) => void;
  } = $props();

  let settings = $state<Settings>({
    roots: [],
    defaultAgent: null,
    projectAgents: {},
    recentProjects: [],
    prefs: {}
  });
  let projectsByRoot = $state<Record<string, ProjectEntry[]>>({});
  let ides = $state<Ide[]>([]);
  let newRoot = $state("");
  let createIn = $state("");
  let createName = $state("");
  let createPrompt = $state("");

  const realAgents = $derived(agents.filter(a => a.id !== "shell"));
  const startMode = $derived(settings.prefs.startMode ?? "temp");

  async function refresh() {
    [settings, ides] = await Promise.all([workspace.settings(), ide.detect()]);
    projectsByRoot = Object.fromEntries(
      await Promise.all(settings.roots.map(async root => [root, await scan(root)] as const))
    );
  }

  async function setStartMode(mode: StartMode) {
    settings = await workspace.setPrefs({
      ...settings.prefs,
      startMode: mode
    });
  }
  function scan(root: string): Promise<ProjectEntry[]> {
    return workspace.scan(root).catch((): ProjectEntry[] => []);
  }

  async function addRoot() {
    const path = newRoot.trim();
    if (!path) {
      return;
    }

    settings = await workspace.addRoot(path);
    projectsByRoot = {
      ...projectsByRoot,
      [path]: await scan(path)
    };
    newRoot = "";
  }
  async function removeRoot(path: string) {
    settings = await workspace.removeRoot(path);
    const { [path]: _drop, ...rest } = projectsByRoot;
    projectsByRoot = rest;
  }

  // Native folder picker (Tauri dialog) — nicer than pasting a path.
  async function browseRoot() {
    const picked = await openDialog({
      directory: true,
      multiple: false
    });
    if (typeof picked === "string") {
      newRoot = picked;
      await addRoot();
    }
  }

  async function clearRecent() {
    settings = await workspace.clearRecent();
  }

  async function setMaster(agentId: string) {
    settings = await workspace.setDefaultAgent(agentId);
  }

  // Start immediately in a throwaway workspace.
  async function startTemp() {
    const path = await workspace.temp();
    onopen({ path });
  }

  function basename(path: string): string {
    return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
  }
  function isTempPath(path: string): boolean {
    return /[\\/]workspaces[\\/]temp-\d+$/.test(path);
  }

  async function create() {
    if (!createIn || !createName.trim()) {
      return;
    }

    const path = await workspace.create({
      root: createIn,
      name: createName.trim()
    });
    onopen({
      path,
      initialPrompt: createPrompt.trim() || undefined
    });
  }

  onMount(refresh);
</script>

{#snippet openActions(path: string)}
  <span class="row-actions">
    <button aria-label="Open in file explorer" onclick={() => void os.explorer(path)}>
      <Icon name="folder" /> Files
    </button>
    <button aria-label="Open in terminal" onclick={() => void os.terminal(path)}>
      <Icon name="terminal" /> Terminal
    </button>
    {#if ides.length > 0}
      <button
        aria-label="Open in {ides[0].label}" onclick={() => void ide.open({
          command: ides[0].command,
          path
        })}>
        <Icon name="code" /> {ides[0].label}
      </button>
    {/if}
  </span>
{/snippet}

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

    <section class="quick">
      <button class="temp-start" onclick={startTemp}>
        <span class="ico">✦</span>
        <span class="txt">
          <strong>Start in a temp workspace</strong>
          <small>Jump straight in — switch to a real project any time.</small>
        </span>
      </button>
      <div class="startmode">
        <span class="sm-label">On launch with no project</span>
        <div class="sm-toggle">
          <button class="sm-btn" class:on={startMode === "temp"} onclick={() => setStartMode("temp")}>Temp workspace</button>
          <button class="sm-btn" class:on={startMode === "picker"} onclick={() => setStartMode("picker")}>This picker</button>
        </div>
      </div>
    </section>

    {#if settings.recentProjects.length > 0}
      <section class="recent">
        <div class="recent-head">
          <h2>Recent</h2>
          <button class="clear" onclick={clearRecent}><Icon name="trash" /> Clear</button>
        </div>
        <ul class="recent-list">
          {#each settings.recentProjects as path (path)}
            <li class="row">
              <button class="recent-item" onclick={() => onopen({ path })}>
                {#if isTempPath(path)}
                  <span class="temp-tag">temp</span>
                {/if}
                <span class="rname">{basename(path)}</span>
                <span class="rpath">{path}</span>
              </button>
              {@render openActions(path)}
            </li>
          {/each}
        </ul>
      </section>
    {/if}

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
      <form
        class="addrow" onsubmit={e => {
          e.preventDefault(); addRoot();
        }}>
        <input
          placeholder="C:\repositories  ·  paste a folder path"
          spellcheck="false"
          type="text"
          bind:value={newRoot}
        />
        <button class="browse" onclick={browseRoot} type="button"><Icon name="folder" /> Browse…</button>
        <button disabled={!newRoot.trim()} type="submit">Add root</button>
      </form>

      {#each settings.roots as root (root)}
        <div class="root">
          <div class="root-head">
            <code class="rootpath">{root}</code>
            <button
              class="remove"
              aria-label="Remove root"
              data-tooltip="Remove root"
              onclick={() => removeRoot(root)}
            >×</button>
          </div>
          <ul class="projects">
            {#each projectsByRoot[root] ?? [] as p (p.path)}
              <li class="row">
                <button class="project" onclick={() => onopen({ path: p.path })}>
                  <span class="pname">{p.name}</span>
                  {#if p.isGit}
                    <span class="git">git</span>
                  {/if}
                </button>
                {@render openActions(p.path)}
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
          <select aria-label="Root folder" bind:value={createIn}>
            <option disabled value="">Choose a root…</option>
            {#each settings.roots as root (root)}
              <option value={root}>{root}</option>
            {/each}
          </select>
          <input placeholder="project-name" spellcheck="false" type="text" bind:value={createName} />
          <textarea
            placeholder="First prompt for the agent (optional) — e.g. “scaffold a SvelteKit app with auth”"
            rows="3"
            bind:value={createPrompt}
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
    overflow-y: auto;
    block-size: 100%;
    background: radial-gradient(120% 70% at 50% 0%, var(--surface-1), var(--surface));
  }

  .inner {
    display: flex;
    flex-direction: column;
    gap: 28px;
    inline-size: min(680px, 100%);
    margin-inline: auto;
    padding-block: 48px 64px;
    padding-inline: 24px;
  }

  header {
    .brand {
      color: var(--primary);
      font-weight: 700;
      letter-spacing: 0.02em;
    }

    h1 {
      margin-block: 12px 8px;
      margin-inline: 0;
      font-size: clamp(26px, 4vw, 36px);
      letter-spacing: -0.02em;
      text-wrap: balance;
    }

    .lede {
      max-inline-size: 52ch;
      margin: 0;
      color: var(--on-surface-var);
    }
  }

  .temp-start {
    display: flex;
    gap: 14px;
    align-items: center;
    inline-size: 100%;
    padding: 16px 18px;
    border: 1px solid var(--primary);
    border-radius: var(--r-lg);
    background: var(--primary-container);
    color: var(--on-primary-container);
    text-align: start;
    cursor: pointer;
    transition: filter 150ms var(--ease);

    &:hover {
      filter: brightness(1.05);
    }

    .ico {
      font-size: 20px;
    }

    .txt {
      display: flex;
      flex-direction: column;
      gap: 2px;
    }

    small {
      color: var(--on-primary-container);
      font-size: 12px;
      opacity: 80%;
    }
  }

  .recent-head {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: baseline;
  }

  .clear {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    border: none;
    background: transparent;
    color: var(--on-surface-var);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition: color 150ms var(--ease);

    &:hover {
      color: var(--crit);
    }
  }

  .browse {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }

  .recent-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .row > button:first-child {
    flex: 1;
    min-inline-size: 0;
  }

  .row-actions {
    display: flex;
    gap: 4px;
    margin-inline-start: auto;
    opacity: 0%;
    transition: opacity 150ms var(--ease);

    button {
      display: inline-flex;
      gap: 4px;
      align-items: center;
      padding: 3px 9px;
      border: 1px solid var(--outline);
      border-radius: 999px;
      background: var(--surface-2);
      color: var(--on-surface-var);
      font: inherit;
      font-size: 11px;
      cursor: pointer;
    }

    button:hover {
      color: var(--primary);
    }
  }

  .row:hover .row-actions {
    opacity: 100%;
  }

  .startmode {
    display: flex;
    gap: 10px;
    align-items: center;
    margin-block-start: 12px;
  }

  .sm-label {
    color: var(--on-surface-var);
    font-size: 12px;
  }

  .sm-toggle {
    display: inline-flex;
    padding: 2px;
    border-radius: 999px;
    background: var(--surface-2);

    .sm-btn {
      padding: 4px 12px;
      border: none;
      border-radius: 999px;
      background: transparent;
      color: var(--on-surface-var);
      font: inherit;
      font-size: 12px;
      cursor: pointer;

      &.on {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }
    }
  }

  .recent-item {
    display: flex;
    gap: 10px;
    align-items: baseline;
    inline-size: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--on-surface);
    text-align: start;
    cursor: pointer;

    &:hover {
      background: var(--surface-2);
    }

    .rname {
      font-family: var(--font-mono);
      font-size: 13px;
    }

    .rpath {
      overflow: hidden;
      color: var(--on-surface-var);
      font-size: 11px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .temp-tag {
    padding-inline: 6px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-var);
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  h2 {
    margin-block: 0 4px;
    margin-inline: 0;
    font-size: 15px;
  }

  .hint {
    margin-block: 0 12px;
    margin-inline: 0;
    color: var(--on-surface-var);
    font-size: 13px;
  }

  .chips,
  .addrow,
  .createform {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .chip {
    padding: 8px 16px;
    border: 1px solid transparent;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-var);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;

    &.on {
      border-color: var(--primary);
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  input,
  select,
  textarea {
    padding: 10px 12px;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-size: 14px;
  }

  input {
    flex: 1;
    min-inline-size: 220px;
    font-family: var(--font-mono);
    font-size: 13px;
  }

  textarea {
    inline-size: 100%;
    resize: vertical;
  }

  button {
    padding: 10px 18px;
    border: none;
    border-radius: var(--r-md);
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 600;
    cursor: pointer;

    &:disabled {
      opacity: 50%;
      cursor: default;
    }
  }

  .roots .root {
    margin-block-start: 14px;
  }

  .root-head {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .rootpath {
    color: var(--on-surface-var);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .remove {
    margin-inline-start: auto;
    padding: 2px 8px;
    background: transparent;
    color: var(--on-surface-var);
    font-size: 16px;

    &:hover {
      color: var(--crit);
    }
  }

  .projects {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-block: 8px 0;
    margin-inline: 0;
    padding: 0;
    list-style: none;
  }

  .project {
    display: flex;
    gap: 8px;
    align-items: center;
    inline-size: 100%;
    padding: 12px 14px;
    border-radius: var(--r-md);
    background: var(--surface-2);
    color: var(--on-surface);
    text-align: start;

    &:hover {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    .pname {
      font-weight: 600;
      font-size: 14px;
    }

    .git {
      margin-inline-start: auto;
      color: var(--tertiary);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.06em;
      text-transform: uppercase;
    }
  }

  .none {
    color: var(--on-surface-var);
    list-style: none;
    font-size: 13px;
  }

  .createform {
    flex-direction: column;
  }

  .createform .go {
    align-self: start;
  }
</style>
