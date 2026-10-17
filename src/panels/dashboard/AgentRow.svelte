<script lang="ts">
  import { agentIconName } from "@/lib/agent/agent-icon";
  import { AGENT_STATE_TEXT, agentState } from "@/lib/dashboard/agent-board";
  import type { BoardAgent } from "@/lib/dashboard/agent-board";
  import Icon from "@/lib/Icon.svelte";
  import { formatAge, formatTimestamp } from "@/lib/util/format";
  import SummaryText from "@/panels/dashboard/SummaryText.svelte";

  // One agent on the dashboard: its tab's name, what state it is in and for how
  // long, the model it runs, and the AI summary of what it has done. A click
  // anywhere on the card brings its window forward on this tab - through a
  // button stretched over the card, so the summary's links can still be links.
  const { agent, now, onfocus }: {
    agent: BoardAgent;
    now: number;
    onfocus: () => Promise<void>;
  } = $props();

  const state = $derived(agentState(agent));
</script>

<li class="agent" data-state={state}>
  <button class="open" onclick={onfocus} type="button">
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
  </button>
  {#if agent.summary}
    <p class="summary"><SummaryText text={agent.summary} /></p>
  {:else}
    <p class="summary pending">No summary yet - it comes once the agent has done some work</p>
  {/if}
</li>

<style>
  .agent {
    --state-color: var(--on-surface-variant);

    position: relative;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-small);
    background: var(--surface-2);
    color: var(--on-surface);
    text-align: start;
    transition: background 120ms var(--ease), border-color 120ms var(--ease);

    &:has(.open:hover, .open:focus-visible) {
      border-color: var(--primary);
      background: var(--surface-3);
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

  /* The card's one action, stretched over all of it by its ::after. */
  .open {
    display: flex;
    flex-direction: column;
    gap: 4px;
    justify-content: flex-start;
    align-items: stretch;
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;

    &::after {
      content: "";
      position: absolute;
      inset: 0;
    }

    &:hover,
    &:focus-visible {
      outline: none;
      filter: none;
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

  /* Shown whole: the summary is written to fit two lines (summary.rs), never
     clipped or faded to make it fit. */
  .summary {
    margin: 0;
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
