<script lang="ts">
  // The go-ahead for a chat-storage limit that deletes chats right away: which
  // ones (each can be unticked to keep it for good), how many and how much,
  // that it can't be undone, and - when the protected chats alone are over the
  // total - why the total stays above it.
  import ConfirmDialog from "@/lib/dialogs/ConfirmDialog.svelte";
  import type { Agent, ChatStorage } from "@/lib/types";
  import { errorMessage } from "@/lib/util/errors/errors";
  import { cleanupSummary, keptAboveLimit, selectedCleanup } from "@/panels/config/chat-storage";
  import DeletedChatList from "@/panels/config/DeletedChatList.svelte";
  import { SvelteSet } from "svelte/reactivity";

  const { preview, agents, storageLimitMegabytes, onapply, oncancel }: {
    /** The chats measured against the proposed limits. */
    preview: ChatStorage;
    /** The installed agents, for the names the list shows. */
    agents: readonly Agent[];
    storageLimitMegabytes: number | null;
    /** Save the limits and delete all but `keep`; rejects when that fails. */
    onapply: (keep: string[]) => Promise<void>;
    oncancel: () => void;
  } = $props();

  let isApplying = $state(false);
  let failure = $state<string | null>(null);
  /** The listed chats unticked - every one starts ticked, to be deleted. */
  const kept = new SvelteSet<string>();

  const selected = $derived(
    selectedCleanup({
      cleanup: preview.cleanup,
      kept
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
      return "Keep them all";
    }

    return "Delete chats";
  });
  const title = $derived.by(() => {
    if (selected.chatCount === 0) {
      return "Keep every chat?";
    }

    return `Delete ${cleanupSummary(selected)}?`;
  });
</script>

<ConfirmDialog
  busy={isApplying}
  busyLabel="Deleting…"
  {confirmLabel}
  danger={selected.chatCount > 0}
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
      await onapply([...kept]);
    } catch (error) {
      failure = errorMessage({
        error,
        fallback: "Couldn't delete the chats"
      });
    } finally {
      isApplying = false;
    }
  }}
  {title}
>
  <DeletedChatList
    {agents}
    cleanup={preview.cleanup}
    isLocked={isApplying}
    {kept}
    ontoggle={({ ids, isKept }) => {
      for (const id of ids) {
        if (isKept) {
          kept.add(id);
        } else {
          kept.delete(id);
        }
      }
    }}
  />
  <p>
    This limit deletes the ticked chats now and keeps deleting as chats grow. An unticked chat is kept for
    good. A deleted chat can't be resumed, by PADE or by its agent
  </p>
  {#if keptNote}
    <p>{keptNote}</p>
  {/if}
</ConfirmDialog>
