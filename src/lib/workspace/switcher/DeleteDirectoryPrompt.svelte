<script lang="ts">
  import WorkspaceDeleteDialog from "@/lib/dialogs/WorkspaceDeleteDialog.svelte";

  // The destructive "Delete directory" confirmation for one switcher row. It
  // stays open (busy) while the removal runs and shows its failure in place; on
  // success the refreshed list animates the row out and the prompt closes.
  const { project, labels, ondelete, onclose }: {
    project: string;
    labels: Record<string, string>;
    /** Delete the directory - resolves once the row has animated out (or
     *  throws its message). */
    ondelete: (path: string) => Promise<void>;
    onclose: () => void;
  } = $props();

  const DELETE_FAILED = "Couldn’t delete that directory";

  let isDeleting = $state(false);
  let deleteError = $state("");
</script>

<WorkspaceDeleteDialog
  confirmLabel="Delete directory"
  error={deleteError}
  isBusy={isDeleting}
  isNested
  {labels}
  oncancel={() => {
    if (!isDeleting) {
      onclose();
    }
  }}
  onconfirm={async () => {
    isDeleting = true;
    deleteError = "";
    try {
      await ondelete(project);
      onclose();
    } catch (error) {
      deleteError = typeof error === "string" ? error : DELETE_FAILED;
    } finally {
      isDeleting = false;
    }
  }}
  path={project}
  title="Delete this project directory?"
/>
