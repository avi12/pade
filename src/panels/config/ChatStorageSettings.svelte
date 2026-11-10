<script lang="ts">
  // How much disk the agents' recorded chats take, and the two limits that
  // keep it in check: one chat's size and all chats together. A limit that
  // would delete chats right away asks first, saying how many and how much.
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { updatePrefs } from "@/lib/settings/prefs.svelte";
  import { prefs } from "@/lib/settings/settings.svelte";
  import type { Agent, ChatStorage } from "@/lib/types";
  import Segmented from "@/lib/ui/controls/Segmented.svelte";
  import { settledValue } from "@/lib/util/async/settled";
  import {
    agentShares,
    CHAT_SIZE_CHOICES,
    CHAT_STORAGE_CHOICES,
    keptAboveLimit,
    storageSummary
  } from "@/panels/config/chat-storage";
  import type { ChatStorageLimits } from "@/panels/config/chat-storage";
  import ChatStorageConfirm from "@/panels/config/ChatStorageConfirm.svelte";
  import { onMount } from "svelte";

  const { agents }: {
    /** The installed agents, for the names each share is listed under. */
    agents: Agent[];
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

<div class="field">
  <span class="field-label">{summary}</span>
  {#if shares}
    <span class="field-hint shares">{shares}</span>
  {/if}
</div>

<div class="limit-row">
  <span class="field-text">
    <span class="field-label">Largest chat</span>
    <span class="field-hint">A bigger chat is deleted</span>
  </span>
  <div class="limit-choice">
    <Segmented
      chosen={saved.sizeLimitMegabytes}
      label="Largest chat"
      onchoose={async size => await propose({
        ...saved,
        sizeLimitMegabytes: size
      })}
      options={CHAT_SIZE_CHOICES}
    />
  </div>
</div>

<div class="limit-row">
  <span class="field-text">
    <span class="field-label">All chats</span>
    <span class="field-hint">Over this, each project's oldest chat goes first</span>
  </span>
  <div class="limit-choice">
    <Segmented
      chosen={saved.storageLimitMegabytes}
      label="All chats"
      onchoose={async total => await propose({
        ...saved,
        storageLimitMegabytes: total
      })}
      options={CHAT_STORAGE_CHOICES}
    />
  </div>
</div>

<p class="field-hint">
  A project's newest chat and anything used in the last week are always kept. Applies to every project
</p>
{#if keptNote}
  <p class="field-hint">{keptNote}</p>
{/if}

{#if pending}
  <ChatStorageConfirm
    onapply={applyPending}
    oncancel={() => (pending = null)}
    preview={pending.preview}
    storageLimitMegabytes={pending.limits.storageLimitMegabytes}
  />
{/if}

<style>
  p {
    margin: 0;
  }

  .shares {
    font-family: var(--font-monospace);
  }

  .limit-row {
    /* Label beside the options while the panel is wide; stacked once the drag
       handle takes that room away. Asked of the card, never the viewport. */
    @container (inline-size < 420px) {
      flex-direction: column;
      align-items: stretch;
    }

    display: flex;
    gap: 12px;
    justify-content: space-between;
    align-items: center;
  }

  .limit-choice {
    flex: none;
    min-inline-size: 230px;
  }
</style>
