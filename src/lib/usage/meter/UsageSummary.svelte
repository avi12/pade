<script lang="ts">
  import { measuredAgentCount, severityClass } from "@/lib/usage/meter/usage-meter";
  import { type AgentGroup, severityBreakdown } from "@/lib/usage/usage-groups";
  import { formatCount, formatQuantity } from "@/lib/util/format";

  // The details panel's head: a title with the running count, a tally of agents
  // per severity, and one bar split by how the agents bucket on it.
  const { groups }: { groups: AgentGroup[] } = $props();

  const severitySlices = $derived(severityBreakdown(groups));
  const measuredAgents = $derived(measuredAgentCount(severitySlices));
  // Only the severities some agent is at take a tally and a segment.
  const presentSlices = $derived(severitySlices.filter(slice => slice.count > 0));
  const runningLabel = $derived(
    `${formatQuantity({
      count: groups.length,
      noun: "agent"
    })} running`
  );
</script>

<header class="summary">
  <div class="titles">
    <h2 class="title">Usage</h2>
    <p class="running">{runningLabel}</p>
  </div>
  <div class="severity-counts">
    {#each presentSlices as slice (slice.level)}
      <span class={["severity-count severity", severityClass(slice.level)]}>
        <span class="severity-dot"></span>
        <span class="severity-value">{formatCount(slice.count)}</span> {slice.label}
      </span>
    {/each}
  </div>
</header>

<div class="distribution">
  {#each presentSlices as slice (slice.level)}
    <span
      style:inline-size="{(slice.count / measuredAgents) * 100}%"
      class={["distribution-segment severity", severityClass(slice.level)]}
    ></span>
  {/each}
</div>

<style>
  /* ── Header: title + running count, plus per-severity agent tallies ── */
  .summary {
    display: flex;
    gap: 12px;
    justify-content: space-between;
    align-items: flex-start;
    margin-block-end: 11px;
  }

  .titles {
    display: flex;
    flex-direction: column;
    line-height: 1.15;
  }

  .title {
    margin: 0;
    font-weight: 700;
    font-size: 0.9375rem;
    letter-spacing: -0.01em;
  }

  .running {
    margin: 0;
    margin-block-start: 1px;
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
  }

  .severity-counts {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    justify-content: flex-end;
    padding-block-start: 2px;
  }

  .severity-count {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    color: var(--on-surface-variant);
    font-weight: 600;
    font-size: 0.6875rem;

    .severity-dot {
      flex: none;
      block-size: 8px;
      inline-size: 8px;
      border-radius: var(--radius-full);
      background: var(--severity-color);
    }
  }

  /* ── Distribution: one pill-track split by how agents bucket on severity ── */
  .distribution {
    display: flex;
    gap: 2px;
    overflow: clip;
    block-size: 6px;
    margin-block-end: 15px;
    border-radius: var(--radius-full);
    background: var(--surface-3);
  }

  .distribution-segment {
    min-inline-size: 2px;
    background: var(--severity-color);
    transition: inline-size 300ms var(--ease), background 300ms var(--ease);
  }
</style>
