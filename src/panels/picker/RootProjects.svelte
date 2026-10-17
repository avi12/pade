<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { Ide, ProjectEntry } from "@/lib/types";
  import { collapseRow } from "@/lib/ui/motion";
  import { formatQuantity } from "@/lib/util/format";
  import LanguageBar from "@/lib/workspace/LanguageBar.svelte";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
  import OpenInEditorButton from "@/panels/picker/OpenInEditorButton.svelte";
  import RowMenu from "@/panels/picker/RowMenu.svelte";

  // One root folder: its path, how many projects it holds and a remove button,
  // over the list of projects detected inside it - each opens in a click.
  const { root, projects, ides, lifecycle, onopen, onremove }: {
    root: string;
    projects: ProjectEntry[];
    ides: Ide[];
    lifecycle: WorkspaceLifecycle;
    onopen: (target: { path: string }) => void;
    onremove: (path: string) => Promise<void>;
  } = $props();

  const projectCount = $derived.by(() => {
    if (projects.length === 0) {
      return "";
    }

    return formatQuantity({
      count: projects.length,
      noun: "project"
    });
  });
</script>

<div class="root" out:collapseRow>
  <div class="root-head">
    <span class="root-icon" aria-hidden="true"><Icon name="folder" size={15} /></span>
    <code class="rootpath">{root}</code>
    {#if projectCount}
      <span class="root-count">{projectCount}</span>
    {/if}
    <button
      class="remove"
      aria-label="Remove root"
      data-tooltip="Remove root"
      onclick={async () => await onremove(root)}
    ><Icon name="close" size={14} /></button>
  </div>
  <ul class="projects">
    {#each projects as project (project.path)}
      <li class="row" out:collapseRow>
        <button class="project" onclick={() => onopen({ path: project.path })}>
          <span class="project-name">{project.name}</span>
          {#if project.isGit}
            <span class="git">git</span>
          {/if}
          <LanguageBar path={project.path} />
        </button>
        <OpenInEditorButton name={project.name} {ides} path={project.path} />
        <RowMenu {lifecycle} path={project.path} scope="root" />
      </li>
    {:else}
      <li class="none">No projects found in this folder</li>
    {/each}
  </ul>
</div>

<style>
  .root {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .root-head {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .root-icon {
    display: inline-flex;
    flex: none;
    color: var(--on-surface-variant);
  }

  .rootpath {
    padding: 4px 10px;
    border-radius: var(--radius-small);
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-size: 12px;
  }

  /* How many projects the root holds - frames the list below as "what's inside". */
  .root-count {
    color: var(--on-surface-variant);
    font-weight: 600;
    font-size: 11px;
    white-space: nowrap;
  }

  .remove {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 24px;
    inline-size: 24px;

    /* Pushed to the row's end, past the path + count. */
    margin-inline-start: auto;
    padding: 0;
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--on-surface-variant);

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
      filter: none;
    }
  }

  /* The per-row "open in editor" button lives in picker/OpenInEditorButton.svelte
     (shared with the Recent rows). */

  .projects {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* Detected project - tonal card; lifts into primary-container on hover. */
  .project {
    position: relative;
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

    .project-name {
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
