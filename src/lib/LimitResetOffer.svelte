<script lang="ts">
  // claude.ai's "Reset for free" in the usage menu: when Claude's account holds a
  // free usage-limit reset (a launch promotion, say), offer to spend it here.
  // Spending it can't be undone, so the button only opens a confirmation.
  import { usage as usageApi } from "@/lib/bridge";
  import ConfirmDialog from "@/lib/ConfirmDialog.svelte";
  import { errorMessage } from "@/lib/errors";
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import type { AccountUsage, LimitReset } from "@/lib/types";
  import { untrack } from "svelte";

  const { account, onreset }: {
    /** Claude's latest usage reading — each new one re-checks the offer, so it
     *  follows the meter's own refresh instead of keeping a timer of its own. */
    account: AccountUsage | null | undefined;
    /** Re-read the account once its limits were reset. */
    onreset: () => Promise<void>;
  } = $props();

  let offer = $state<LimitReset | null>(null);
  let isConfirming = $state(false);
  let isClaiming = $state(false);
  let failure = $state<string | null>(null);

  const expiry = $derived.by(() => {
    if (!offer?.endsAt) {
      return "";
    }

    const date = new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric"
    }).format(new Date(offer.endsAt));
    return ` · use by ${date}`;
  });

  const spentReset = $derived.by(() => {
    if (offer?.resetsLeft === 1) {
      return "your one free reset";
    }

    return "one of your free resets";
  });

  async function loadOffer() {
    offer = await usageApi.limitResetOffer().catch(() => null);
  }

  $effect(() => {
    if (account === undefined) {
      return;
    }

    untrack(loadOffer);
  });
</script>

{#if offer}
  <div class="limit-reset-offer" data-tooltip={offer.label}>
    <Icon name="refresh" />
    <span class="offer-text">
      Free limit reset
      <span class="offer-meta">{formatCount(offer.resetsLeft)} left{expiry}</span>
    </span>
    <button
      class="offer-button"
      onclick={() => {
        failure = null;
        isConfirming = true;
      }}
      type="button"
    >Reset for free</button>
  </div>

  {#if isConfirming}
    <ConfirmDialog
      busy={isClaiming}
      busyLabel="Resetting…"
      confirmLabel="Reset now"
      error={failure}
      icon="refresh"
      nested
      oncancel={() => {
        if (!isClaiming) {
          isConfirming = false;
        }
      }}
      onconfirm={async () => {
        if (!offer) {
          return;
        }

        isClaiming = true;
        try {
          const outcome = await usageApi.claimLimitReset(offer.grantId);
          if (!outcome.reset) {
            failure = `Not reset: ${outcome.reason ?? outcome.result}`;
            return;
          }

          isConfirming = false;
          await onreset();
          await loadOffer();
        } catch (error) {
          failure = errorMessage({
            error,
            fallback: "Couldn't reach Claude to reset the limits."
          });
        } finally {
          isClaiming = false;
        }
      }}
      title="Reset your usage limits?"
    >
      <p>
        Clears Claude's usage limits right now. This spends {spentReset}, and it can't be undone.
      </p>
    </ConfirmDialog>
  {/if}
{/if}

<style>
  .limit-reset-offer {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-block-start: 10px;
    color: var(--on-surface-variant);
    font-size: 11px;
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
    font-size: 10px;
  }

  .offer-button {
    padding: 4px 10px;
    border: 1px solid var(--outline-variant);
    border-radius: var(--radius-small);
    background: var(--surface-2);
    color: var(--on-surface);
    font-weight: 600;
    font-size: 11px;
    cursor: pointer;

    &:hover {
      background: var(--surface-3);
    }
  }
</style>
