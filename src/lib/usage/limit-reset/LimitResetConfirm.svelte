<script lang="ts">
  // The one confirmation for spending one of Claude's free usage-limit resets,
  // opened from a menu row for that kind, or from the collapsed meter's chip,
  // which first asks which kind to spend when there are several. A popover
  // rather than a modal, so an open usage menu stays visible behind it.
  import ConfirmDialog from "@/lib/dialogs/ConfirmDialog.svelte";
  import {
    cancelLimitReset,
    chooseLimitReset,
    confirmLimitReset,
    limitResetConfirmation,
    limitResetOffers
  } from "@/lib/stores/usage/limitReset.svelte";
  import { limitResetKind, NOTHING_TO_RESET, spentResetPhrase } from "@/lib/usage/limit-reset/limit-reset-kind";
  import LimitResetChoice from "@/lib/usage/limit-reset/LimitResetChoice.svelte";

  const { onreset }: {
    /** Re-read the account once its limits were reset. */
    onreset: () => Promise<void>;
  } = $props();

  const confirmation = $derived(limitResetConfirmation());
  const offers = $derived(limitResetOffers());
  const hasChoice = $derived(confirmation.isChoosing);
</script>

{#if confirmation.isOpen && confirmation.chosen}
  {@const kind = limitResetKind(confirmation.chosen.kind)}
  {@const title = hasChoice ? "Which reset do you want to use?" : `Use your ${kind.name}?`}
  {@const spentReset = spentResetPhrase({
    name: kind.name,
    resetsLeft: confirmation.chosen.resetsLeft
  })}
  <ConfirmDialog
    busy={confirmation.isClaiming}
    busyLabel="Resetting…"
    confirmLabel="Use {kind.name}"
    error={confirmation.failure}
    icon="refresh"
    isConfirmable={confirmation.chosen.hasUsageToClear}
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
    {#if confirmation.chosen.hasUsageToClear}
      <p>
        {kind.effect}, right now. This spends {spentReset}, and it can't be undone. Your
        weekly limit still resets on its usual day.
      </p>
    {:else}
      <p>{NOTHING_TO_RESET}</p>
    {/if}
  </ConfirmDialog>
{/if}
