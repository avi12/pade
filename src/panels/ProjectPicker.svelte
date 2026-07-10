<script lang="ts">
  import { contextMenu, ide, os, workspace } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import { SHELL_AGENT_ID, StartMode } from "@/lib/types";
  import type { Agent, Ide, ProjectEntry, Settings } from "@/lib/types";
  import { ask, open as openDialog } from "@tauri-apps/plugin-dialog";
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
    ownedWorkspaces: [],
    labels: {},
    prefs: {}
  });
  let projectsByRoot = $state<Record<string, ProjectEntry[]>>({});
  let ides = $state<Ide[]>([]);
  // Primary detected kind of the current dir, so we can tag "this project"'s row.
  let currentKind = $state<string | null>(null);
  let newRoot = $state("");
  let createIn = $state("");
  let createName = $state("");
  let createPrompt = $state("");

  const realAgents = $derived(agents.filter(a => a.id !== SHELL_AGENT_ID));
  const startMode = $derived(settings.prefs.startMode ?? StartMode.enum.temp);
  const autoName = $derived(settings.prefs.autoNameTemp !== false);

  // Editor-rules engine — fixed, priority-ordered project kinds. A rule maps a
  // kind to an editor id; unmatched folders use the fallback. One row per kind.
  const EDITOR_KINDS = [
    {
      kind: "web",
      label: "Web / JavaScript"
    },
    {
      kind: "python",
      label: "Python"
    },
    {
      kind: "java",
      label: "Java"
    },
    {
      kind: "go",
      label: "Go"
    },
    {
      kind: "rust",
      label: "Rust"
    },
    {
      kind: "android",
      label: "Android"
    }
  ] as const;

  // Rules/fallback live in prefs; a missing map is treated as no rules.
  const ideRules = $derived(settings.prefs.ideRules ?? {});
  const ideFallback = $derived(settings.prefs.ideFallback ?? ides[0]?.id ?? "");

  function editorLabel(editorId: string): string {
    return ides.find(i => i.id === editorId)?.label ?? "Choose…";
  }
  // Stable, valid popover id/anchor per editor select (kind or "fallback").
  function editorSelectId(key: string): string {
    return `ide-${key.replaceAll(/[^a-zA-Z0-9]/g, "-")}`;
  }

  async function setEditorRule({ kind, editorId }: {
    kind: string;
    editorId: string;
  }) {
    settings = await workspace.setPrefs({
      ...settings.prefs,
      ideRules: {
        ...ideRules,
        [kind]: editorId
      }
    });
  }
  async function setEditorFallback(editorId: string) {
    settings = await workspace.setPrefs({
      ...settings.prefs,
      ideFallback: editorId
    });
  }

  async function refresh() {
    [settings, ides, currentKind] = await Promise.all([
      workspace.settings(),
      ide.detect(),
      ide.projectKind().catch(() => null)
    ]);
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
  async function setAutoName(on: boolean) {
    settings = await workspace.setPrefs({
      ...settings.prefs,
      autoNameTemp: on
    });
  }

  // Explorer "Open in PADE" folder context menu (Windows-only, per-user).
  const isWindows = navigator.userAgent.includes("Windows");
  let ctxMenuOn = $state(false);
  async function loadCtxMenu() {
    if (isWindows) {
      ctxMenuOn = await contextMenu.status();
    }
  }
  async function setCtxMenu(on: boolean) {
    if (on) {
      await contextMenu.register();
    } else {
      await contextMenu.unregister();
    }

    ctxMenuOn = await contextMenu.status();
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
  // Prefer a friendly auto-derived label over the raw folder name.
  function displayName(path: string): string {
    return settings.labels[path] ?? basename(path);
  }
  function isTempPath(path: string): boolean {
    return /[\\/]workspaces[\\/]temp-\d+$/.test(path);
  }
  function isOwned(path: string): boolean {
    // Temp dirs are ADE-created even if predating owned-workspace tracking.
    return settings.ownedWorkspaces.includes(path) || isTempPath(path);
  }
  // Stable, valid popover id/anchor per row (paths are unique).
  function menuId(path: string): string {
    return `m${path.replaceAll(/[^a-zA-Z0-9]/g, "-")}`;
  }

  // Owned-workspace lifecycle: delete, move (→ permanent, still deletable),
  // rename (→ promoted into the primary project root).
  let renaming = $state<string | null>(null);
  let renameValue = $state("");

  async function deleteWorkspace(path: string) {
    const ok = await ask(`Delete this workspace and its files?\n\n${path}`, {
      title: "Delete workspace",
      kind: "warning"
    });
    if (!ok) {
      return;
    }

    settings = await workspace.delete(path);
  }

  async function moveWorkspace(path: string) {
    const dest = await openDialog({
      directory: true,
      multiple: false
    });
    if (typeof dest !== "string") {
      return;
    }

    await workspace.move({
      from: path,
      destDir: dest
    });
    await refresh();
  }

  function startRename(path: string) {
    renaming = path;
    renameValue = basename(path);
  }

  async function commitRename(path: string) {
    if (!renameValue.trim()) {
      return;
    }

    await workspace.rename({
      from: path,
      newName: renameValue.trim()
    });
    renaming = null;
    await refresh();
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

  onMount(() => {
    void refresh();
    void loadCtxMenu();
  });
</script>

{#snippet rowMenu(path: string)}
  <button
    style:anchor-name="--{menuId(path)}"
    class="kebab"
    aria-label="Project actions"
    popovertarget={menuId(path)}
  >⋯</button>
  <ul id={menuId(path)} style:position-anchor="--{menuId(path)}" class="menu" popover>
    <li>
      <button onclick={() => void os.explorer(path)} popovertarget={menuId(path)} popovertargetaction="hide">
        <Icon name="folder" /> Open in Files
      </button>
    </li>
    <li>
      <button onclick={() => void os.terminal(path)} popovertarget={menuId(path)} popovertargetaction="hide">
        <Icon name="terminal" /> Open in Terminal
      </button>
    </li>
    {#if ides.length > 0}
      <li>
        <button
          onclick={() => void ide.open({
            command: ides[0].command,
            path
          })}
          popovertarget={menuId(path)}
          popovertargetaction="hide"
        >
          <Icon name="code" /> Open in {ides[0].label}
        </button>
      </li>
    {/if}
    {#if isOwned(path)}
      <li class="sep">
        <button onclick={() => startRename(path)} popovertarget={menuId(path)} popovertargetaction="hide">
          <Icon name="code" /> Rename to a project
        </button>
      </li>
      <li>
        <button onclick={async () => await moveWorkspace(path)} popovertarget={menuId(path)} popovertargetaction="hide">
          <Icon name="folder" /> Move…
        </button>
      </li>
      <li>
        <button
          class="danger"
          onclick={async () => await deleteWorkspace(path)}
          popovertarget={menuId(path)}
          popovertargetaction="hide"
        >
          <Icon name="trash" /> Delete workspace
        </button>
      </li>
    {/if}
  </ul>
{/snippet}

{#snippet editorSelect({ key, value, onpick, ariaLabel }: {
  key: string;
  value: string;
  onpick: (editorId: string) => void;
  ariaLabel: string;
})}
  {@const selectId = editorSelectId(key)}
  <span class="editor-sel">
    <button
      style:anchor-name="--{selectId}"
      class="editor-trigger"
      aria-label={ariaLabel}
      disabled={ides.length === 0}
      popovertarget={selectId}
      type="button"
    >
      <span>{editorLabel(value)}</span>
      <span class="caret" aria-hidden="true">▾</span>
    </button>
    <ul id={selectId} style:position-anchor="--{selectId}" class="menu editor-menu" popover>
      {#each ides as editor (editor.id)}
        {@const isPicked = editor.id === value}
        <li>
          <button
            class="editor-opt"
            class:picked={isPicked}
            aria-current={isPicked}
            onclick={() => onpick(editor.id)}
            popovertarget={selectId}
            popovertargetaction="hide"
            type="button"
          >
            <span>{editor.label}</span>
            {#if isPicked}
              <span class="tick" aria-hidden="true">✓</span>
            {/if}
          </button>
        </li>
      {:else}
        <li class="none editor-empty">No editors detected.</li>
      {/each}
    </ul>
  </span>
{/snippet}

<div class="picker">
  <div class="inner">
    <header>
      <span class="brand">◆ PADE</span>
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
          <button
            class="sm-btn"
            class:on={startMode === StartMode.enum.temp}
            onclick={() => setStartMode(StartMode.enum.temp)}
          >Temp workspace</button>
          <button
            class="sm-btn"
            class:on={startMode === StartMode.enum.picker}
            onclick={() => setStartMode(StartMode.enum.picker)}
          >This picker</button>
        </div>
      </div>
      <label class="autoname">
        <span class="ck">
          <input checked={autoName} onchange={e => setAutoName(e.currentTarget.checked)} type="checkbox" />
          <span class="box" aria-hidden="true">
            <svg fill="none" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7" /></svg>
          </span>
        </span>
        <span>Auto-name temp workspaces once the agent starts working</span>
      </label>
      {#if isWindows}
        <label class="autoname">
          <span class="ck">
            <input checked={ctxMenuOn} onchange={e => setCtxMenu(e.currentTarget.checked)} type="checkbox" />
            <span class="box" aria-hidden="true">
              <svg fill="none" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7" /></svg>
            </span>
          </span>
          <span>Add “Open in PADE” to the folder right-click menu</span>
        </label>
      {/if}
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
              {#if renaming === path}
                <form
                  class="rename" onsubmit={async e => {
                    e.preventDefault(); await commitRename(path);
                  }}>
                  <input aria-label="New name" bind:value={renameValue} />
                  <button type="submit">Save to root</button>
                  <button onclick={() => (renaming = null)} type="button">Cancel</button>
                </form>
              {:else}
                <button class="recent-item" onclick={() => onopen({ path })}>
                  {#if isTempPath(path)}
                    <span class="temp-tag">temp</span>
                  {/if}
                  <span class="rname">{displayName(path)}</span>
                  <span class="rpath">{path}</span>
                </button>
                {@render rowMenu(path)}
              {/if}
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

    <section class="editors">
      <div class="ed-head">
        <h2>Editors</h2>
        <p class="hint">
          PADE reads what’s in a folder and opens it in the editor you set for
          that kind of project. Rules win over order — no shuffling a priority
          list.
        </p>
      </div>
      <ul class="ed-rules">
        {#each EDITOR_KINDS as { kind, label } (kind)}
          {@const isThisProject = currentKind === kind}
          <li class="ed-rule" class:here={isThisProject}>
            <span class="ed-kind">
              <span class="ed-label">{label}</span>
              {#if isThisProject}
                <span class="here-tag">this project</span>
              {/if}
            </span>
            <span class="ed-spacer"></span>
            <span class="ed-arrow">detected → open in</span>
            {@render editorSelect({
              key: kind,
              value: ideRules[kind] ?? ideFallback,
              onpick: editorId => setEditorRule({
                kind,
                editorId
              }),
              ariaLabel: `Editor for ${label} projects`
            })}
          </li>
        {/each}
        <li class="ed-rule fallback">
          <span class="ed-label">Any other folder</span>
          <span class="ed-spacer"></span>
          <span class="ed-arrow">fall back to</span>
          {@render editorSelect({
            key: "fallback",
            value: ideFallback,
            onpick: setEditorFallback,
            ariaLabel: "Fallback editor"
          })}
        </li>
      </ul>
    </section>

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
                {@render rowMenu(p.path)}
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
    background: radial-gradient(120% 100% at 50% 0%, var(--surface-1), var(--surface));
  }

  .inner {
    display: flex;
    flex-direction: column;
    gap: 28px;
    inline-size: min(680px, 100%);
    margin-inline: auto;
    padding-block: 48px 80px;
    padding-inline: 24px;
    animation: rise 420ms var(--ease);
  }

  header {
    animation: rise 300ms var(--ease);

    .brand {
      color: var(--primary);
      font-weight: 700;
      font-size: 13px;
      letter-spacing: 0.02em;
    }

    h1 {
      margin-block: 10px 0;
      margin-inline: 0;
      font-weight: 800;
      font-size: clamp(24px, 4vw, 36px);
      letter-spacing: -0.02em;
      text-wrap: balance;
    }

    .lede {
      max-inline-size: 52ch;
      margin-block: 8px 0;
      margin-inline: 0;
      color: var(--on-surface-var);
    }
  }

  /* Big scratch-workspace card — filled primary-container with a hairline
     primary edge; brightens on hover. */
  .temp-start {
    display: flex;
    flex-direction: column;
    gap: 8px;
    justify-content: center;
    inline-size: 100%;
    padding: 20px 22px;
    border: 1px solid var(--primary);
    border-radius: var(--r-lg);
    background: var(--primary-container);
    color: var(--on-primary-container);
    text-align: start;
    cursor: pointer;
    transition: filter 150ms var(--ease);

    &:hover {
      filter: brightness(1.08);
    }

    .ico {
      font-size: 20px;
    }

    .txt {
      display: flex;
      flex-direction: column;
      gap: 2px;
    }

    strong {
      font-weight: 700;
      font-size: 16px;
    }

    small {
      color: var(--on-primary-container);
      font-size: 13px;
      opacity: 85%;
    }
  }

  .recent-head {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: center;
  }

  .clear {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    padding: 4px 8px;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--on-surface-var);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition:
      color 150ms var(--ease),
      background 150ms var(--ease);

    &:hover {
      background: var(--crit-wash);
      color: var(--crit);
    }
  }

  .browse {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    padding: 10px 16px;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-2);
    }
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
    position: relative;
    display: flex;
    gap: 4px;
    align-items: center;
  }

  .row > button:first-child {
    flex: 1;
    min-inline-size: 0;
  }

  /* Trailing kebab — a subtle pill circle that fills on hover. */
  .kebab {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 30px;
    inline-size: 30px;
    margin-inline-start: auto;
    padding: 0;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--on-surface-var);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-2);
      color: var(--on-surface);
    }
  }

  /* Native popover row menu — light-dismisses on outside click; the only place
     a floating shadow is warranted. */
  .menu {
    position: absolute;
    inset: auto;
    overflow-y: auto;
    max-block-size: min(70vh, 520px);
    min-inline-size: 250px;
    margin-block: 6px 0;
    margin-inline: 0;
    padding: 6px;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
    background: var(--surface-2);
    list-style: none;
    box-shadow: 0 16px 40px color-mix(in sRGB, var(--on-surface) 22%, transparent);
    position-area: bottom span-left;

    li button {
      display: flex;
      gap: 10px;
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

    .sep {
      margin-block-start: 6px;
      padding-block-start: 6px;
      border-block-start: 1px solid var(--outline);
    }

    .danger:hover {
      background: var(--crit-wash);
      color: var(--crit);
    }
  }

  .rename {
    display: flex;
    gap: 8px;
    align-items: center;
    inline-size: 100%;
    padding: 6px 8px;
    border: 1px solid var(--primary);
    border-radius: var(--r-md);
    background: var(--surface-2);

    input {
      flex: 1;
      min-inline-size: 0;
      padding: 0;
      border: none;
      background: transparent;
      color: var(--on-surface);
      font-family: var(--font-mono);
      font-weight: 600;
      font-size: 13px;
    }

    button {
      flex: none;
      padding: 6px 14px;
      border: none;
      border-radius: 999px;
      background: var(--primary);
      color: var(--on-primary);
      font: inherit;
      font-weight: 700;
      font-size: 12px;
      cursor: pointer;
      transition: filter 150ms var(--ease);

      &:hover {
        filter: brightness(1.06);
      }
    }

    button + button {
      background: transparent;
      color: var(--on-surface-var);
      font-weight: 600;

      &:hover {
        background: var(--surface-3);
        filter: none;
      }
    }
  }

  .startmode {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
  }

  .autoname {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 13px;
    cursor: pointer;
  }

  /* Animated custom checkbox — a rounded box that fills primary with a
     stroke-drawn check (check-pop) when toggled. Semantic input stays inside. */
  .ck {
    position: relative;
    display: inline-grid;
    flex: none;
    place-items: center;
    block-size: 20px;
    inline-size: 20px;

    input {
      position: absolute;
      inset: 0;
      block-size: 100%;
      inline-size: 100%;
      margin: 0;
      opacity: 0%;
      cursor: pointer;
    }

    .box {
      display: grid;
      place-items: center;
      block-size: 20px;
      inline-size: 20px;
      border: 2px solid var(--outline);
      border-radius: 7px;
      background: var(--surface-2);
      transition:
        background 250ms var(--ease),
        border-color 250ms var(--ease),
        scale 300ms var(--spring);

      svg {
        display: block;
        block-size: 13px;
        inline-size: 13px;
      }

      path {
        fill: none;
        stroke: var(--on-primary);
        stroke-dasharray: 22;
        stroke-dashoffset: 22;
        stroke-linecap: round;
        stroke-linejoin: round;
        stroke-width: 3;
        transition: stroke-dashoffset 240ms var(--ease) 60ms;
      }
    }

    input:checked + .box {
      border-color: var(--primary);
      background: var(--primary);
      scale: 1.06;
      animation: check-pop 360ms var(--spring);
    }

    input:checked + .box path {
      stroke-dashoffset: 0;
    }

    input:not(:checked):hover + .box {
      border-color: var(--primary);
    }
  }

  .sm-label {
    color: var(--on-surface-var);
    font-size: 13px;
  }

  /* Pill segmented toggle. */
  .sm-toggle {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
    background: var(--surface-2);

    .sm-btn {
      padding: 6px 14px;
      border: none;
      border-radius: 999px;
      background: transparent;
      color: var(--on-surface-var);
      font: inherit;
      font-weight: 600;
      font-size: 12px;
      cursor: pointer;
      transition:
        background 150ms var(--ease),
        color 150ms var(--ease);

      &.on {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }
    }
  }

  /* Recent row — pill button, mono name, truncating path; fills on hover. */
  .recent-item {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding: 10px 12px;
    border: none;
    border-radius: var(--r-md);
    background: transparent;
    color: var(--on-surface);
    text-align: start;
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-2);
    }

    .rname {
      flex: none;
      font-family: var(--font-mono);
      font-weight: 600;
      font-size: 13px;
    }

    .rpath {
      overflow: hidden;
      color: var(--on-surface-var);
      font-family: var(--font-mono);
      font-size: 11px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .temp-tag {
    flex: none;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-var);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  /* Section eyebrows — uppercase micro-labels. */
  h2 {
    margin: 0;
    color: var(--on-surface-var);
    font-weight: 700;
    font-size: 12px;
    letter-spacing: 0.07em;
    text-transform: uppercase;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .hint {
    margin: 0;
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

  /* Choice chips — pills; selected gets a primary edge over its container. */
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
    transition: border-color 150ms var(--ease);

    &:hover {
      border-color: var(--primary);
    }

    &.on {
      border-color: var(--primary);
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  input,
  select,
  textarea {
    padding: 10px 14px;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-size: 14px;
  }

  input {
    flex: 1;
    min-inline-size: 200px;
    font-family: var(--font-mono);
    font-size: 13px;
  }

  textarea {
    inline-size: 100%;
    line-height: 1.5;
    resize: vertical;
  }

  button {
    padding: 10px 18px;
    border: none;
    border-radius: var(--r-md);
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 700;
    cursor: pointer;
    transition: filter 150ms var(--ease);

    &:hover {
      filter: brightness(1.06);
    }

    &:disabled {
      opacity: 50%;
      filter: none;
      cursor: default;
    }
  }

  .roots .root {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .root-head {
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .rootpath {
    padding: 4px 10px;
    border-radius: var(--r-sm);
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .remove {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 24px;
    inline-size: 24px;
    margin-inline-start: auto;
    padding: 0;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-var);
    font-size: 16px;
    transition:
      background 150ms var(--ease),
      color 150ms var(--ease);

    &:hover {
      background: var(--crit-wash);
      color: var(--crit);
      filter: none;
    }
  }

  .projects {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* Detected project — tonal card; lifts into primary-container on hover. */
  .project {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding: 11px 13px;
    border-radius: var(--r-md);
    background: var(--surface-2);
    color: var(--on-surface);
    text-align: start;
    transition:
      background 150ms var(--ease),
      color 150ms var(--ease);

    &:hover {
      background: var(--primary-container);
      color: var(--on-primary-container);
      filter: none;
    }

    .pname {
      font-weight: 600;
      font-size: 14px;
    }

    .git {
      color: var(--tertiary);
      font-weight: 700;
      font-size: 9px;
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

  /* Editor-rules engine — one tonal row per project kind, plus a dashed
     fall-back row. Each row carries a native-popover editor select. */
  .ed-head {
    display: flex;
    flex-direction: column;
    gap: 6px;

    .hint {
      max-inline-size: 62ch;
      font-size: 12px;
      line-height: 1.5;
    }
  }

  .ed-rules {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .ed-rule {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
    padding-block: 10px;
    padding-inline: 14px 8px;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
    background: var(--surface-1);
    transition:
      border-color 150ms var(--ease),
      background 150ms var(--ease);

    &:hover {
      border-color: var(--primary-container);
      background: var(--surface-2);
    }

    &.fallback {
      border-style: dashed;
    }

    /* The current project's kind — a subtle tertiary edge. */
    &.here {
      border-color: var(--tertiary);
      background: var(--tertiary-wash);
    }
  }

  .ed-kind {
    display: flex;
    gap: 8px;
    align-items: center;
    min-inline-size: 150px;
  }

  .ed-label {
    font-weight: 600;
    font-size: 13px;
  }

  .here-tag {
    flex: none;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--tertiary);
    color: var(--on-primary);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .ed-spacer {
    flex: 1;
    min-inline-size: 8px;
  }

  .ed-arrow {
    flex: none;
    color: var(--on-surface-var);
    font-size: 12px;
  }

  .editor-sel {
    position: relative;
    flex: none;
  }

  /* Popover select trigger — pill that brightens its edge on hover. */
  .editor-trigger {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    padding: 8px 12px;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition:
      border-color 150ms var(--ease),
      background 150ms var(--ease);

    &:hover:not(:disabled) {
      border-color: var(--primary);
      background: var(--surface-3);
      filter: none;
    }

    .caret {
      font-size: 9px;
      opacity: 70%;
    }
  }

  /* Reuse the row-menu popover chrome; align + size for a select. */
  .editor-menu {
    min-inline-size: 180px;

    .editor-opt {
      justify-content: space-between;

      &.picked {
        color: var(--primary);
      }
    }

    .tick {
      color: var(--primary);
    }

    .editor-empty {
      padding: 8px 10px;
    }
  }
</style>
