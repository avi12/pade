<script lang="ts">
  // The review of the chats a storage limit flags: which ones, none ticked to
  // start, and only the ticked ones deleted - with how many and how much, that
  // it can't be undone, and, when the protected chats alone are over the
  // total, why the total stays above it.
  import ConfirmDialog from "@/lib/dialogs/ConfirmDialog.svelte";
  import type { Agent, ChatStorage } from "@/lib/types";
  import { errorMessage } from "@/lib/util/errors/errors";
  import { cleanupSummary, keptAboveLimit, tickedCleanup } from "@/panels/config/chat-storage";
  import DeletedChatList from "@/panels/config/DeletedChatList.svelte";
  import { SvelteSet } from "svelte/reactivity";

  const { preview, agents, storageLimitMegabytes, ondelete, oncancel }: {
    /** The chats measured against the saved limits. */
    preview: ChatStorage;
    /** The installed agents, for the names the list shows. */
    agents: readonly Agent[];
    storageLimitMegabytes: number | null;
    /** Delete the ticked chats; rejects when that fails. */
    ondelete: (ids: string[]) => Promise<void>;
    oncancel: () => void;
  } = $props();

  let isDeleting = $state(false);
  let failure = $state<string | null>(null);
  /** The chats ticked to delete - none until the user ticks them. */
  const ticked = new SvelteSet<string>();

  const selected = $derived(
    tickedCleanup({
      cleanup: preview.cleanup,
      ticked
    })
  );
  const keptNote = $derived(
    keptAboveLimit({
      storage: preview,
      storageLimitMegabytes
    })
  );
  const confirmLabel = $derived.by(() => {
    if (selected.chatCount === 0) {
      return "Tick chats to delete";
    }

    return `Delete ${cleanupSummary(selected)}`;
  });
</script>

<ConfirmDialog
  busy={isDeleting}
  busyLabel="Deleting…"
  {confirmLabel}
  danger
  error={failure}
  icon="trash"
  oncancel={() => {
    if (!isDeleting) {
      oncancel();
    }
  }}
  onconfirm={async () => {
    if (selected.chatCount === 0) {
      return;
    }

    isDeleting = true;
    failure = null;
    try {
      await ondelete([...ticked]);
    } catch (error) {
      failure = errorMessage({
        error,
        fallback: "Couldn't delete the chats"
      });
    } finally {
      isDeleting = false;
    }
  }}
  title="{cleanupSummary(preview.cleanup)} over the limit"
>
  <DeletedChatList
    {agents}
    cleanup={preview.cleanup}
    isLocked={isDeleting}
    ontoggle={({ ids, isTicked }) => {
      for (const id of ids) {
        if (isTicked) {
          ticked.add(id);
        } else {
          ticked.delete(id);
        }
      }
    }}
    {ticked}
  />
  <p>
    Only the chats you tick are deleted. A deleted chat can't be resumed, by PADE or by its agent
  </p>
  {#if keptNote}
    <p>{keptNote}</p>
  {/if}
</ConfirmDialog>
