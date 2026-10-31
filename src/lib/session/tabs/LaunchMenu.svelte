<script lang="ts" module>
  /** The tab strip's launch dropdown - session-shortcuts opens it by id from a shortcut. */
  export const ADD_SESSION_MENU_ID = "add-session-menu";
  /** The anchor the launch dropdown hangs off - set on the strip's "+". */
  export const ADD_MENU_ANCHOR = "--add-session-anchor";
</script>

<script lang="ts">
  // The launch dropdown behind the strip's "+": every agent, plus the local
  // branches to start on as a new worktree.

  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import type { Agent } from "@/lib/types";
  import { rovingMenu } from "@/lib/ui/keyboard/roving-menu";

  const { agents, branches, onlaunch, onlaunchbranch }: {
    agents: Agent[];
    /** Local branches when the project is a git repo - offered as worktrees. */
    branches: string[];
    onlaunch: (agent: Agent) => void;
    onlaunchbranch: (branch: string) => Promise<void>;
  } = $props();
</script>

<ul
  id={ADD_SESSION_MENU_ID}
  style:position-anchor={ADD_MENU_ANCHOR}
  class="menu popover-menu"
  {@attach rovingMenu}
  popover
>
  <li class="menu-separator">Launch an agent</li>
  {#each agents as agent (agent.id)}
    <li>
      <button
        class="menu-item"
        onclick={() => onlaunch(agent)}
        popovertarget={ADD_SESSION_MENU_ID}
        popovertargetaction="hide"
      ><span class="launch-icon"><Icon name={agentIconName(agent.id)} /></span>{agent.label}</button>
    </li>
  {/each}
  {#if branches.length > 0}
    <li class="menu-divider" role="separator"></li>
    <li class="menu-separator">On a branch - new worktree</li>
    <li class="branch-section">
      <ul class="branch-list">
        {#each branches as branch (branch)}
          <li>
            <button
              class="menu-item branch-item"
              onclick={async () => await onlaunchbranch(branch)}
              popovertarget={ADD_SESSION_MENU_ID}
              popovertargetaction="hide"
            ><span class="branch-icon"><Icon name="git" /></span>{branch}</button>
          </li>
        {/each}
      </ul>
    </li>
  {/if}
</ul>

<style>
  /* Shell and rows come from the shared .popover-menu and .menu-item; only
     width and anchor side live here. */
  .menu {
    /* Percentages resolve against the position-area cell - the room below the
       "+" - so the menu stops at the window's foot and the branches scroll
       while the agents stay in view. */
    display: flex;
    flex-direction: column;
    max-block-size: calc(100% - var(--popover-menu-offset));
    min-inline-size: 220px;
    position-area: bottom span-right;

    .branch-section {
      display: flex;
      flex-direction: column;
      min-block-size: 0;
    }

    .branch-list {
      overflow-y: auto;
      overscroll-behavior: contain;
      margin: 0;
      padding: 0;
      list-style: none;
    }

    /* Leading glyph tints: agents read primary, branches read tertiary (git). */
    .launch-icon {
      display: inline-flex;
      color: var(--primary);
    }

    .branch-icon {
      display: inline-flex;
      color: var(--tertiary);
    }

    /* Branch rows spell the branch name in the mono face. */
    .branch-item {
      font-family: var(--font-monospace);
    }
  }
</style>
