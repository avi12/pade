<script lang="ts">
  // The strip's trailing "+": a plain click launches another session of the
  // last tab's agent; Ctrl/Cmd-click or right-click opens the launch menu.

  import { TAB_SHORTCUTS, TabAction } from "@/lib/session/tab-shortcuts";
  import LaunchMenu, { ADD_MENU_ANCHOR, ADD_SESSION_MENU_ID } from "@/lib/session/tabs/LaunchMenu.svelte";
  import type { Agent } from "@/lib/types";

  const { agents, branches, lastAgent, onlaunch, onlaunchbranch }: {
    agents: Agent[];
    /** Local branches when the project is a git repo - offered as worktrees. */
    branches: string[];
    /** The agent a plain "+" click launches another of. */
    lastAgent: Agent | undefined;
    onlaunch: (agent: Agent) => void;
    onlaunchbranch: (branch: string) => Promise<void>;
  } = $props();

  const POPOVER_OPEN_SELECTOR = ":popover-open";

  const newTabShortcut = TAB_SHORTCUTS[TabAction.New];
  const launchMenuShortcut = TAB_SHORTCUTS[TabAction.LaunchMenu];
  const agentLabel = $derived(lastAgent?.label ?? "agent");

  function openAddMenu() {
    const elMenu = document.getElementById(ADD_SESSION_MENU_ID);
    if (elMenu instanceof HTMLElement && !elMenu.matches(POPOVER_OPEN_SELECTOR)) {
      elMenu.showPopover();
    }
  }
</script>

<span class="add-wrap menu-host">
  <button
    style:anchor-name={ADD_MENU_ANCHOR}
    class="add-button menu-trigger"
    aria-label={`New ${agentLabel} session - ${newTabShortcut.description}: ${newTabShortcut.label}; ${launchMenuShortcut.description}: ${launchMenuShortcut.label}`}
    data-tooltip={`New ${agentLabel} session · ${newTabShortcut.label} · Ctrl-click, right-click, or ${launchMenuShortcut.label} for options`}
    onclick={e => {
      if (e.ctrlKey || e.metaKey) {
        openAddMenu();
        return;
      }

      if (lastAgent) {
        onlaunch(lastAgent);
      }
    }}
    oncontextmenu={e => {
      e.preventDefault();
      openAddMenu();
    }}
  >+</button>
  <LaunchMenu {agents} {branches} {onlaunch} {onlaunchbranch} />
</span>

<style>
  /* Groups the add trigger with its popover as one menu-host, adding no box of
     its own so the button lays out in the strip exactly as before. */
  .add-wrap {
    display: contents;
  }

  .add-button {
    display: grid;
    flex: none;
    place-items: center;
    block-size: 30px;
    inline-size: 30px;
    border: none;
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--on-surface-variant);
    font-size: 18px;
    cursor: pointer;
    transition: color 150ms var(--ease), background 150ms var(--ease);

    &:hover {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }
</style>
