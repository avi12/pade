<script lang="ts">
  // The kinds of reset the account holds, as one native radio group, so the
  // confirmation can ask which to spend. Each option says what it frees up and
  // how many are left; the pick is lit via :has.
  import type { LimitReset } from "@/lib/types";
  import { limitResetKind, limitResetSummary, NOTHING_TO_RESET } from "@/lib/usage/limit-reset/limit-reset-kind";

  const { offers, chosenGrantId, isLocked, onchoose }: {
    offers: readonly LimitReset[];
    chosenGrantId: string;
    /** While a claim is on its way the pick can't change. */
    isLocked: boolean;
    onchoose: (grantId: string) => void;
  } = $props();
</script>

<fieldset class="reset-choice" disabled={isLocked}>
  <legend class="visually-hidden">Which reset to use</legend>
  {#each offers as offer (offer.grantId)}
    {@const kind = limitResetKind(offer.kind)}
    <label>
      <input
        name="limit-reset-choice"
        checked={offer.grantId === chosenGrantId}
        disabled={!offer.hasUsageToClear}
        onchange={() => onchoose(offer.grantId)}
        type="radio"
      />
      <span class="option-text">
        <span class="option-head">
          <span class="option-name">{kind.title}</span>
          <span class="option-meta">{limitResetSummary(offer)}</span>
        </span>
        <span class="option-effect">
          {#if offer.hasUsageToClear}
            {kind.effect}
          {:else}
            {NOTHING_TO_RESET}
          {/if}
        </span>
      </span>
    </label>
  {/each}
</fieldset>

<style>
  .reset-choice {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    border: 0;

    label {
      display: flex;
      gap: 10px;
      align-items: start;
      padding: 8px 10px;
      border: 1px solid var(--outline-variant);
      border-radius: var(--radius-small);
      background: var(--surface-2);
      cursor: pointer;
      transition:
        background 120ms var(--ease),
        border-color 120ms var(--ease);

      &:hover {
        background: var(--surface-3);
      }

      &:has(input:checked) {
        border-color: var(--primary);
        background: var(--surface-3);
      }

      &:has(input:focus-visible) {
        outline: 2px solid var(--primary);
        outline-offset: 1px;
      }
    }

    input {
      flex: none;
      margin: 0;
      margin-block-start: 2px;
      accent-color: var(--primary);
    }

    &:disabled label,
    label:has(input:disabled) {
      opacity: 60%;
      cursor: default;
    }
  }

  .option-text {
    display: grid;
    gap: 2px;
    min-inline-size: 0;
  }

  .option-head {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 8px;
    align-items: baseline;
  }

  .option-name {
    color: var(--on-surface);
    font-weight: 600;
    font-size: 0.8125rem;
  }

  .option-meta {
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 0.6875rem;
  }

  .option-effect {
    color: var(--on-surface-variant);
    font-size: 0.75rem;
  }

  @media (prefers-reduced-motion: reduce) {
    .reset-choice label {
      transition: none;
    }
  }
</style>
