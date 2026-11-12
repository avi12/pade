<script lang="ts">
  // The chats a chat-storage limit flags, old heavy ones first, each by its
  // agent's title with its agent, folder, size and last use - each with a tick
  // to delete it, none ticked to start, and one to tick or untick them all -
  // and how many more the list leaves out.
  import type { Agent } from "@/lib/types/agent";
  import type { ChatStorage } from "@/lib/types/chat-storage";
  import Checkbox from "@/lib/ui/controls/Checkbox.svelte";
  import { truncationTooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { deletedChatRow, unlistedChats } from "@/panels/config/chat-storage/chat-storage";

  const { cleanup, agents, ticked, isLocked, ontoggle }: {
    cleanup: ChatStorage["cleanup"];
    agents: readonly Agent[];
    /** The listed chats ticked to delete. */
    ticked: ReadonlySet<string>;
    /** While the deletion runs the ticks can't change. */
    isLocked: boolean;
    ontoggle: (change: {
      ids: readonly string[];
      isTicked: boolean;
    }) => void;
  } = $props();

  const now = Date.now();
  const unlisted = $derived(unlistedChats(cleanup));
  const isAllTicked = $derived(cleanup.chats.length > 0 && ticked.size === cleanup.chats.length);

  function titleTooltip(title: string) {
    return truncationTooltip({ tooltip: () => title });
  }
</script>

<label class="tick-all">
  <Checkbox
    checked={isAllTicked}
    disabled={isLocked}
    onchange={checked => ontoggle({
      ids: cleanup.chats.map(chat => chat.id),
      isTicked: checked
    })}
  />
  <span>Select all listed</span>
</label>
<ul class="deleted-chats">
  {#each cleanup.chats as chat (chat.id)}
    {@const row = deletedChatRow({
      chat,
      agents,
      now
    })}
    <li>
      <label class="chat">
        <Checkbox
          checked={ticked.has(chat.id)}
          disabled={isLocked}
          onchange={checked => ontoggle({
            ids: [chat.id],
            isTicked: checked
          })}
        />
        <span class="chat-text">
          <span class="chat-title" {@attach titleTooltip(row.title)}>{row.title}</span>
          <span class="chat-details">{row.details}</span>
        </span>
      </label>
    </li>
  {/each}
</ul>
{#if unlisted}
  <p class="unlisted">{unlisted}</p>
{/if}

<style>
  .tick-all {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--on-surface);
    font-weight: 600;
    font-size: 0.75rem;
    cursor: pointer;
  }

  .deleted-chats {
    display: grid;
    gap: 2px;
    overflow-y: auto;
    max-block-size: 240px;
    margin: 0;
    padding: 4px;
    border: 1px solid var(--outline-variant);
    border-radius: var(--radius-small);
    background: var(--surface-2);
    list-style: none;
  }

  .chat {
    display: flex;
    gap: 8px;
    align-items: center;
    min-inline-size: 0;
    padding-block: 4px;
    padding-inline: 6px;
    border-radius: var(--radius-small);
    cursor: pointer;

    &:hover {
      background: var(--surface-3);
    }
  }

  .chat-text {
    display: grid;
    gap: 1px;
    min-inline-size: 0;
  }

  .chat-title {
    overflow: hidden;
    color: var(--on-surface);
    font-weight: 600;
    font-size: 0.75rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chat-details {
    overflow: hidden;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 0.625rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .unlisted {
    margin: 0;
    color: var(--on-surface-variant);
    font-size: 0.75rem;
  }
</style>
