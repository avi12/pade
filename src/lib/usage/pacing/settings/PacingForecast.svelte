<script lang="ts">
  import { agentForecast, slowedSessionCount } from "@/lib/stores/usage/usagePacing.svelte";
  import type { PacedWindowKind } from "@/lib/types/pacing";
  import { pacingPreview } from "@/lib/usage/pacing/pacing-policy";
  import { formatPercent } from "@/lib/util/format";

  // One limit's live line for the active agent: how much is spent and when it
  // resets, when it runs out at this pace, and what pacing is doing about it.
  // A reading, so it sets in mono.
  const { agent, kind }: {
    agent: string;
    kind: PacedWindowKind;
  } = $props();

  const preview = $derived(
    pacingPreview({
      kind,
      forecast: agentForecast({
        agentId: agent,
        kind
      }),
      slowedCount: slowedSessionCount(),
      now: Date.now()
    })
  );
</script>

{#if preview}
  <output class="forecast">
    <span class="window">{preview.name} {formatPercent(preview.utilization)}</span>
    <span style:--used={`${preview.utilization}%`} class="meter"></span>
    <span class="resets">{preview.resets}</span>
    <span class="line">{preview.pace}</span>
    <span class="line action">{preview.action}</span>
  </output>
{/if}

<style>
  .forecast {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 4px 10px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 10px;
    border-inline-start: 2px solid var(--primary);
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-size: 0.6875rem;
  }

  .window {
    text-transform: capitalize;
  }

  .resets {
    color: var(--on-surface-variant);
  }

  .line {
    grid-column: 1 / -1;
    color: var(--on-surface-variant);
  }

  .action {
    color: var(--on-surface);
  }

  .meter {
    block-size: 6px;
    background:
      linear-gradient(var(--primary), var(--primary)) no-repeat 0 0 / var(--used) 100%,
      var(--surface-3);
  }
</style>
