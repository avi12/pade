<script lang="ts">
  import { isTemporaryWorkspace, projectRootPrefix } from "@/lib/util/paths";
  import RootedName from "@/lib/workspace/display/RootedName.svelte";
  import TempBadge from "@/lib/workspace/display/TempBadge.svelte";

  // A project row's name line: its root and name ("avi\pade", the top bar's
  // label), on one line, then the TEMP badge for a throwaway workspace.
  const { project, name }: {
    project: string;
    name: string;
  } = $props();
</script>

<span class="project-row-name-line">
  <span class="project-row-name"><RootedName {name} prefix={projectRootPrefix(project)} /></span>
  {#if isTemporaryWorkspace(project)}
    <TempBadge />
  {/if}
</span>

<style>
  /* A long name (a temp workspace's sentence-long label) ends in an ellipsis,
     so every row keeps one line; the tooltip has the rest. */
  .project-row-name-line {
    display: flex;
    gap: 6px;
    align-items: center;
    min-inline-size: 0;
  }

  .project-row-name {
    overflow: hidden;
    min-inline-size: 0;
    font-family: var(--font-monospace);
    font-weight: 600;
    font-size: 0.8125rem;
    text-overflow: ellipsis;
    white-space: nowrap;

    :global(:root[data-palette="cyberpunk"]) & {
      font-weight: 700;
      font-size: 0.7813rem;
      letter-spacing: 0.02em;
    }
  }
</style>
