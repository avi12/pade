<script lang="ts" module>
  /** The stop action's name - the row's stop button and this menu's item. */
  export const STOP_SESSION = "Stop session";
</script>

<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { SidebarChat } from "@/lib/session/chat-registry";
  import { rovingMenu } from "@/lib/ui/roving-menu";

  // A chat row's right-click menu - open it, start another in its project,
  // stop it while it runs, or delete it for good. It hangs at the pointer
  // that opened it, not at the row.
  const { chat, onopen, onnew, onstop, ondelete }: {
    chat: SidebarChat;
    /** Takes the click, so Ctrl/Shift on the item opens a new window too. */
    onopen: (e: MouseEvent) => void;
    onnew: () => void;
    onstop: () => void;
    /** Never offered for a recorded conversation, whose transcript is the
     *  agent's, not PADE's to delete. */
    ondelete: () => void;
  } = $props();

  const menuId = $derived(`project-chat-menu-${chat.id}`);
  // Another window's session is that window's to stop or delete, never this
  // rail's.
  const isElsewhere = $derived(chat.windowLabel !== undefined);
  const isDeletable = $derived(chat.resume === undefined && !isElsewhere);
  const isStoppable = $derived(chat.isLive && !isElsewhere);

  let elMenu = $state<HTMLElement | null>(null);
  let menuPosition = $state({
    x: 0,
    y: 0
  });

  /** Open the menu at the pointer of a right-click. */
  export function openAt(e: MouseEvent) {
    e.preventDefault();
    menuPosition = {
      x: e.clientX,
      y: e.clientY
    };
    elMenu?.showPopover();
  }
</script>

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
    onclick={onopen}
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
  {#if isStoppable}
    <button
      class="menu-item"
      onclick={onstop}
      popovertarget={menuId}
      popovertargetaction="hide"
      role="menuitem"
      type="button"
    >
      <span class="menu-icon"><Icon name="stop" size={15} /></span>
      <span>{STOP_SESSION}</span>
    </button>
  {/if}
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
  /* Fixed at the pointer, not anchored to the row. */
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
