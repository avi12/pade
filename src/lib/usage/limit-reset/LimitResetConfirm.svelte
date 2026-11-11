<script lang="ts">
  // The one confirmation for spending one of Claude's free usage-limit resets,
  // opened from that kind's button in the "Usage resets" card. A popover rather
  // than a modal, so an open usage menu stays visible behind it.
  import ConfirmDialog from "@/lib/dialogs/ConfirmDialog.svelte";
  import { cancelLimitReset, confirmLimitReset, limitResetConfirmation } from "@/lib/stores/usage/limitReset.svelte";
  import { limitResetKind, NOTHING_TO_RESET, spentResetPhrase } from "@/lib/usage/limit-reset/limit-reset-kind";

  const { onreset }: {
    /** Re-read the account once its limits were reset. */
    onreset: () => Promise<void>;
  } = $props();

  const confirmation = $derived(limitResetConfirmation());
</script>

{#if confirmation.isOpen && confirmation.chosen}
  {@const kind = limitResetKind(confirmation.chosen.kind)}
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
    title="Use your {kind.name}?"
  >
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
