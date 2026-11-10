<script lang="ts">
  // Which chats a chat-storage limit deletes, largest first, each by its
  // agent's title with its agent, folder, size and last use - and how many
  // smaller ones the list leaves out.
  import type { Agent, ChatStorage } from "@/lib/types";
  import { truncationTooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { deletedChatRow, unlistedChats } from "@/panels/config/chat-storage";

  const { cleanup, agents }: {
    cleanup: ChatStorage["cleanup"];
    agents: readonly Agent[];
  } = $props();

  const now = Date.now();
  const unlisted = $derived(unlistedChats(cleanup));

  function titleTooltip(title: string) {
    return truncationTooltip({ tooltip: () => title });
  }
</script>

<ul class="deleted-chats">
  {#each cleanup.chats as chat, iChat (iChat)}
    {@const row = deletedChatRow({
      chat,
      agents,
      now
    })}
    <li>
      <span class="chat-title" {@attach titleTooltip(row.title)}>{row.title}</span>
      <span class="chat-details">{row.details}</span>
    </li>
  {/each}
</ul>
{#if unlisted}
  <p class="unlisted">{unlisted}</p>
{/if}

<style>
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

  li {
    display: grid;
    gap: 1px;
    min-inline-size: 0;
    padding-block: 4px;
    padding-inline: 6px;
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
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 0.625rem;
  }

  .unlisted {
    margin: 0;
    color: var(--on-surface-variant);
    font-size: 0.75rem;
  }
</style>
