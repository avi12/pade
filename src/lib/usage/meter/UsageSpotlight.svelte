<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { Spotlight } from "@/lib/usage/usage-groups";
  import { isJustReset, NO_READING, severityClass } from "@/lib/usage/usage-meter";

  // The single limit closest to its cap across all agents, as a big number over
  // a full-width track, with its countdown.
  const { spotlight, justReset }: {
    spotlight: Spotlight;
    justReset: ReadonlySet<string>;
  } = $props();

  // The countdown, phrased ("resets in 3h 30m").
  const spotlightReset = $derived.by(() => {
    if (!spotlight.limit.reset) {
      return NO_READING;
    }

    return `resets ${spotlight.limit.reset}`;
  });
</script>

<section class={["spotlight severity", severityClass(spotlight.limit.level)]}>
  <div class="spotlight-head">
    <span class="spotlight-id">
      <span class="agent-icon"><Icon name={spotlight.agent.icon} /></span>
      <span class="spotlight-text">
        <span class="eyebrow">Closest to a limit</span>
        <span class="spotlight-title">{spotlight.agent.name} · {spotlight.limit.label}</span>
      </span>
    </span>
    <output
      class="spotlight-percent"
      class:just-reset={isJustReset({
        justReset,
        agentId: spotlight.agent.id,
        limit: spotlight.limit
      })}
    >{spotlight.limit.reading}</output>
  </div>
  <span class="track">
    <span style:inline-size="{spotlight.limit.percentage}%" class="track-fill"></span>
  </span>
  <span class="spotlight-reset">{spotlightReset}</span>
</section>

<style>
  .spotlight {
    margin-block-end: 14px;
    padding: 12px 13px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);

    .spotlight-head {
      display: flex;
      gap: 10px;
      justify-content: space-between;
      align-items: center;
      margin-block-end: 9px;
    }

    .spotlight-id {
      display: inline-flex;
      gap: 8px;
      align-items: center;
      min-inline-size: 0;
    }

    .spotlight-text {
      display: flex;
      flex-direction: column;
      min-inline-size: 0;
      line-height: 1.2;
    }

    .eyebrow {
      font-size: 9px;
      letter-spacing: 0.09em;
    }

    .spotlight-title {
      overflow: hidden;
      font-weight: 700;
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .spotlight-percent {
      flex: none;
      color: var(--severity-color);
      font-weight: 800;
      font-size: 22px;
      line-height: 1;
    }

    .spotlight-reset {
      display: block;
      margin-block-start: 7px;
      color: var(--on-surface-variant);
      font-size: 11px;
    }
  }

  /* The full-width progress track + its severity-colored fill. */
  .track {
    display: block;
    overflow: clip;
    block-size: 8px;
    border-radius: var(--radius-full);
    background: var(--surface-3);
  }

  .track-fill {
    display: block;
    block-size: 100%;
    border-radius: var(--radius-full);
    background: var(--severity-color);
    transition: inline-size 300ms var(--ease), background 300ms var(--ease);
  }
</style>
