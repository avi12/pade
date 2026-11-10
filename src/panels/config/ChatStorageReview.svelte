<script lang="ts">
  // How many chats the saved limits flag, with the button that opens their
  // review - shown only while there are some.
  import type { Agent, ChatStorage } from "@/lib/types";
  import { formatQuantity } from "@/lib/util/format";
  import ChatStorageConfirm from "@/panels/config/ChatStorageConfirm.svelte";

  let { storage, agents, storageLimitMegabytes, ondelete, isOpen = $bindable() }: {
    /** The chats measured against the saved limits. */
    storage: ChatStorage;
    agents: readonly Agent[];
    storageLimitMegabytes: number | null;
    /** Delete the ticked chats. */
    ondelete: (ids: string[]) => Promise<void>;
    /** Whether the review is open; picking a limit that flags chats opens it. */
    isOpen: boolean;
  } = $props();

  const flagged = $derived(storage.cleanup.chatCount);
  const flaggedSummary = $derived(
    `${formatQuantity({
      count: flagged,
      noun: "chat"
    })} over the limit`
  );
</script>

{#if flagged > 0}
  <div class="field-row">
    <span class="field-label">{flaggedSummary}</span>
    <button class="field-action" onclick={() => (isOpen = true)} type="button">Review</button>
  </div>
{/if}

{#if isOpen}
  <ChatStorageConfirm
    {agents}
    oncancel={() => (isOpen = false)}
    ondelete={async ids => {
      await ondelete(ids);
      isOpen = false;
    }}
    preview={storage}
    {storageLimitMegabytes}
  />
{/if}
