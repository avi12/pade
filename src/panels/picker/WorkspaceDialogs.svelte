<script lang="ts">
  import RelabelDialog from "@/lib/dialogs/RelabelDialog.svelte";
  import RenameFolderDialog from "@/lib/dialogs/RenameFolderDialog.svelte";
  import WorkspaceDeleteDialog from "@/lib/dialogs/WorkspaceDeleteDialog.svelte";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";

  // The workspace lifecycle's prompts, raised from either list's row menu: the
  // delete confirmation, the folder rename and the PADE-only relabel. One of
  // each for the whole picker, rendered outside its scrolling page.
  const { lifecycle, labels }: {
    lifecycle: WorkspaceLifecycle;
    /** Friendly display labels per path - the one source every row reads. */
    labels: Record<string, string>;
  } = $props();

  const relabelTargetLabel = $derived(labels[lifecycle.relabelTarget ?? ""] ?? "");
  const isDeleteTargetOwned = $derived(
    lifecycle.deleteTarget !== null && lifecycle.isOwned(lifecycle.deleteTarget)
  );
  const deleteConfirmLabel = $derived.by(() => {
    if (isDeleteTargetOwned) {
      return "Delete workspace";
    }

    return "Delete directory";
  });
  const deleteTitle = $derived.by(() => {
    if (isDeleteTargetOwned) {
      return "Delete this workspace?";
    }

    return "Delete this project directory?";
  });
</script>

{#if lifecycle.deleteTarget}
  <WorkspaceDeleteDialog
    confirmLabel={deleteConfirmLabel}
    error={lifecycle.deleteError}
    isBusy={lifecycle.isDeleting}
    isSkipTipShown
    {labels}
    oncancel={() => lifecycle.cancelDelete()}
    onconfirm={async () => await lifecycle.confirmDelete()}
    path={lifecycle.deleteTarget}
    title={deleteTitle}
  />
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
