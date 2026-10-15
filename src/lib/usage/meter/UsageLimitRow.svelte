<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { severityClass } from "@/lib/usage/meter/usage-meter";
  import type { Limit } from "@/lib/usage/usage-groups";

  // One limit in an agent card: kind badge, bar, percent, then its countdown -
  // or, on the extra-usage row, the credits balance. The row lays out on the
  // card's shared grid (`display: contents`), so the columns line up down it.
  const { limit, isJustReset }: {
    limit: Limit;
    /** It just started over, so its number flashes. */
    isJustReset: boolean;
  } = $props();

  const percentTooltip = $derived.by(() => {
    if (limit.isUncapped) {
      return "Spent on extra usage this month";
    }

    return undefined;
  });
  const resetTooltip = $derived(limit.resetAt || undefined);
</script>

<div class={["limit severity", severityClass(limit.level)]}>
  <span class="limit-kind">{limit.kindShort}</span>
  <span class="limit-track">
    <span style:inline-size="{limit.percentage}%" class="limit-fill"></span>
  </span>
  <output class="limit-percent" class:just-reset={isJustReset} data-tooltip={percentTooltip}>{limit.reading}</output>
  {#if limit.balance}
    <span class="limit-balance" data-tooltip="Prepaid usage credits left to spend">
      <Icon name="wallet" />{limit.balance} left
    </span>
  {:else}
    <span class="limit-reset" data-tooltip={resetTooltip}>
      {#if limit.reset}
        <Icon name="clock" />{limit.reset}
      {/if}
    </span>
  {/if}
</div>

<style>
  .limit {
    display: contents;
  }

  /* A limit's single-letter kind badge (S / W / O), tinted neutral. */
  .limit-kind {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 17px;
    min-inline-size: 17px;
    padding-inline: 3px;
    border-radius: 5px;
    background: var(--surface-3);
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-weight: 800;
    font-size: 9px;
  }

  .limit-track {
    display: block;
    overflow: clip;
    block-size: 7px;
    border-radius: var(--radius-full);
    background: var(--surface-3);
  }

  .limit-fill {
    display: block;
    block-size: 100%;
    border-radius: var(--radius-full);
    background: var(--severity-color);
    transition: inline-size 300ms var(--ease), background 300ms var(--ease);
  }

  .limit-percent {
    min-inline-size: 2.5rem;
    color: var(--severity-color);
    font-weight: 700;
    font-size: 11.5px;
    text-align: end;
  }

  /* Clock + live "in …" countdown - or the credits balance - right-aligned to
     the card edge. */
  .limit-reset,
  .limit-balance {
    display: inline-flex;
    gap: 3px;
    align-items: center;
    justify-self: end;
    color: var(--on-surface-variant);
    font-size: 10px;
    white-space: nowrap;
  }
</style>
