<script lang="ts">
  // The usage panel's "Usage resets" card: Claude's free usage-limit resets
  // (claude.ai's "Reset for free"), how many are available, then one block per
  // kind (LimitResetItem) - a weekly reset apart from a Fable one, as claude.ai
  // offers them. The offers and their confirmation live in stores/limitReset.
  import Icon from "@/lib/Icon.svelte";
  import { limitResetOffers } from "@/lib/stores/usage/limitReset.svelte";
  import { revealBlock } from "@/lib/ui/motion";
  import { limitResetTotal } from "@/lib/usage/limit-reset/limit-reset-kind";
  import LimitResetItem from "@/lib/usage/limit-reset/LimitResetItem.svelte";
  import { formatCount } from "@/lib/util/format";

  const offers = $derived(limitResetOffers());
  const total = $derived(limitResetTotal(offers));
</script>

{#if offers.length > 0}
  <section class="resets" aria-label="Usage resets" transition:revealBlock>
    <header class="resets-head">
      <span class="eyebrow section-label"><Icon name="refresh" />Usage resets</span>
      <span class="available">{formatCount(total)} available</span>
    </header>

    {#each offers as offer (offer.grantId)}
      <LimitResetItem {offer} />
    {/each}

    <span class="footnote">Each use spends one reset. Unused resets disappear when they expire</span>
  </section>
{/if}

<style>
  .resets {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-block-end: 14px;
    padding-block: 12px;
    padding-inline: 13px;
    border: 1px solid var(--primary);
    border-radius: var(--radius-medium);
    background: color-mix(in oklab, var(--primary) 7%, var(--surface-1));
  }

  .resets-head {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: center;

    .section-label {
      display: inline-flex;
      gap: 6px;
      align-items: center;
    }
  }

  .available {
    padding-block: 2px;
    padding-inline: 8px;
    border-radius: var(--radius-full);
    background: var(--primary-container);
    color: var(--on-primary-container);
    font-weight: 700;
    font-size: 0.6875rem;
  }

  .footnote {
    color: var(--on-surface-variant);
    font-size: 0.625rem;
  }
</style>
