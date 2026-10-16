<script lang="ts">
  import { agentIconName } from "@/lib/agent/agent-icon";
  import { AGENT_STATE_TEXT, agentState } from "@/lib/dashboard/agent-board";
  import type { BoardAgent } from "@/lib/dashboard/agent-board";
  import Icon from "@/lib/Icon.svelte";
  import { formatAge, formatTimestamp } from "@/lib/util/format";

  // One agent on the dashboard: its tab's name, what state it is in and for how
  // long, the model it runs, and the AI summary of what it has done. A click
  // brings its window forward on this tab.
  const { agent, now, onfocus }: {
    agent: BoardAgent;
    now: number;
    onfocus: () => Promise<void>;
  } = $props();

  const state = $derived(agentState(agent));
</script>

<li>
  <button class="agent" data-state={state} onclick={onfocus} type="button">
    <span class="head">
      <Icon name={agentIconName(agent.agentId)} size={16} />
      <span class="name">{agent.name}</span>
      <span class="state" data-tooltip={formatTimestamp(agent.statusSince)}>
        <span class="dot"></span>
        {AGENT_STATE_TEXT[state]} · {formatAge({
          stamp: agent.statusSince,
          now
        })}
      </span>
    </span>
    {#if agent.model}
      <span class="model">{agent.agentLabel} · {agent.model}</span>
    {/if}
    {#if agent.summary}
      <span class="summary">{agent.summary}</span>
    {:else}
      <span class="summary pending">No summary yet - it comes once the agent has done some work</span>
    {/if}
  </button>
</li>

<style>
  .agent {
    --state-color: var(--on-surface-variant);

    display: flex;
    flex-direction: column;
    gap: 4px;
    inline-size: 100%;
    padding: 10px 12px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-small);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: background 120ms var(--ease), border-color 120ms var(--ease);

    &:hover,
    &:focus-visible {
      border-color: var(--primary);
      background: var(--surface-3);
      filter: none;
    }

    &[data-state="waiting"] {
      --state-color: var(--critical);

      border-color: var(--critical);
    }

    &[data-state="working"] {
      --state-color: var(--tertiary);
    }

    &[data-state="starting"] {
      --state-color: var(--warning);
    }
  }

  .head {
    display: flex;
    gap: 8px;
    align-items: center;
    min-inline-size: 0;
  }

  .name {
    flex: 1;
    overflow: hidden;
    font-weight: 600;
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .state {
    display: inline-flex;
    flex-shrink: 0;
    gap: 6px;
    align-items: center;
    color: var(--state-color);
    font-family: var(--font-monospace);
    font-size: 11px;
  }

  .dot {
    block-size: 7px;
    inline-size: 7px;
    border-radius: var(--radius-full);
    background: var(--state-color);

    [data-state="working"] & {
      animation: pulse 1600ms var(--ease) infinite;
    }
  }

  .model {
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 11px;
  }

  .summary {
    font-size: 12.5px;
    line-height: 1.45;

    &.pending {
      color: var(--on-surface-variant);
      font-style: italic;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dot {
      animation: none;
    }
  }
</style>
