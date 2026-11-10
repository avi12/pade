<script lang="ts">
  // How much disk the agents' recorded chats take, and the two limits that
  // keep it in check: one chat's size and all chats together. A limit that
  // would delete chats right away asks first, saying how many and how much.
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { updatePrefs } from "@/lib/settings/prefs.svelte";
  import type { Agent, ChatStorage, Prefs } from "@/lib/types";
  import { settledValue } from "@/lib/util/async/settled";
  import {
    agentShares,
    CHAT_SIZE_CHOICES,
    CHAT_STORAGE_CHOICES,
    keptAboveLimit,
    storageSummary
  } from "@/panels/picker/chat-storage";
  import type { ChatStorageLimits } from "@/panels/picker/chat-storage";
  import ChatStorageConfirm from "@/panels/picker/ChatStorageConfirm.svelte";
  import ChoiceRow from "@/panels/picker/ChoiceRow.svelte";
  import { onMount } from "svelte";

  const { agents, prefs }: {
    agents: Agent[];
    prefs: Prefs;
  } = $props();

  const saved = $derived<ChatStorageLimits>({
    sizeLimitMegabytes: prefs.chatSizeLimitMegabytes ?? null,
    storageLimitMegabytes: prefs.chatStorageLimitMegabytes ?? null
  });

  let storage = $state<ChatStorage | null>(null);
  /** A limit waiting on the user's go-ahead, measured against it. */
  let pending = $state<{
    limits: ChatStorageLimits;
    preview: ChatStorage;
  } | null>(null);
  const summary = $derived.by(() => {
    if (!storage) {
      return "Measuring…";
    }

    return storageSummary(storage);
  });

  const shares = $derived.by(() => {
    if (!storage) {
      return "";
    }

    return agentShares({
      storage,
      agents
    });
  });

  const keptNote = $derived.by(() => {
    if (!storage) {
      return null;
    }

    return keptAboveLimit({
      storage,
      storageLimitMegabytes: saved.storageLimitMegabytes
    });
  });

  async function measure(limits: ChatStorageLimits) {
    return await settledValue({
      promise: sendMessage(Command.chatStorage, limits),
      fallback: null
    });
  }

  async function apply(limits: ChatStorageLimits) {
    await updatePrefs({
      chatSizeLimitMegabytes: limits.sizeLimitMegabytes,
      chatStorageLimitMegabytes: limits.storageLimitMegabytes
    });
    storage = await sendMessage(Command.chatStorageClean);
  }

  async function propose(limits: ChatStorageLimits) {
    const preview = await measure(limits);
    const deletesNothing = preview === null || preview.cleanup.chatCount === 0;
    if (deletesNothing) {
      await apply(limits);
      return;
    }

    pending = {
      limits,
      preview
    };
  }

  async function applyPending() {
    if (!pending) {
      return;
    }

    await apply(pending.limits);
    pending = null;
  }

  onMount(async () => {
    storage = await measure(saved);
  });
</script>

<section class="chat-storage">
  <div class="storage-head">
    <h2>Chat storage</h2>
    <output class="storage-total">{summary}</output>
  </div>
  {#if shares}
    <p class="storage-shares">{shares}</p>
  {/if}
  <ChoiceRow
    caption="Delete a chat bigger than"
    choices={CHAT_SIZE_CHOICES}
    onpick={async size => await propose({
      ...saved,
      sizeLimitMegabytes: size
    })}
    selected={saved.sizeLimitMegabytes}
  />
  <ChoiceRow
    caption="Keep all chats under"
    choices={CHAT_STORAGE_CHOICES}
    onpick={async total => await propose({
      ...saved,
      storageLimitMegabytes: total
    })}
    selected={saved.storageLimitMegabytes}
  />
  <p class="storage-hint">
    Deletes the oldest chat of each project in turn. A project's newest chat and anything used in the last
    week are always kept
  </p>
  {#if keptNote}
    <p class="storage-hint">{keptNote}</p>
  {/if}
</section>

{#if pending}
  <ChatStorageConfirm
    onapply={applyPending}
    oncancel={() => (pending = null)}
    preview={pending.preview}
    storageLimitMegabytes={pending.limits.storageLimitMegabytes}
  />
{/if}

<style>
  .chat-storage {
    gap: 10px;
  }

  .storage-head {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: baseline;
  }

  .storage-total {
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
  }

  .storage-shares,
  .storage-hint {
    margin: 0;
    color: var(--on-surface-variant);
    font-size: 0.75rem;
  }

  .storage-shares {
    font-family: var(--font-monospace);
  }
</style>
