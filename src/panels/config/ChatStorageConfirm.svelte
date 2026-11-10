<script lang="ts">
  // The go-ahead for a chat-storage limit that deletes chats right away: how
  // many and how much, which ones, that it can't be undone, and - when the protected chats
  // alone are over the total - why the total stays above it.
  import ConfirmDialog from "@/lib/dialogs/ConfirmDialog.svelte";
  import type { Agent, ChatStorage } from "@/lib/types";
  import { errorMessage } from "@/lib/util/errors/errors";
  import { cleanupSummary, keptAboveLimit } from "@/panels/config/chat-storage";
  import DeletedChatList from "@/panels/config/DeletedChatList.svelte";

  const { preview, agents, storageLimitMegabytes, onapply, oncancel }: {
    /** The chats measured against the proposed limits. */
    preview: ChatStorage;
    /** The installed agents, for the names the list shows. */
    agents: readonly Agent[];
    storageLimitMegabytes: number | null;
    /** Save the limits and delete; rejects when that fails. */
    onapply: () => Promise<void>;
    oncancel: () => void;
  } = $props();

  let isApplying = $state(false);
  let failure = $state<string | null>(null);

  const keptNote = $derived(
    keptAboveLimit({
      storage: preview,
      storageLimitMegabytes
    })
  );
</script>

<ConfirmDialog
  busy={isApplying}
  busyLabel="Deleting…"
  confirmLabel="Delete chats"
  danger
  error={failure}
  icon="trash"
  oncancel={() => {
    if (!isApplying) {
      oncancel();
    }
  }}
  onconfirm={async () => {
    isApplying = true;
    failure = null;
    try {
      await onapply();
    } catch (error) {
      failure = errorMessage({
        error,
        fallback: "Couldn't delete the chats"
      });
    } finally {
      isApplying = false;
    }
  }}
  title="Delete {cleanupSummary(preview.cleanup)}?"
>
  <DeletedChatList {agents} cleanup={preview.cleanup} />
  <p>
    This limit deletes these chats now and keeps deleting as chats grow. A deleted chat can't be resumed, by
    PADE or by its agent
  </p>
  {#if keptNote}
    <p>{keptNote}</p>
  {/if}
</ConfirmDialog>
