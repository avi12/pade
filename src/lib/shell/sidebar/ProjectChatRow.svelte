<script lang="ts">
  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import type { SidebarChat } from "@/lib/session/chat-registry";
  import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
  import type { RecordedConversation } from "@/lib/types";
  import { rovingMenu } from "@/lib/ui/roving-menu";

  // One chat row in the left rail, with the right-click menu that belongs to
  // it - open it, start another in its project, or delete it for good. The
  // menu hangs at the pointer that opened it, not at the row.
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

  const menuId = $derived(`project-chat-menu-${chat.id}`);
  const isDeletable = $derived(chat.resume === undefined);

  // Shared by the row's click and its menu's "Open chat".
  function open() {
    if (chat.resume !== undefined) {
      onresume(chat.resume);
      return;
    }

    onselect();
  }
  let elMenu = $state<HTMLElement | null>(null);
  let menuPosition = $state({
    x: 0,
    y: 0
  });
</script>

<button
  class="chat"
  class:active={isActive}
  aria-current={isActive}
  data-tooltip={chat.label}
  onclick={open}
  oncontextmenu={e => {
    e.preventDefault();
    menuPosition = {
      x: e.clientX,
      y: e.clientY
    };
    elMenu?.showPopover();
  }}
  type="button"
>
  <span class="agent-icon"><Icon name={agentIconName(chat.agentId)} size={14} /></span>
  {#if chat.isLive}
    <span class="status-dot {sessionStatus(chat.id)}" aria-hidden="true"></span>
  {/if}
  <span class="chat-label">{chat.label}</span>
</button>

<div
  bind:this={elMenu}
  id={menuId}
  style:left="{menuPosition.x}px"
  style:top="{menuPosition.y}px"
  class="context-menu popover-menu"
  {@attach rovingMenu}
  popover
  role="menu"
>
  <button
    class="menu-item"
    onclick={open}
    popovertarget={menuId}
    popovertargetaction="hide"
    role="menuitem"
    type="button"
  >
    <span class="menu-icon"><Icon name="terminal" size={15} /></span>
    <span>Open chat</span>
  </button>
  <button
    class="menu-item"
    onclick={onnew}
    popovertarget={menuId}
    popovertargetaction="hide"
    role="menuitem"
    type="button"
  >
    <span class="menu-icon"><Icon name="plus" size={15} /></span>
    <span>New chat in this project</span>
  </button>
  {#if isDeletable}
    <div class="menu-divider"></div>
    <button
      class="menu-item critical"
      onclick={ondelete}
      popovertarget={menuId}
      popovertargetaction="hide"
      role="menuitem"
      type="button"
    >
      <span class="menu-icon"><Icon name="trash" size={15} /></span>
      <span>Delete chat</span>
    </button>
  {/if}
</div>

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

  /* Live status, mirroring the tab strip's dot colours. */
  .status-dot {
    flex: none;
    block-size: 7px;
    inline-size: 7px;
    border-radius: var(--radius-full);
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

  /* The right-click menu - fixed at the pointer, not anchored to the row. */
  .context-menu {
    position: fixed;
    min-inline-size: 210px;
    margin: 0;

    .menu-icon {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
    }

    .menu-item.critical {
      color: var(--critical);

      .menu-icon {
        color: var(--critical);
      }

      &:hover {
        background: var(--critical-wash);
        color: var(--critical);
      }
    }
  }
</style>
