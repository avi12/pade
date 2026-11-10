<script lang="ts">
  // How much disk the agents' recorded chats take, and the two limits that
  // keep it in check: one chat's size and all chats together. A limit never
  // deletes by itself: it flags the chats over it, and the review lets the
  // user tick which go - none are ticked until they do.
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { updatePrefs } from "@/lib/settings/prefs.svelte";
  import { prefs } from "@/lib/settings/settings.svelte";
  import type { Agent, ChatStorage } from "@/lib/types";
  import { settledValue } from "@/lib/util/async/settled";
  import {
    agentShares,
    CHAT_SIZE_CHOICES,
    CHAT_STORAGE_CHOICES,
    keptAboveLimit,
    storageSummary
  } from "@/panels/config/chat-storage";
  import type { ChatStorageLimits } from "@/panels/config/chat-storage";
  import ChatLimitRow from "@/panels/config/ChatLimitRow.svelte";
  import ChatStorageReview from "@/panels/config/ChatStorageReview.svelte";
  import { onMount } from "svelte";

  const { agents }: {
    /** The installed agents, for the names each share is listed under. */
    agents: Agent[];
  } = $props();

  const saved = $derived<ChatStorageLimits>({
    sizeLimitMegabytes: prefs.chatSizeLimitMegabytes ?? null,
    storageLimitMegabytes: prefs.chatStorageLimitMegabytes ?? null
  });

  /** The chats measured against the saved limits. */
  let storage = $state<ChatStorage | null>(null);
  let isReviewing = $state(false);

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

  /** Save a limit - it deletes nothing by itself - and, when it flags chats,
   *  open the review so the user can tick which go. */
  async function chooseLimits(limits: ChatStorageLimits) {
    await updatePrefs({
      chatSizeLimitMegabytes: limits.sizeLimitMegabytes,
      chatStorageLimitMegabytes: limits.storageLimitMegabytes
    });
    storage = await measure(limits);
    isReviewing = (storage?.cleanup.chatCount ?? 0) > 0;
  }

  async function deleteTicked(ids: string[]) {
    storage = await sendMessage(Command.chatStorageDelete, { ids });
  }

  onMount(async () => {
    storage = await measure(saved);
  });
</script>

<div class="field">
  <span class="field-label">{summary}</span>
  {#if shares}
    <span class="field-hint shares">{shares}</span>
  {/if}
</div>

<ChatLimitRow
  chosen={saved.sizeLimitMegabytes}
  hint="A bigger chat is flagged"
  label="Largest chat"
  onchoose={async size => await chooseLimits({
    ...saved,
    sizeLimitMegabytes: size
  })}
  options={CHAT_SIZE_CHOICES}
/>

<ChatLimitRow
  chosen={saved.storageLimitMegabytes}
  hint="Over this, each project's oldest chats are flagged first"
  label="All chats"
  onchoose={async total => await chooseLimits({
    ...saved,
    storageLimitMegabytes: total
  })}
  options={CHAT_STORAGE_CHOICES}
/>

{#if storage}
  <ChatStorageReview
    {agents}
    ondelete={deleteTicked}
    {storage}
    storageLimitMegabytes={saved.storageLimitMegabytes}
    bind:isOpen={isReviewing}
  />
{/if}

<p class="field-hint">
  Nothing is deleted until you tick it. A project's newest chat and anything used in the last week are
  never flagged. Applies to every project
</p>
{#if keptNote}
  <p class="field-hint">{keptNote}</p>
{/if}

<style>
  p {
    margin: 0;
  }

  .shares {
    font-family: var(--font-monospace);
  }
</style>
