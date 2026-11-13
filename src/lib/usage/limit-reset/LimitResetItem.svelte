<script lang="ts">
  // One kind of free reset in the "Usage resets" card: what it works on, how
  // many are left, when the first expires and what it clears, with its own
  // button - greyed out while there is nothing for it to clear.
  import { askToResetLimits } from "@/lib/stores/usage/limitReset.svelte";
  import type { LimitReset } from "@/lib/types/usage";
  import { limitResetExpiry, limitResetKind, NOTHING_TO_RESET } from "@/lib/usage/limit-reset/limit-reset-kind";
  import { formatCount } from "@/lib/util/format";

  const { offer }: {
    offer: LimitReset;
  } = $props();

  const kind = $derived(limitResetKind(offer.kind));
  const expiry = $derived(
    limitResetExpiry({
      offer,
      now: Date.now()
    })
  );
</script>

<div class="reset" class:is-idle={!offer.hasUsageToClear}>
  <div class="reset-head">
    <span class="scope">{kind.scope}</span>
    <span class="title">{kind.title}</span>
    <span class="count">×{formatCount(offer.resetsLeft)}</span>
  </div>
  {#if expiry}
    <div class="expiry" class:is-soon={expiry.isSoon}>
      <span>{expiry.countdown}</span>
      <span class="date">{expiry.date}</span>
    </div>
  {/if}
  <span class="effect">
    {#if offer.hasUsageToClear}
      {kind.effect}
    {:else}
      {NOTHING_TO_RESET}
    {/if}
  </span>
  <button
    class="use"
    disabled={!offer.hasUsageToClear}
    onclick={() => offer.hasUsageToClear && askToResetLimits(offer.grantId)}
    type="button"
  >Use 1 reset</button>
</div>

<style>
  .reset {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border-radius: var(--radius-small);
    background: var(--surface-2);
  }

  .reset-head {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .scope {
    flex: none;
    padding-block: 2px;
    padding-inline: 6px;
    border-radius: var(--radius-small);
    background: var(--primary-container);
    color: var(--on-primary-container);
    font-weight: 800;
    font-size: 0.5625rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .title {
    flex: 1;
    overflow: hidden;
    min-inline-size: 0;
    color: var(--on-surface);
    font-weight: 700;
    font-size: 0.75rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    flex: none;
    color: var(--on-surface);
    font-weight: 800;
    font-size: 0.75rem;
  }

  .expiry {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: baseline;
    color: var(--on-surface-variant);
    font-weight: 500;
    font-size: 0.6563rem;

    &.is-soon {
      color: var(--warning);
      font-weight: 700;
    }

    .date {
      color: var(--on-surface-variant);
      font-weight: 500;
    }
  }

  .effect {
    color: var(--on-surface-variant);
    font-size: 0.625rem;
    line-height: 1.4;
  }

  .use {
    align-self: flex-start;
    padding-block: 6px;
    padding-inline: 12px;
    border: none;
    border-radius: var(--radius-full);
    background: var(--primary);
    color: var(--on-primary);
    font-weight: 700;
    font-size: 0.6875rem;
    cursor: pointer;

    &:disabled {
      background: var(--surface-3);
      color: var(--on-surface-variant);
      cursor: not-allowed;
    }

    &:hover:not(:disabled) {
      filter: brightness(1.08);
    }
  }

  .reset.is-idle :is(.title, .count) {
    opacity: 0.6;
  }
</style>
