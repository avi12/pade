<script lang="ts">
  // The usage menu's row for Claude's free usage-limit reset (claude.ai's "Reset
  // for free"). The offer and its confirmation live in stores/limitReset, shared
  // with the collapsed meter's chip.
  import Icon from "@/lib/Icon.svelte";
  import { askToResetLimits, limitResetOffer, limitResetSummary } from "@/lib/stores/usage/limitReset.svelte";
  import { revealBlock } from "@/lib/ui/motion";

  const offer = $derived(limitResetOffer());
</script>

{#if offer}
  <div class="limit-reset-offer" data-tooltip={offer.label} transition:revealBlock>
    <Icon name="refresh" />
    <span class="offer-text">
      Free limit reset
      <span class="offer-meta">{limitResetSummary()}</span>
    </span>
    <button class="offer-button" onclick={askToResetLimits} type="button">Reset for free</button>
  </div>
{/if}

<style>
  .limit-reset-offer {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-block-start: 10px;
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
  }

  .offer-text {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-inline-size: 0;
    color: var(--on-surface);
    font-weight: 600;
  }

  .offer-meta {
    color: var(--on-surface-variant);
    font-weight: 400;
    font-size: 0.625rem;
  }

  .offer-button {
    padding: 4px 10px;
    border: 1px solid var(--outline-variant);
    border-radius: var(--radius-small);
    background: var(--surface-2);
    color: var(--on-surface);
    font-weight: 600;
    font-size: 0.6875rem;
    cursor: pointer;

    &:hover {
      background: var(--surface-3);
    }
  }
</style>
