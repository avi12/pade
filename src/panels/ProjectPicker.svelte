<script lang="ts">
  import BrandMark from "@/lib/BrandMark.svelte";
  import { ide, os, workspace } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import { baseName, displayName, isTemporaryWorkspace } from "@/lib/paths";
  import { StartMode } from "@/lib/types";
  import type { Agent, Ide, ProjectEntry, Settings } from "@/lib/types";
  import { FolderPath, nameError, parseInput, ProjectName } from "@/lib/validate";
  import AgentsSection from "@/panels/picker/AgentsSection.svelte";
  import "@/panels/picker/chrome.css";
  import EditorsSection from "@/panels/picker/EditorsSection.svelte";
  import OnLaunchSection from "@/panels/picker/OnLaunchSection.svelte";
  import QuickStartSection from "@/panels/picker/QuickStartSection.svelte";
  import { ask, open as openDialog } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";

  // Shown when the app wasn't launched inside a project. Manage root folders,
  // browse the projects inside them, open one, or create a new one — then hand
  // the chosen project path (and optional first prompt) back to the app.
  const {
    agents,
    onopen,
    onmove,
    onrename
  }: {
    agents: Agent[];
    onopen: (target: {
      path: string;
      initialPrompt?: string;
    }) => void;
    onmove: (target: {
      from: string;
      destDir: string;
    }) => Promise<string>;
    onrename: (target: {
      from: string;
      newName: string;
    }) => Promise<string>;
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

  async function setEditorRule({ kind, editorId }: {
    kind: string;
    editorId: string;
  }) {
    settings = await workspace.setPrefs({
      ...settings.prefs,
      ideRules: {
        ...settings.prefs.ideRules,
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

  function scan(root: string): Promise<ProjectEntry[]> {
    return workspace.scan(root).catch((): ProjectEntry[] => []);
  }

  async function addRoot() {
    const path = parseInput({
      schema: FolderPath,
      raw: newRoot
    });
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

  function isOwned(path: string): boolean {
    // Temp dirs are ADE-created even if predating owned-workspace tracking.
    return settings.ownedWorkspaces.includes(path) || isTemporaryWorkspace(path);
  }
  // Stable, valid popover id/anchor per row. A path can appear in more than one
  // section (Recent AND under its root), so the same path would otherwise mint
  // duplicate ids/anchors and clicking one kebab would open the wrong menu.
  // The section scope disambiguates them.
  function menuId({ path, scope }: {
    path: string;
    scope: string;
  }): string {
    return `menu-${scope}-${path.replaceAll(/[^a-zA-Z0-9]/g, "-")}`;
  }

  // Owned-workspace lifecycle: delete, move (→ permanent, still deletable),
  // rename (→ promoted into the primary project root).
  let renaming = $state<string | null>(null);
  let renameValue = $state("");

  // Live rename validation — nameError (lib/validate) surfaces the schema's own
  // message; the same schema gates Save, so an invalid name can't reach a
  // rename() that would silently no-op.
  const renameError = $derived(nameError(renameValue));
  const renameValid = $derived(ProjectName.safeParse(renameValue).success);

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

    await onmove({
      from: path,
      destDir: dest
    });
    await refresh();
  }

  function startRename(path: string) {
    renaming = path;
    renameValue = baseName(path);
  }

  async function commitRename(path: string) {
    const newName = parseInput({
      schema: ProjectName,
      raw: renameValue
    });
    if (!newName) {
      return;
    }

    await onrename({
      from: path,
      newName
    });
    renaming = null;
    await refresh();
  }

  onMount(() => void refresh());
</script>

{#snippet rowMenu({ path, scope }: {
  path: string;
  scope: string;
})}
  {@const identifier = menuId({
    path,
    scope
  })}
  <button
    style:anchor-name="--{identifier}"
    class="kebab"
    aria-label="Project actions"
    popovertarget={identifier}
  ><Icon name="more" /></button>
  <ul id={identifier} style:position-anchor="--{identifier}" class="menu" popover>
    <li class="head">Reveal</li>
    <li>
      <button class="mi" onclick={() => void os.explorer(path)} popovertarget={identifier} popovertargetaction="hide">
        <Icon name="folder" /><span class="mi-txt">Open in Files</span>
      </button>
    </li>
    <li>
      <button class="mi" onclick={() => void os.terminal(path)} popovertarget={identifier} popovertargetaction="hide">
        <Icon name="terminal" /><span class="mi-txt">Open in Terminal</span>
      </button>
    </li>
    {#if ides.length > 0}
      <li>
        <button
          class="mi"
          onclick={() => void ide.open({
            command: ides[0].command,
            path
          })}
          popovertarget={identifier}
          popovertargetaction="hide"
        >
          <Icon name="code" /><span class="mi-txt">Open in {ides[0].label}</span>
        </button>
      </li>
    {/if}
    {#if isOwned(path)}
      <li class="head sep">Workspace</li>
      <li>
        <button class="mi" onclick={() => startRename(path)} popovertarget={identifier} popovertargetaction="hide">
          <Icon name="pencil" /><span class="mi-txt">Rename to a project</span>
        </button>
      </li>
      <li>
        <button
          class="mi"
          onclick={async () => await moveWorkspace(path)}
          popovertarget={identifier}
          popovertargetaction="hide"
        >
          <Icon name="swap" /><span class="mi-txt">Move…</span>
        </button>
      </li>
      <li>
        <button
          class="mi danger"
          onclick={async () => await deleteWorkspace(path)}
          popovertarget={identifier}
          popovertargetaction="hide"
        >
          <Icon name="trash" /><span class="mi-txt">Delete workspace</span>
        </button>
      </li>
    {/if}
  </ul>
{/snippet}

<div class="picker">
  <div class="inner">
    <header>
      <BrandMark />
      <h1>Open a project</h1>
      <p class="lede">
        Pick up where you left off, drop into a throwaway workspace, or point
        PADE at your code.
      </p>
    </header>

    <QuickStartSection {onopen} roots={settings.roots} />

    <OnLaunchSection onautoname={setAutoName} onstartmode={setStartMode} prefs={settings.prefs} />

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
                  class="rename" onsubmit={async event => {
                    event.preventDefault(); await commitRename(path);
                  }}>
                  <input
                    aria-describedby={renameError ? "rename-error" : undefined}
                    aria-invalid={renameError !== null}
                    aria-label="Project name"
                    bind:value={renameValue}
                  />
                  <button disabled={!renameValid} type="submit">Save</button>
                  <button onclick={() => (renaming = null)} type="button">Cancel</button>
                  {#if renameError}
                    <output id="rename-error" class="field-error rename-error">{renameError}</output>
                  {/if}
                </form>
              {:else}
                <button class="recent-item" onclick={() => onopen({ path })}>
                  {#if isTemporaryWorkspace(path)}
                    <span
                      class="temp-tag"
                      data-tooltip="Auto-named by the agent — the folder keeps its path"
                    >temp</span>
                  {:else if isOwned(path)}
                    <span class="project-tag">project</span>
                  {/if}
                  <span class="rname">{displayName(path, settings.labels)}</span>
                  <span class="rpath">{path}</span>
                </button>
                {@render rowMenu({
                  path,
                  scope: "recent"
                })}
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <AgentsSection {agents} defaultAgent={settings.defaultAgent} onpick={setMaster} />

    <EditorsSection
      {currentKind}
      {ides}
      onfallback={setEditorFallback}
      onrule={setEditorRule}
      prefs={settings.prefs}
    />

    <section class="roots">
      <h2>Root folders</h2>
      <form
        class="addrow" onsubmit={event => {
          event.preventDefault(); addRoot();
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
            ><Icon name="close" size={14} /></button>
          </div>
          <ul class="projects">
            {#each projectsByRoot[root] ?? [] as project (project.path)}
              <li class="row">
                <button class="project" onclick={() => onopen({ path: project.path })}>
                  <span class="pname">{project.name}</span>
                  {#if project.isGit}
                    <span class="git">git</span>
                  {/if}
                </button>
                {@render rowMenu({
                  path: project.path,
                  scope: "root"
                })}
              </li>
            {:else}
              <li class="none">No projects found in this folder.</li>
            {/each}
          </ul>
        </div>
      {/each}
    </section>
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
      color: var(--on-surface-variant);
    }
  }

  /* Shared page chrome (eyebrows, base fields/buttons, rows, kebab menus) lives
     in picker/chrome.css so the section components share one copy. */

  /* The "Start something new" cards live in picker/QuickStartSection.svelte. */

  /* In the rename row, the error sits on its own full-width line below the field
     + buttons (the row is flex; this breaks to a new line). */
  .rename-error {
    flex-basis: 100%;
    margin-block-start: 2px;
  }

  /* "On launch" toggles live in picker/OnLaunchSection.svelte. */

  /* ── Recent ── */
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
    color: var(--on-surface-variant);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition:
      color 150ms var(--ease),
      background 150ms var(--ease);

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
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

  /* Recent row — pill button, mono name, truncating path; fills on hover. */
  .recent-item {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding: 10px 12px;
    border: none;
    border-radius: var(--radius-medium);
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
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 13px;
    }

    .rpath {
      overflow: hidden;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
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
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .project-tag {
    flex: none;
    color: var(--tertiary);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  /* Inline rename — a bordered field with Save (primary) + Cancel. */
  .rename {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    inline-size: 100%;
    padding: 6px 8px;
    border: 1px solid var(--primary);
    border-radius: var(--radius-medium);
    background: var(--surface-2);

    input {
      flex: 1;
      min-inline-size: 0;
      padding: 0;
      border: none;
      background: transparent;
      color: var(--on-surface);
      font-family: var(--font-monospace);
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

      &:hover:not(:disabled) {
        filter: brightness(1.06);
      }

      &:disabled {
        opacity: 50%;
        filter: none;
        cursor: default;
      }
    }

    button + button {
      background: transparent;
      color: var(--on-surface-variant);
      font-weight: 600;

      &:hover {
        background: var(--surface-3);
        filter: none;
      }
    }
  }

  /* Default-agent chips live in picker/AgentsSection.svelte. */
  .addrow {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .addrow input {
    flex: 1;
    min-inline-size: 200px;
    font-family: var(--font-monospace);
    font-size: 13px;
  }

  .browse {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    padding: 10px 16px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-2);
      filter: none;
    }
  }

  /* ── Root folders ── */
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
    border-radius: var(--radius-small);
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: var(--font-monospace);
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
    color: var(--on-surface-variant);

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
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
    border-radius: var(--radius-medium);
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

  /* Editor-rules rows live in picker/EditorsSection.svelte. */
</style>
