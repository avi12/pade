<script lang="ts">
  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import type { SidebarChat } from "@/lib/session/chats/chat-registry";
  import ProjectChatMenu, { STOP_SESSION } from "@/lib/shell/sidebar/ProjectChatMenu.svelte";
  import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
  import type { RecordedConversation } from "@/lib/types";
  import { truncationTooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { isNewWindowClick } from "@/lib/workspace/open-windows";

  // One chat row in the left rail, with its right-click menu. A live chat
  // carries a stop button beside the row's own button, never inside it, so
  // stopping never also opens the chat. A row another PADE window runs wears
  // that window's mark, and only that window can stop it. Ctrl, Cmd or Shift
  // asks for the chat in a new window, as on a project row.
  const { chat, isActive, onselect, onresume, ondelete, onnew, onstop }: {
    chat: SidebarChat;
    isActive: boolean;
    /** Bring up a PADE chat - here, or in a new window. */
    onselect: (opensNewWindow: boolean) => void;
    /** Reopen a recorded conversation on its own transcript. */
    onresume: (target: {
      conversation: RecordedConversation;
      opensNewWindow: boolean;
    }) => void;
    /** Delete a PADE chat - never offered for a recorded conversation, whose
     *  transcript is the agent's, not PADE's to delete. */
    ondelete: () => void;
    onnew: () => void;
    /** End a live chat's agent session the way closing its tab does - it
     *  stays in the rail as a conversation to resume. */
    onstop: () => void;
  } = $props();

  const ELSEWHERE_NOTE = "Running in another window";
  const isElsewhere = $derived(chat.windowLabel !== undefined);
  const isStoppable = $derived(chat.isLive && !isElsewhere);
  const status = $derived(chat.status ?? sessionStatus(chat.id));

  // Shared by the row's click and its menu's "Open chat".
  function open(e: MouseEvent) {
    const opensNewWindow = isNewWindowClick(e);
    if (chat.resume !== undefined) {
      onresume({
        conversation: chat.resume,
        opensNewWindow
      });
      return;
    }

    onselect(opensNewWindow);
  }
  let menu = $state<ProjectChatMenu | null>(null);

  // The full title, only while the row ellipsizes it.
  function labelTruncationTooltip(element: HTMLElement) {
    return truncationTooltip({
      tooltip: () => chat.label,
      measureSelector: ".chat-label"
    })(element);
  }

  // Shared by the row's button and its stop button, so a right-click anywhere
  // on the row opens the same menu.
  function openMenu(e: MouseEvent) {
    menu?.openAt(e);
  }
</script>

<!-- One keyboard line, out of the tab order: the arrows reach it from its
     project's header (sidebar-keyboard). -->
<div class="row" class:active={isActive} data-sidebar-line>
  <button
    class="chat"
    {@attach labelTruncationTooltip}
    aria-current={isActive}
    data-sidebar-item
    onclick={open}
    oncontextmenu={openMenu}
    tabindex="-1"
    type="button"
  >
    <span class="agent-icon">
      <Icon name={agentIconName(chat.agentId)} size={14} />
      {#if chat.isLive}
        <span class="status-dot {status}" aria-hidden="true"></span>
      {/if}
    </span>
    <span class="chat-label">{chat.label}</span>
    <span class="elsewhere" hidden={!isElsewhere}>
      <Icon name="external" size={12} />
      <span class="elsewhere-note">{ELSEWHERE_NOTE}</span>
    </span>
  </button>
  {#if isStoppable}
    <button
      class="stop"
      aria-label="{STOP_SESSION}: {chat.label}"
      data-sidebar-item
      data-tooltip={STOP_SESSION}
      onclick={onstop}
      oncontextmenu={openMenu}
      tabindex="-1"
      type="button"
    ><Icon name="stop" size={12} /></button>
  {/if}
</div>

<ProjectChatMenu bind:this={menu} {chat} {ondelete} {onnew} onopen={open} {onstop} />

<style>
  /* The row carries the hover and active fill, so the chat button and its
     stop button read as one row. */
  .row {
    display: flex;
    align-items: center;
    min-inline-size: 0;
    border-radius: var(--radius-small);
    color: var(--on-surface-variant);
    transition: background 120ms var(--ease), color 120ms var(--ease);

    @media (prefers-reduced-motion: reduce) {
      transition: none;
    }

    &:hover {
      background: var(--surface-2);
      color: var(--on-surface);
    }

    &.active {
      background: var(--primary-container);
      color: var(--on-primary-container);
      font-weight: 600;
    }

    /* Calm until wanted: the stop button shows on the row's hover or focus,
       and always on the active row - the chat most likely to be stopped. It
       only fades, so Tab still reaches it. */
    &:hover,
    &:focus-within,
    &.active {
      --stop-opacity: 100%;
    }
  }

  .chat {
    display: flex;
    flex: 1;
    gap: 7px;
    align-items: center;
    min-inline-size: 0;
    padding-block: 6px;
    padding-inline: 8px;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 0.7813rem;
    text-align: start;
    cursor: pointer;

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

    .elsewhere {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);

      &[hidden] {
        display: none;
      }
    }

    /* Spoken, not shown - the window glyph says it to the eye. */
    .elsewhere-note {
      position: absolute;
      overflow: hidden;
      block-size: 1px;
      inline-size: 1px;
      clip-path: inset(50%);
      white-space: nowrap;
    }
  }

  .stop {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 22px;
    inline-size: 22px;
    margin-inline-end: 3px;
    padding: 0;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: inherit;
    opacity: var(--stop-opacity, 0%);
    cursor: pointer;
    transition: background 120ms var(--ease), color 120ms var(--ease), opacity 120ms var(--ease);

    @media (prefers-reduced-motion: reduce) {
      transition: none;
    }

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
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
