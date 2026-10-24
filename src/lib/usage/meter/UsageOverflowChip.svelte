<script lang="ts">
  import { overflowCount, overflowLevel, severityClass } from "@/lib/usage/meter/usage-meter";
  import type { AgentGroup } from "@/lib/usage/usage-groups";
  import { formatCount, formatQuantity } from "@/lib/util/format";

  // The trigger's "+N" for the agents past its pills: a muted count, with a
  // severity dot when any agent is hot.
  const { groups }: { groups: AgentGroup[] } = $props();

  const overflow = $derived(overflowCount(groups));
  const overflowSeverity = $derived(overflowLevel(groups));
  const overflowTooltip = $derived(
    `${formatQuantity({
      count: overflow,
      noun: "more agent"
    })} - open for the full list`
  );
</script>

{#if overflow > 0}
  <span class={["overflow-chip severity", severityClass(overflowSeverity)]} data-tooltip={overflowTooltip}>
    {#if overflowSeverity}
      <span class="overflow-dot"></span>
    {/if}
    +{formatCount(overflow)}
  </span>
{/if}

<style>
  .overflow-chip {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    padding-block: 3px;
    padding-inline: 7px;
    border-radius: var(--radius-small);
    background: var(--surface-1);
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 0.6875rem;

    .overflow-dot {
      flex: none;
      block-size: 6px;
      inline-size: 6px;
      border-radius: var(--radius-full);
      background: var(--severity-color);
    }
  }
</style>
