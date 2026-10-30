<script lang="ts">
  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import { runningModelLook } from "@/lib/model/running-model";
  import RunningModelName from "@/lib/model/RunningModelName.svelte";
  import AgentStatusGlyph from "@/lib/session/AgentStatusGlyph.svelte";
  import type { SidebarChat } from "@/lib/session/chats/chat-registry";
  import ProjectChatMenu, { STOP_SESSION } from "@/lib/shell/sidebar/ProjectChatMenu.svelte";
  import { sessionModel } from "@/lib/stores/agent/context.svelte";
  import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
  import { pacingStatus } from "@/lib/stores/usage/usagePacing.svelte";
  import type { RecordedConversation } from "@/lib/types";
  import { truncationTooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { spokenTime } from "@/lib/usage/pacing/pacing-policy";
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
  const PAUSED_NOTE = "Paused by usage pacing";
  const STEP_FINISHING_NOTE = "finishing its step first";
  const isElsewhere = $derived(chat.windowLabel !== undefined);
  const pacing = $derived(pacingStatus(chat.id));
  // The model and effort in words - the mark under the label shows only the
  // model, tinted by its effort.
  const modelNote = $derived.by(() => {
    const running = sessionModel(chat.id);
    if (running === null) {
      return undefined;
    }

    return runningModelLook({
      running
    }).label;
  });
  const isPaused = $derived(chat.isLive && !isElsewhere && pacing?.isStopped === true);
  // When it continues on its own - the time the pause strip on its pane names.
  const pausedUntil = $derived.by(() => {
    const continuesAt = pacing?.continuesAt ?? null;
    if (continuesAt === null) {
      return "";
    }

    return spokenTime({
      time: continuesAt,
      now: Date.now()
    });
  });
  const pausedNote = $derived.by(() => {
    if (pausedUntil === "") {
      return `${PAUSED_NOTE} - ${STEP_FINISHING_NOTE}`;
    }

    return `${PAUSED_NOTE} - continues around ${pausedUntil}`;
  });
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
    {#if chat.isLive}
      <!-- Measured here only when this window runs it; another window's chat
           still breathes with its reported status. -->
      <AgentStatusGlyph agentId={chat.agentId} sessionId={isElsewhere ? null : chat.id} {status} />
    {:else}
      <span class="agent-icon"><Icon name={agentIconName(chat.agentId)} size={14} /></span>
    {/if}
    <span class="chat-text">
      <span class="chat-label" dir="auto">{chat.label}</span>
      <span class="model" data-tooltip={modelNote}><RunningModelName sessionId={chat.id} /></span>
    </span>
    <span class="paused" data-tooltip={pausedNote} hidden={!isPaused}>
      <Icon name="pause" size={12} />
      <span class="paused-until">{pausedUntil}</span>
      <span class="spoken">{pausedNote}</span>
    </span>
    <span class="elsewhere" hidden={!isElsewhere}>
      <Icon name="external" size={12} />
      <span class="spoken">{ELSEWHERE_NOTE}</span>
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

    /* The label over what the session runs, so the label keeps the row's
       whole width. */
    .chat-text {
      display: flex;
      flex: 1;
      flex-direction: column;
      min-inline-size: 0;
    }

    /* Empty for a chat this window isn't running. */
    .model {
      display: inline-flex;
      align-self: start;
      font-family: var(--font-monospace);
      font-size: 0.6875rem;

      &:empty {
        display: none;
      }
    }

    .paused {
      display: inline-flex;
      flex: none;
      gap: 3px;
      align-items: center;
      color: var(--warning);
      font-family: var(--font-monospace);
      font-size: 0.6875rem;

      &[hidden] {
        display: none;
      }
    }

    /* Spoken, not shown - the glyph beside it says it to the eye. */
    .spoken {
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
</style>
