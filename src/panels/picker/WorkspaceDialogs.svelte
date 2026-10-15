<script lang="ts">
  import RelabelDialog from "@/lib/RelabelDialog.svelte";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
  import WorkspaceDeleteDialog from "@/panels/picker/WorkspaceDeleteDialog.svelte";

  // The owned-workspace lifecycle's prompts, raised from either list's row menu:
  // the delete confirmation and the PADE-only relabel. One of each for the whole
  // picker, rendered outside its scrolling page.
  const { lifecycle, labels }: {
    lifecycle: WorkspaceLifecycle;
    /** Friendly display labels per path - the one source every row reads. */
    labels: Record<string, string>;
  } = $props();
</script>

{#if lifecycle.deleteTarget}
  <WorkspaceDeleteDialog {labels} {lifecycle} path={lifecycle.deleteTarget} />
{/if}

{#if lifecycle.relabelTarget}
  <RelabelDialog
    currentLabel={labels[lifecycle.relabelTarget] ?? ""}
    onclose={() => lifecycle.finishRelabel()}
    path={lifecycle.relabelTarget}
  />
{/if}
