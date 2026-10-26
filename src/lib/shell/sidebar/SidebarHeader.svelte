<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { WindowMode } from "@/lib/types";
  import { rovingMenu } from "@/lib/ui/keyboard/roving-menu";
  import SwitcherActions from "@/lib/workspace/switcher/SwitcherActions.svelte";
  import type { ComponentProps } from "svelte";

  type ActionsProps = ComponentProps<typeof SwitcherActions>;

  // The rail's head: its title and the window actions menu - open the full
  // picker, start a throwaway workspace, or spawn an empty window or the agents
  // dashboard.
  const { onswitch, onopenthrowaway }: {
    onswitch?: ActionsProps["onswitch"];
    /** Start a throwaway workspace in this window. */
    onopenthrowaway: () => void;
  } = $props();

  const MENU_ID = "sidebar-project-actions";
  const MENU_LABEL = "Window actions";

  function hideMenu() {
    document.getElementById(MENU_ID)?.hidePopover();
  }
</script>

<div class="sidebar-head">
  <h2>Projects</h2>
  <button
    class="actions-trigger"
    aria-haspopup="menu"
    aria-label={MENU_LABEL}
    data-tooltip={MENU_LABEL}
    popovertarget={MENU_ID}
    type="button"
  ><Icon name="more" size={16} /></button>
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
</div>

<style>
  .sidebar-head {
    display: flex;
    flex: none;
    gap: 6px;
    justify-content: space-between;
    align-items: center;
    padding-block: 8px 4px;
    padding-inline: 14px 8px;

    h2 {
      margin: 0;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 0.6875rem;
      letter-spacing: 0.07em;
      text-transform: uppercase;
    }
  }

  .actions-trigger {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 26px;
    inline-size: 26px;
    padding: 0;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: background 120ms var(--ease), color 120ms var(--ease);

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
