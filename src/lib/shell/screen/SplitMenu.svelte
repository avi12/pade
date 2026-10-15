<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { Agent, AgentSession } from "@/lib/types";

  // Thin strip at the right of the terminal row that opens the split menu: add
  // a session that is not shown yet, or launch a new instance beside the panes.
  const { candidates, agents, onadd, onlaunch }: {
    candidates: AgentSession[];
    agents: Agent[];
    onadd: (id: string) => void;
    onlaunch: (agent: Agent) => void;
  } = $props();
</script>

<div class="add-pane-wrap menu-host">
  <button
    style:anchor-name="--pane-anchor"
    class="add-pane menu-trigger"
    aria-label="Split - add an agent instance"
    data-tooltip="Split - add an agent instance"
    popovertarget="pane-menu"
  ><Icon name="columns" /></button>
  <ul id="pane-menu" style:position-anchor="--pane-anchor" class="pane-menu popover-menu" popover>
    {#if candidates.length > 0}
      <li class="menu-separator">Add to split</li>
      {#each candidates as session (session.id)}
        <li>
          <button
            class="menu-item"
            onclick={() => onadd(session.id)}
            popovertarget="pane-menu"
            popovertargetaction="hide"
          >
            <Icon name="terminal" /> {session.agent.label}
          </button>
        </li>
      {/each}
    {/if}
    <li class="menu-separator">Launch a new instance</li>
    {#each agents as agent (agent.id)}
      <li>
        <button
          class="menu-item"
          onclick={() => onlaunch(agent)}
          popovertarget="pane-menu"
          popovertargetaction="hide"
        >{agent.label}</button>
      </li>
    {/each}
  </ul>
</div>

<style>
  /* Thin strip at the right of the terminal row that opens the split menu. */
  .add-pane-wrap {
    display: flex;
    flex: none;
    align-items: stretch;
  }

  .add-pane {
    display: inline-flex;
    justify-content: center;
    align-items: flex-start;
    inline-size: 44px;
    padding-block-start: 12px;
    border: none;
    border-inline-start: 1px solid var(--outline);
    background: var(--surface-1);
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: color 150ms var(--ease), background 150ms var(--ease);

    &:hover {
      background: var(--surface-2);
      color: var(--on-surface);
    }
  }

  /* Shell and rows come from the shared .popover-menu and .menu-item; the split
     menu opens to the left since its trigger sits at the right edge. */
  .pane-menu {
    min-inline-size: 220px;
    position-area: bottom span-left;
  }
</style>
