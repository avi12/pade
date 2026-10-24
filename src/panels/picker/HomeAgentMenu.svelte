<script lang="ts">
  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import type { Agent } from "@/lib/types";

  // The composer's LLM menu: every installed coding agent, a tick on the
  // picked one.
  const { menuId, agents, selectedAgent, onpick }: {
    menuId: string;
    agents: readonly Agent[];
    selectedAgent: string | null;
    onpick: (agentId: string) => void;
  } = $props();
</script>

<ul id={menuId} style:position-anchor="--{menuId}" class="menu popover-menu" popover>
  {#each agents as agent (agent.id)}
    {@const isPicked = selectedAgent === agent.id}
    <li>
      <button
        class="menu-item"
        class:picked={isPicked}
        aria-current={isPicked}
        onclick={() => onpick(agent.id)}
        popovertarget={menuId}
        popovertargetaction="hide"
        type="button"
      >
        <Icon name={agentIconName(agent.id)} size={15} />
        <span class="menu-item-text">{agent.label}</span>
        {#if isPicked}
          <span class="tick" aria-hidden="true">✓</span>
        {/if}
      </button>
    </li>
  {/each}
</ul>
