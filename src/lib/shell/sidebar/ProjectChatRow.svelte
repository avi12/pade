<script lang="ts">
  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import type { SidebarChat } from "@/lib/session/chat-registry";
  import ProjectChatMenu from "@/lib/shell/sidebar/ProjectChatMenu.svelte";
  import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
  import type { RecordedConversation } from "@/lib/types";

  // One chat row in the left rail, with its right-click menu.
  const { chat, isActive, onselect, onresume, ondelete, onnew }: {
    chat: SidebarChat;
    isActive: boolean;
    /** Bring up a PADE chat. */
    onselect: () => void;
    /** Reopen a recorded conversation on its own transcript. */
    onresume: (conversation: RecordedConversation) => void;
    /** Delete a PADE chat - never offered for a recorded conversation, whose
     *  transcript is the agent's, not PADE's to delete. */
    ondelete: () => void;
    onnew: () => void;
  } = $props();

  // Shared by the row's click and its menu's "Open chat".
  function open() {
    if (chat.resume !== undefined) {
      onresume(chat.resume);
      return;
    }

    onselect();
  }
  let menu = $state<ProjectChatMenu | null>(null);
</script>

<button
  class="chat"
  class:active={isActive}
  aria-current={isActive}
  data-tooltip={chat.label}
  onclick={open}
  oncontextmenu={e => menu?.openAt(e)}
  type="button"
>
  <span class="agent-icon">
    <Icon name={agentIconName(chat.agentId)} size={14} />
    {#if chat.isLive}
      <span class="status-dot {sessionStatus(chat.id)}" aria-hidden="true"></span>
    {/if}
  </span>
  <span class="chat-label">{chat.label}</span>
</button>

<ProjectChatMenu bind:this={menu} {chat} {ondelete} {onnew} onopen={open} />

<style>
  .chat {
    display: flex;
    gap: 7px;
    align-items: center;
    inline-size: 100%;
    padding-block: 6px;
    padding-inline: 8px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    font: inherit;
    font-size: 12.5px;
    text-align: start;
    cursor: pointer;
    transition: background 120ms var(--ease), color 120ms var(--ease);

    &:hover {
      background: var(--surface-2);
      color: var(--on-surface);
    }

    &.active {
      background: var(--primary-container);
      color: var(--on-primary-container);
      font-weight: 600;
    }

    .agent-icon {
      position: relative;
      display: inline-flex;
      flex: none;
    }

    .chat-label {
      flex: 1;
      overflow: hidden;
      min-inline-size: 0;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  /* Live status, mirroring the tab strip's dot colours - a badge on the agent
     icon's corner, so a live chat's label lines up with every other row's. */
  .status-dot {
    position: absolute;
    inset-block-end: -2px;
    inset-inline-end: -3px;
    z-index: 1;
    block-size: 7px;
    inline-size: 7px;

    /* Half its own size, not the pill token: the cyberpunk palette's
       `--radius-full` is a 7px chamfer, which bevels a 7px dot down to a
       sliver. 50% is a circle elsewhere and a diamond under the bevel. */
    border-radius: 50%;
    background: var(--on-surface-variant);

    @media (prefers-reduced-motion: reduce) {
      &.working {
        animation: none;
      }
    }

    &.working {
      background: var(--primary);
      animation: pulse 1100ms var(--ease) infinite;
    }

    &.ready {
      background: var(--tertiary);
    }

    &.exited {
      background: var(--critical);
    }
  }
</style>
