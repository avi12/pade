<script lang="ts">
  import { agentForecast } from "@/lib/stores/usage/usagePacing.svelte";
  import type { PacedWindowKind, PacingSettings } from "@/lib/types";
  import { pacingPreview } from "@/lib/usage/pacing-policy";

  // One window's live line for the active agent: where its usage is heading -
  // how much is spent (and, weekly, a tick where an even spend would be), when
  // it runs out and resets - and what pacing would do about it now. A reading,
  // so it sets in mono.
  const { agent, kind, settings }: {
    agent: string;
    kind: PacedWindowKind;
    settings: PacingSettings;
  } = $props();

  const preview = $derived(
    pacingPreview({
      forecast: agentForecast({
        agentId: agent,
        kind
      }),
      settings,
      now: Date.now()
    })
  );
</script>

<output class="forecast">
  {#if preview.meter}
    <span class="window">{preview.meter.name}</span>
    <span
      style:--used={`${preview.meter.utilization}%`}
      style:--even={preview.meter.evenPercentage === null ? null : `${preview.meter.evenPercentage}%`}
      class="meter"
      class:has-even={preview.meter.evenPercentage !== null}
    ></span>
    <span class="detail">{preview.meter.detail}</span>
  {/if}
  <span class="would">{preview.wouldDo}</span>
</output>

<style>
  .forecast {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 10px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 10px;
    border-inline-start: 2px solid var(--primary);
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-size: 11px;
  }

  .detail,
  .would {
    grid-column: 1 / -1;
  }

  .detail {
    color: var(--on-surface-variant);
  }

  .meter {
    position: relative;
    block-size: 6px;
    background:
      linear-gradient(var(--primary), var(--primary)) no-repeat 0 0 / var(--used) 100%,
      var(--surface-3);

    &.has-even::after {
      content: "";
      position: absolute;
      inset-block: -3px;
      inset-inline-start: var(--even);
      inline-size: 2px;
      background: var(--on-surface);
    }
  }
</style>
