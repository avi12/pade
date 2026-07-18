<script lang="ts">
  import { agentIconName } from "@/lib/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import type { Agent } from "@/lib/types";

  // A radiogroup of agent choice-chips — one pill per agent, its brand glyph
  // leading the label, the selected one edged in primary over its container.
  // Shared by the picker's Default-agent section and the create form's
  // per-launch Agent row (DRY); `compact` trims the padding for the denser
  // in-form row.
  const { agents, selected, onpick, ariaLabel, compact = false }: {
    agents: Agent[];
    selected: string | null;
    onpick: (agentId: string) => void;
    ariaLabel: string;
    compact?: boolean;
  } = $props();
</script>

<div class="chips" class:compact aria-label={ariaLabel} role="radiogroup">
  {#each agents as agent (agent.id)}
    {@const isSelected = selected === agent.id}
    <button
      class="chip"
      class:on={isSelected}
      aria-checked={isSelected}
      data-agent={agent.id}
      onclick={() => onpick(agent.id)}
      role="radio"
      type="button"
    >
      <Icon name={agentIconName(agent.id)} size={15} />{agent.label}
    </button>
  {/each}
</div>

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  /* Choice chips — pills; selected gets a primary edge over its container, with
     the agent's own glyph leading the label. The glyph carries the agent's brand
     colour (--agent-brand, set per data-agent in theme.css); a monochrome-brand
     agent has none and its glyph follows the label colour. */
  .chip {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    padding: 8px 16px;
    border: 1px solid transparent;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition: border-color 150ms var(--ease);

    :global(.icon) {
      color: var(--agent-brand, currentColor);
    }

    &:hover {
      border-color: var(--primary);
    }

    &.on {
      border-color: var(--primary);
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  /* The create form's Agent row reads a touch more compact than the standalone
     Default-agent chips. */
  .compact .chip {
    padding: 7px 14px;
  }
</style>
