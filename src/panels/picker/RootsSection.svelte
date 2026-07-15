<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { collapseRow } from "@/lib/motion";
  import type { Ide, ProjectEntry } from "@/lib/types";
  import { FolderPath, parseInput } from "@/lib/validate";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
  import RowMenu from "@/panels/picker/RowMenu.svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";

  // Root folders: add one (typed path or native folder picker), remove one,
  // and browse the projects detected inside each. Root persistence and the
  // per-root scan stay with the parent (single settings owner) via onadd /
  // onremove; the typed path is validated here, at the point of entry.
  const {
    roots,
    projectsByRoot,
    ides,
    lifecycle,
    onopen,
    onadd,
    onremove
  }: {
    roots: string[];
    projectsByRoot: Record<string, ProjectEntry[]>;
    ides: Ide[];
    lifecycle: WorkspaceLifecycle;
    onopen: (target: { path: string }) => void;
    onadd: (path: string) => Promise<void>;
    onremove: (path: string) => Promise<void>;
  } = $props();

  let newRoot = $state("");

  async function addRoot() {
    const path = parseInput({
      schema: FolderPath,
      raw: newRoot
    });
    if (!path) {
      return;
    }

    await onadd(path);
    newRoot = "";
  }
</script>

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
    <!-- Native folder picker (Tauri dialog) — nicer than pasting a path. -->
    <button
      class="browse"
      onclick={async () => {
        const picked = await openDialog({
          directory: true,
          multiple: false
        });
        if (typeof picked === "string") {
          newRoot = picked;
          await addRoot();
        }
      }}
      type="button"
    ><Icon name="folder" /> Browse…</button>
    <button disabled={!newRoot.trim()} type="submit">Add root</button>
  </form>

  {#each roots as root (root)}
    <div class="root">
      <div class="root-head">
        <code class="rootpath">{root}</code>
        <button
          class="remove"
          aria-label="Remove root"
          data-tooltip="Remove root"
          onclick={async () => await onremove(root)}
        ><Icon name="close" size={14} /></button>
      </div>
      <ul class="projects">
        {#each projectsByRoot[root] ?? [] as project (project.path)}
          <li class="row" out:collapseRow>
            <button class="project" onclick={() => onopen({ path: project.path })}>
              <span class="pname">{project.name}</span>
              {#if project.isGit}
                <span class="git">git</span>
              {/if}
            </button>
            <RowMenu {ides} {lifecycle} path={project.path} scope="root" />
          </li>
        {:else}
          <li class="none">No projects found in this folder.</li>
        {/each}
      </ul>
    </div>
  {/each}
</section>

<style>
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
</style>
