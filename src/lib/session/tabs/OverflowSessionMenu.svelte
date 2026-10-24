<script lang="ts">
  // The strip's "+N" overflow: a trigger counting the sessions that fit neither
  // as pills nor as dots, and a popover listing them to select or close. Absent
  // while every session fits.

  import Icon from "@/lib/Icon.svelte";
  import SessionDot from "@/lib/session/tabs/SessionDot.svelte";
  import { sessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
  import type { AgentSession } from "@/lib/types";
  import { formatCount } from "@/lib/util/format";

  const { sessions, activeId, isActive, isAwaitingChoice, onselect, onclose }: {
    /** The sessions collapsed into the "+N". */
    sessions: AgentSession[];
    activeId: string | null;
    /** The active session sits in a collapsed tier (a dot or the "+N"). */
    isActive: boolean;
    isAwaitingChoice: (id: string) => boolean;
    onselect: (id: string) => void;
    onclose: (session: AgentSession) => void;
  } = $props();

  const OVERFLOW_MENU_ID = "overflow-session-menu";
  const OVERFLOW_MENU_ANCHOR = "--overflow-session-anchor";

  // A pending prompt hidden in the overflow still surfaces on the trigger.
  const hasAwaitingChoice = $derived(sessions.some(session => isAwaitingChoice(session.id)));
</script>

{#if sessions.length > 0}
  <span class="more-wrap menu-host">
    <button
      style:anchor-name={OVERFLOW_MENU_ANCHOR}
      class="more-button menu-trigger"
      class:active={isActive}
      class:awaiting-choice={hasAwaitingChoice}
      aria-label="Show remaining sessions"
      popovertarget={OVERFLOW_MENU_ID}
    >+{formatCount(sessions.length)}</button>
    <ul
      id={OVERFLOW_MENU_ID}
      style:position-anchor={OVERFLOW_MENU_ANCHOR}
      class="menu more-menu popover-menu"
      popover
    >
      {#each sessions as session (session.id)}
        <li class="more-item" class:active={session.id === activeId}>
          <button
            class="menu-item more-pick"
            onclick={() => onselect(session.id)}
            popovertarget={OVERFLOW_MENU_ID}
            popovertargetaction="hide"
          >
            <SessionDot isAwaitingChoice={isAwaitingChoice(session.id)} sessionId={session.id} />
            <span class="more-label">{sessionLabel(session.id) ?? session.agent.label}</span>
          </button>
          <button
            class="menu-item more-close-button"
            aria-label="Close session"
            data-tooltip="Close session"
            onclick={() => onclose(session)}
          ><Icon name="close" size={13} /></button>
        </li>
      {/each}
    </ul>
  </span>
{/if}

<style>
  /* Under `.tabs` so these rules keep the specificity they had in the strip. */
  :global(.tabs) {
    .more-wrap {
      flex: none;
    }

    /* The "+N" overflow trigger. */
    .more-button {
      display: inline-flex;
      flex: none;
      align-items: center;
      block-size: 22px;
      padding-inline: 9px;
      border: none;
      border-radius: var(--radius-full);
      background: var(--surface-2);
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 0.6875rem;
      cursor: pointer;
      transition: color 150ms var(--ease), background 150ms var(--ease);

      &.active {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }

      /* A choice pending on a session hidden in the overflow flashes the +N
         (choice-flash lives in SessionTabs). */
      &.awaiting-choice {
        color: var(--critical);
        animation: choice-flash 1100ms var(--ease) infinite;

        @media (prefers-reduced-motion: reduce) {
          box-shadow: 0 0 0 2px var(--critical-wash);
          animation: none;
        }
      }

      &:hover {
        background: var(--surface-3);
      }
    }
  }

  /* Overflow-session popover: a compact two-column grid of the collapsed tabs.
     Shell and rows come from the shared .popover-menu and .menu-item. */
  .more-menu {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 4px;
    overflow-y: auto;
    max-block-size: min(60vh, 420px);
    inline-size: min(360px, 80vw);
    min-inline-size: 0;
    padding: 8px;
    position-area: bottom span-right;

    .more-item {
      display: flex;
      align-items: center;
      border-radius: var(--radius-small);

      /* The whole row washes neutral surface-3 on hover (canon); the inner pick
         button carries no fill of its own - so it cancels the shared .menu-item
         primary hover. */
      &:not(.active):hover {
        background: var(--surface-3);
      }

      &.active {
        background: var(--primary-container);
      }
    }

    .more-pick {
      display: flex;
      flex: 1;
      gap: 8px;
      align-items: center;
      inline-size: auto;
      min-inline-size: 0;
      font-family: var(--font-monospace);
      font-size: 0.75rem;

      &:hover {
        background: transparent;
        color: inherit;
      }
    }

    .more-item.active .more-pick {
      color: var(--on-primary-container);
    }

    .more-label {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .more-close-button {
      display: inline-flex;
      flex: none;
      justify-content: center;
      align-items: center;
      block-size: 26px;
      inline-size: 26px;
      padding: 0;
      color: var(--on-surface-variant);
      font-size: 0.9375rem;

      &:hover {
        background: var(--critical-wash);
        color: var(--critical);
      }
    }
  }
</style>
