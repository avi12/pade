<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { NEEDS_YOU } from "@/lib/shell/sidebar/chat-activity";
  import { WindowMode } from "@/lib/types";
  import { rovingMenu } from "@/lib/ui/keyboard/roving-menu";
  import { formatCount } from "@/lib/util/format";
  import SwitcherActions from "@/lib/workspace/switcher/SwitcherActions.svelte";
  import type { ComponentProps } from "svelte";

  type ActionsProps = ComponentProps<typeof SwitcherActions>;

  // The rail's head: its title over how many chats are working and how many
  // wait on you, the New menu (open the full picker, start a throwaway
  // workspace, or spawn an empty window or the agents dashboard) and the
  // button that folds the rail to a strip and back. Folded, only that button
  // shows.
  const { onswitch, onopenthrowaway, workingCount, needsYouCount, isCollapsed, ontogglecollapse }: {
    onswitch?: ActionsProps["onswitch"];
    /** Start a throwaway workspace in this window. */
    onopenthrowaway: () => void;
    /** How many chats across the rail are working. */
    workingCount: number;
    /** How many chats across the rail wait on the user. */
    needsYouCount: number;
    /** The rail is folded to a strip. */
    isCollapsed: boolean;
    ontogglecollapse: () => void;
  } = $props();

  const MENU_ID = "sidebar-project-actions";
  const NEW_LABEL = "New";
  const NEW_TOOLTIP = "Open a project, a throwaway workspace or a window";
  const COLLAPSE_LABEL = "Collapse the projects";
  const EXPAND_LABEL = "Show the projects";
  const SINGLE = 1;
  const NEED_YOU_PLURAL = "need you";

  const toggleLabel = $derived.by(() => {
    if (isCollapsed) {
      return EXPAND_LABEL;
    }

    return COLLAPSE_LABEL;
  });
  const needsYouNoun = $derived.by(() => {
    if (needsYouCount === SINGLE) {
      return NEEDS_YOU;
    }

    return NEED_YOU_PLURAL;
  });

  function hideMenu() {
    document.getElementById(MENU_ID)?.hidePopover();
  }
</script>

<div class="sidebar-head" class:collapsed={isCollapsed}>
  {#if !isCollapsed}
    <div class="title">
      <h2 class="section-label">Projects</h2>
      <p class="activity">
        <span class="working">{formatCount(workingCount)} working</span>
        ·
        <span class="needs-you">{formatCount(needsYouCount)} {needsYouNoun}</span>
      </p>
    </div>
    <button
      class="new"
      aria-haspopup="menu"
      data-tooltip={NEW_TOOLTIP}
      popovertarget={MENU_ID}
      type="button"
    ><Icon name="plus" size={14} />{NEW_LABEL}</button>
    <div id={MENU_ID} class="actions-menu popover-menu" {@attach rovingMenu} popover role="menu">
      <SwitcherActions
        onspawn={async mode => {
          await sendMessage(Command.windowCreate, { mode });
          hideMenu();
        }}
        onswitch={onswitch === undefined
          ? undefined
          : () => {
            hideMenu();
            onswitch();
          }}
        onthrowaway={async opensNewWindow => {
          hideMenu();

          if (opensNewWindow) {
            await sendMessage(Command.windowCreate, { mode: WindowMode.enum.temp });
            return;
          }

          onopenthrowaway();
        }}
      />
    </div>
  {/if}
  <button
    class="collapse"
    aria-expanded={!isCollapsed}
    aria-label={toggleLabel}
    data-tooltip={toggleLabel}
    onclick={ontogglecollapse}
    type="button"
  ><Icon name="sidebar" size={16} /></button>
</div>

<style>
  .sidebar-head {
    display: flex;
    flex: none;
    gap: 6px;
    align-items: center;
    padding-block: 8px 4px;
    padding-inline: 14px 8px;

    &.collapsed {
      justify-content: center;
      padding-inline: 0;
    }

    h2 {
      margin: 0;
      font-size: 0.6875rem;

      @media (prefers-reduced-motion: no-preference) {
        :global(:root[data-palette="cyberpunk"]) .sidebar-head:hover & {
          animation: var(--cyber-jitter-animation);
        }
      }
    }

    /* Cyberpunk: a red accent bar leads the head (the title's label look is
       the shared `.section-label`). */
    :global(:root[data-palette="cyberpunk"]) & {
      padding-block: 0.625rem 0.375rem;

      &:not(.collapsed)::before {
        content: "";
        flex: none;
        align-self: stretch;
        inline-size: 0.3125rem;
        margin-inline-end: 0.25rem;
        background: var(--cyber-accent);
      }

      h2 {
        font-size: 0.625rem;
      }
    }
  }

  /* The title takes the free space, so the buttons sit at the end. */
  .title {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 0.0625rem;
    min-inline-size: 0;
  }

  .activity {
    overflow: hidden;
    margin: 0;
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
    text-overflow: ellipsis;
    white-space: nowrap;

    .needs-you {
      color: var(--warning);
      font-weight: 700;
    }
  }

  .new,
  .collapse {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 1.75rem;
    padding: 0;
    border: none;
    border-radius: var(--radius-small);
    cursor: pointer;
    transition: background 120ms var(--ease), color 120ms var(--ease);
  }

  /* The filled New button - the head's one call to action. */
  .new {
    gap: 0.25rem;
    padding-inline: 0.4375rem 0.625rem;
    background: var(--primary-container);
    color: var(--on-primary-container);
    font: inherit;
    font-weight: 700;
    font-size: 0.7188rem;

    &:hover,
    &:has(+ .actions-menu:popover-open) {
      background: color-mix(in sRGB, var(--primary) 22%, var(--primary-container));
    }

    /* Its hover tooltip must not peek out over its own open menu. */
    &:has(+ .actions-menu:popover-open)::after {
      display: none;
    }

    > :global(svg) {
      font-size: 0.875rem;
    }

    :global(:root[data-palette="cyberpunk"]) & {
      text-shadow: var(--cyber-glow);
    }
  }

  .collapse {
    inline-size: 1.75rem;
    background: transparent;
    color: var(--on-surface-variant);
    font-size: 1rem;

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
    }
  }

  /* Shell comes from the shared .popover-menu; it hangs off the trigger,
     opening into the workspace rather than off the window's edge. */
  .actions-menu {
    inline-size: 300px;
    max-inline-size: 92vw;
    color: var(--on-surface);
    position-area: bottom span-right;
  }
</style>
