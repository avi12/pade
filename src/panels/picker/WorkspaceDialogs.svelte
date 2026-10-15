<script lang="ts">
  import RelabelDialog from "@/lib/RelabelDialog.svelte";
  import RenameFolderDialog from "@/lib/RenameFolderDialog.svelte";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
  import WorkspaceDeleteDialog from "@/panels/picker/WorkspaceDeleteDialog.svelte";

  // The workspace lifecycle's prompts, raised from either list's row menu: the
  // delete confirmation, the folder rename and the PADE-only relabel. One of
  // each for the whole picker, rendered outside its scrolling page.
  const { lifecycle, labels }: {
    lifecycle: WorkspaceLifecycle;
    /** Friendly display labels per path - the one source every row reads. */
    labels: Record<string, string>;
  } = $props();

  const relabelTargetLabel = $derived(labels[lifecycle.relabelTarget ?? ""] ?? "");
</script>

{#if lifecycle.deleteTarget}
  <WorkspaceDeleteDialog {labels} {lifecycle} path={lifecycle.deleteTarget} />
{/if}

{#if lifecycle.renameFolderTarget}
  <RenameFolderDialog
    onclose={() => lifecycle.finishRenameFolder()}
    onrename={target => lifecycle.renameFolder(target)}
    path={lifecycle.renameFolderTarget}
  />
{/if}

{#if lifecycle.relabelTarget}
  <RelabelDialog
    currentLabel={relabelTargetLabel}
    onclose={() => lifecycle.finishRelabel()}
    path={lifecycle.relabelTarget}
  />
{/if}
