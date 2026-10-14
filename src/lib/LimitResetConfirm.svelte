<script lang="ts">
  // The one confirmation for spending Claude's free usage-limit reset, opened from
  // the collapsed meter's chip or the menu row. A popover rather than a modal, so
  // an open usage menu stays visible behind it.
  import ConfirmDialog from "@/lib/ConfirmDialog.svelte";
  import {
    cancelLimitReset,
    confirmLimitReset,
    limitResetConfirmation,
    limitResetOffer
  } from "@/lib/stores/limitReset.svelte";

  const { onreset }: {
    /** Re-read the account once its limits were reset. */
    onreset: () => Promise<void>;
  } = $props();

  const confirmation = $derived(limitResetConfirmation());
  const spentReset = $derived.by(() => {
    if (limitResetOffer()?.resetsLeft === 1) {
      return "your one free reset";
    }

    return "one of your free resets";
  });
</script>

{#if confirmation.isOpen}
  <ConfirmDialog
    busy={confirmation.isClaiming}
    busyLabel="Resetting…"
    confirmLabel="Reset now"
    error={confirmation.failure}
    icon="refresh"
    nested
    oncancel={cancelLimitReset}
    onconfirm={() => confirmLimitReset(onreset)}
    title="Reset your usage limits?"
  >
    <p>
      Clears Claude's usage limits right now. This spends {spentReset}, and it can't be
      undone. Your weekly limit still resets on its usual day.
    </p>
  </ConfirmDialog>
{/if}
