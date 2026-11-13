<script lang="ts">
  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import { runningModelLook } from "@/lib/model/readout/running-model";
  import AgentStatusGlyph from "@/lib/session/AgentStatusGlyph.svelte";
  import type { SidebarChat } from "@/lib/session/chats/chat-registry";
  import { ChatActivity, chatActivity, chatStatus, NEEDS_YOU } from "@/lib/shell/sidebar/chats/chat-activity";
  import ProjectChatMenu, { STOP_SESSION } from "@/lib/shell/sidebar/chats/ProjectChatMenu.svelte";
  import { sessionModel } from "@/lib/stores/agent/context.svelte";
  import { pacingStatus } from "@/lib/stores/usage/usagePacing.svelte";
  import type { RecordedConversation } from "@/lib/types/chat-storage";
  import { truncationTooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { spokenTime } from "@/lib/usage/pacing/engine/pacing-policy";
  import { formatAge } from "@/lib/util/format";
  import { isNewWindowClick } from "@/lib/workspace/open-windows";

  // One chat row in the left rail, on one line: a status dot (waiting on you,
  // working or idle), the title, what it is doing in words (or how long ago an
  // idle one last moved), then the agent's mark. The model it runs is in the
  // row's tooltip. A live chat carries a stop button beside the row's own
  // button, never inside it, so stopping never also opens the chat. A row
  // another PADE window runs wears that window's mark, and only that window
  // can stop it. Ctrl, Cmd or Shift asks for the chat in a new window, as on
  // a project row.
  const { chat, isActive, now, onselect, onresume, ondelete, onnew, onstop }: {
    chat: SidebarChat;
    isActive: boolean;
    /** The instant an idle chat's age is measured against. */
    now: number;
    /** Bring up a PADE chat - here, or in a new window. */
    onselect: (shouldOpenNewWindow: boolean) => void;
    /** Reopen a recorded conversation on its own transcript. */
    onresume: (target: {
      conversation: RecordedConversation;
      shouldOpenNewWindow: boolean;
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
  // The model and effort in words, for the tooltip.
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
  const status = $derived(chatStatus(chat));
  const activity = $derived(
    chatActivity({
      isLive: chat.isLive,
      status
    })
  );
  // What the row says it is doing: a chat waiting on you says so, an idle one
  // how long ago it last moved (when that is known), a working one nothing -
  // its dot says it.
  const idleAge = $derived.by(() => {
    const updatedAt = chat.resume?.updatedAt;
    if (updatedAt === undefined) {
      return "";
    }

    return formatAge({
      stamp: updatedAt,
      now
    });
  });
  const statusText = $derived.by(() => {
    if (activity === ChatActivity.NEEDS_YOU) {
      return NEEDS_YOU;
    }

    if (activity === ChatActivity.IDLE) {
      return idleAge;
    }

    return "";
  });

  // Shared by the row's click and its menu's "Open chat".
  function open(event: MouseEvent) {
    const shouldOpenNewWindow = isNewWindowClick(event);
    if (chat.resume !== undefined) {
      onresume({
        conversation: chat.resume,
        shouldOpenNewWindow
      });
      return;
    }

    onselect(shouldOpenNewWindow);
  }
  let menu = $state<ProjectChatMenu | null>(null);

  // The model it runs at rest; the full title leads while the row ellipsizes it.
  function labelTruncationTooltip(element: HTMLElement) {
    return truncationTooltip({
      tooltip: () => [chat.label, modelNote].filter(line => line !== undefined).join("\n"),
      restingTooltip: () => modelNote ?? "",
      measureSelector: ".chat-label"
    })(element);
  }

  // Shared by the row's button and its stop button, so a right-click anywhere
  // on the row opens the same menu.
  function openMenu(event: MouseEvent) {
    menu?.openAt(event);
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
    <span class="status-dot {activity}" aria-hidden="true"></span>
    <span class="chat-label" dir="auto">{chat.label}</span>
    <span class="paused" data-tooltip={pausedNote} hidden={!isPaused}>
      <Icon name="pause" size={12} />
      <span class="paused-until">{pausedUntil}</span>
      <span class="spoken">{pausedNote}</span>
    </span>
    <span class="elsewhere" hidden={!isElsewhere}>
      <Icon name="external" size={12} />
      <span class="spoken">{ELSEWHERE_NOTE}</span>
    </span>
    <span class="status-text {activity}" hidden={statusText === ""}>{statusText}</span>
    {#if chat.isLive}
      <!-- Measured here only when this window runs it; another window's chat
           still breathes with its reported status. -->
      <AgentStatusGlyph agentId={chat.agentId} sessionId={isElsewhere ? null : chat.id} {status} />
    {:else}
      <span class="agent-icon"><Icon name={agentIconName(chat.agentId)} size={12} /></span>
    {/if}
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

    /* Calm until wanted: the stop button opens out on the row's hover or
       focus, and always on the active row - the chat most likely to be
       stopped. It keeps no width at rest, so the agent mark ends the row like
       the design, yet the arrows still reach it. */
    &:hover,
    &:focus-within,
    &.active {
      --stop-inline-size: 1.375rem;
    }

    /* Cyberpunk: a flat line - pointing at it raises the fill a step, and the
       chat in front wears only a subtle fill, its ink unchanged. */
    :global(:root[data-palette="cyberpunk"]) & {
      color: var(--on-surface);

      &:hover:not(.active) {
        background: var(--surface-3);
      }

      &.active {
        background: var(--surface-2);
        color: var(--on-surface);
        font-weight: inherit;
      }
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

    /* The tooltip puts the title and the model on lines of their own. */
    &::after {
      white-space: pre-line;
    }

    /* Cyberpunk: a session name is mono writing, set a size under its
       project's. */
    :global(:root[data-palette="cyberpunk"]) & {
      gap: 0.4375rem;
      padding-block: 0.3125rem;
      padding-inline: 0.4375rem;
      font-family: var(--font-monospace);
      font-weight: 500;
      font-size: 0.7188rem;
      letter-spacing: 0.02em;
    }

    .agent-icon {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
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

  /* What the chat is doing, in words - a chat waiting on you in the warning
     ink, an idle one's age quiet. */
  .status-text {
    flex: none;
    color: var(--on-surface-variant);
    font-weight: 600;
    font-size: 0.625rem;
    white-space: nowrap;

    &.needs-you {
      color: var(--warning);
    }

    &[hidden] {
      display: none;
    }

    :global(:root[data-palette="cyberpunk"]) & {
      font-family: var(--font-ui);
      font-size: 0.5938rem;
      letter-spacing: normal;
    }
  }

  /* The status dot: waiting on you in the warning ink with a soft halo,
     working in the primary, idle a quiet outline-coloured mark. */
  .status-dot {
    flex: none;
    block-size: 0.4375rem;
    inline-size: 0.4375rem;
    border-radius: 50%;
    background: var(--outline);

    &.working {
      background: var(--primary);
    }

    &.needs-you {
      background: var(--warning);
      box-shadow: 0 0 0 0.125rem var(--warning-wash);
    }

    /* Cyberpunk: a square chip, like the design. */
    :global(:root[data-palette="cyberpunk"]) & {
      border-radius: 0;
    }
  }

  .stop {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    overflow: hidden;
    block-size: 1.375rem;
    inline-size: var(--stop-inline-size, 0);
    margin-inline-end: 3px;
    padding: 0;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: inherit;
    cursor: pointer;
    transition: inline-size 120ms var(--ease), background 120ms var(--ease), color 120ms var(--ease);

    @media (prefers-reduced-motion: reduce) {
      transition: none;
    }

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
    }
  }
</style>
