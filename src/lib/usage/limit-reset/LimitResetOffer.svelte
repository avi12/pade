<script lang="ts">
  // The usage menu's rows for Claude's free usage-limit resets (claude.ai's
  // "Reset for free"), one per kind, each spent on its own - a weekly reset
  // apart from a Fable one, as claude.ai offers them. A kind with nothing to
  // clear is greyed out, as there. The offers and their confirmation live in
  // stores/limitReset, shared with the collapsed meter's chip.
  import Icon from "@/lib/Icon.svelte";
  import { askToResetLimits, limitResetOffers } from "@/lib/stores/usage/limitReset.svelte";
  import { revealBlock } from "@/lib/ui/motion";
  import { tooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { limitResetKind, limitResetSummary, NOTHING_TO_RESET } from "@/lib/usage/limit-reset/limit-reset-kind";
</script>

<!-- The rows sit in the usage menu's scrolling popover, where a `data-tooltip`
     pseudo is clipped: the hint takes the shared top-layer bubble. -->
{#each limitResetOffers() as offer (offer.grantId)}
  {@const kind = limitResetKind(offer.kind)}
  <div
    class="limit-reset-offer"
    class:is-idle={!offer.hasUsageToClear}
    {@attach tooltip(() => offer.hasUsageToClear ? kind.effect : NOTHING_TO_RESET)}
    transition:revealBlock
  >
    <Icon name="refresh" />
    <span class="offer-text">
      {kind.title}
      <span class="offer-meta">{limitResetSummary(offer)}</span>
    </span>
    <button
      class="offer-button"
      disabled={!offer.hasUsageToClear}
      onclick={() => offer.hasUsageToClear && askToResetLimits(offer.grantId)}
      type="button"
    >Use it</button>
  </div>
{/each}

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

    .is-idle & {
      opacity: 60%;
    }
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

    &:disabled {
      color: var(--on-surface-variant);
      opacity: 60%;
      cursor: not-allowed;
    }

    &:hover:not(:disabled) {
      background: var(--surface-3);
    }
  }
</style>
