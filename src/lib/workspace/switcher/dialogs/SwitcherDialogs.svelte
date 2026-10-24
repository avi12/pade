<script lang="ts">
  import RelabelDialog from "@/lib/dialogs/RelabelDialog.svelte";
  import RenameFolderDialog from "@/lib/dialogs/RenameFolderDialog.svelte";
  import type { Relocator } from "@/lib/workspace/relocate/workspace-relocate";
  import DeleteDirectoryPrompt from "@/lib/workspace/switcher/dialogs/DeleteDirectoryPrompt.svelte";

  // The prompts a switcher row opens - delete a project's directory, relabel a
  // temp workspace, rename a folder on disk. Each is nested inside the switcher
  // popover so it stays visible (dimmed) behind the prompt; setting a target
  // opens its dialog and closing clears it.
  let {
    deleteTarget = $bindable(),
    relabelTarget = $bindable(),
    renameFolderTarget = $bindable(),
    labels,
    ondelete,
    onrenamefolder
  }: {
    /** The project whose directory is up for deletion. */
    deleteTarget: string | null;
    /** The temp workspace whose PADE-only label is being edited. */
    relabelTarget: string | null;
    /** The project whose folder is being renamed on disk. */
    renameFolderTarget: string | null;
    labels: Record<string, string>;
    /** Delete the directory - resolves once the row has animated out (or
     *  throws its message). */
    ondelete: (path: string) => Promise<void>;
    onrenamefolder: Relocator["renameFolder"];
  } = $props();

  // The relabel prompt starts from the workspace's current PADE-only label.
  const relabelCurrentLabel = $derived.by(() => {
    if (relabelTarget === null) {
      return "";
    }

    return labels[relabelTarget] ?? "";
  });
</script>

{#if deleteTarget}
  <DeleteDirectoryPrompt
    {labels}
    onclose={() => {
      deleteTarget = null;
    }}
    {ondelete}
    project={deleteTarget}
  />
{/if}

{#if relabelTarget}
  <RelabelDialog
    currentLabel={relabelCurrentLabel}
    nested
    onclose={() => {
      relabelTarget = null;
    }}
    path={relabelTarget}
  />
{/if}

{#if renameFolderTarget}
  <RenameFolderDialog
    nested
    onclose={() => {
      renameFolderTarget = null;
    }}
    onrename={onrenamefolder}
    path={renameFolderTarget}
  />
{/if}
