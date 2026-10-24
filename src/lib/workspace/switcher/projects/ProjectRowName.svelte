<script lang="ts">
  import { isTemporaryWorkspace, projectRootName } from "@/lib/util/paths";
  import RootedName from "@/lib/workspace/display/RootedName.svelte";
  import TempBadge from "@/lib/workspace/display/TempBadge.svelte";

  // A project row's name line: its root and name ("avi/pade", the top bar's
  // label), up to two lines, then the TEMP badge for a throwaway workspace.
  const { project, name }: {
    project: string;
    name: string;
  } = $props();
</script>

<span class="project-row-name-line">
  <span class="project-row-name"><RootedName {name} root={projectRootName(project)} /></span>
  {#if isTemporaryWorkspace(project)}
    <TempBadge />
  {/if}
</span>

<style>
  /* A long name (a temp workspace's sentence-long label) wraps onto a second
     line before it clips, so the rail's narrow width still shows most of it;
     the tooltip has the rest. */
  .project-row-name-line {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    min-inline-size: 0;
  }

  .project-row-name {
    overflow: hidden;
    max-block-size: 2lh;
    min-inline-size: 0;
    font-family: var(--font-monospace);
    font-weight: 600;
    font-size: 0.8125rem;
    overflow-wrap: anywhere;
    line-clamp: 2;
  }
</style>
