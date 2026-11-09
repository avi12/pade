<script lang="ts">
  // The one confirmation for spending one of Claude's free usage-limit resets,
  // opened from the collapsed meter's chip or a menu row. Holding more than one
  // kind, it first asks which to spend. A popover rather than a modal, so an
  // open usage menu stays visible behind it.
  import ConfirmDialog from "@/lib/dialogs/ConfirmDialog.svelte";
  import {
    cancelLimitReset,
    chooseLimitReset,
    confirmLimitReset,
    limitResetConfirmation,
    limitResetOffers
  } from "@/lib/stores/usage/limitReset.svelte";
  import { limitResetKind, spentResetPhrase } from "@/lib/usage/limit-reset/limit-reset-kind";
  import LimitResetChoice from "@/lib/usage/limit-reset/LimitResetChoice.svelte";

  const { onreset }: {
    /** Re-read the account once its limits were reset. */
    onreset: () => Promise<void>;
  } = $props();

  const confirmation = $derived(limitResetConfirmation());
  const offers = $derived(limitResetOffers());
  const hasChoice = $derived(offers.length > 1);
  const kind = $derived(limitResetKind(confirmation.chosen?.clears ?? []));
  const spentReset = $derived(
    spentResetPhrase({
      name: kind.name,
      resetsLeft: confirmation.chosen?.resetsLeft ?? 0
    })
  );
  const title = $derived.by(() => {
    if (hasChoice) {
      return "Which reset do you want to use?";
    }

    return `Use your ${kind.name}?`;
  });
</script>

{#if confirmation.isOpen && confirmation.chosen}
  <ConfirmDialog
    busy={confirmation.isClaiming}
    busyLabel="Resetting…"
    confirmLabel="Use {kind.name}"
    error={confirmation.failure}
    icon="refresh"
    nested
    oncancel={cancelLimitReset}
    onconfirm={() => confirmLimitReset(onreset)}
    {title}
  >
    {#if hasChoice}
      <LimitResetChoice
        chosenGrantId={confirmation.chosen.grantId}
        isLocked={confirmation.isClaiming}
        {offers}
        onchoose={chooseLimitReset}
      />
    {/if}
    <p>
      {kind.effect}, right now. This spends {spentReset}, and it can't be undone. Your
      weekly limit still resets on its usual day.
    </p>
  </ConfirmDialog>
{/if}
