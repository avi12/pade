<script lang="ts">
  // The one confirmation for spending Claude's free usage-limit reset, opened from
  // the collapsed meter's chip or the menu row. A popover rather than a modal, so
  // an open usage menu stays visible behind it.
  import ConfirmDialog from "@/lib/dialogs/ConfirmDialog.svelte";
  import {
    cancelLimitReset,
    confirmLimitReset,
    limitResetConfirmation,
    limitResetOffer
  } from "@/lib/stores/usage/limitReset.svelte";
  import { limitResetKind, spentResetPhrase } from "@/lib/usage/limit-reset/limit-reset-kind";

  const { onreset }: {
    /** Re-read the account once its limits were reset. */
    onreset: () => Promise<void>;
  } = $props();

  const confirmation = $derived(limitResetConfirmation());
  const offer = $derived(limitResetOffer());
  const kind = $derived(limitResetKind(offer?.clears ?? []));
  const spentReset = $derived(
    spentResetPhrase({
      name: kind.name,
      resetsLeft: offer?.resetsLeft ?? 0
    })
  );
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
    title="Use your {kind.name}?"
  >
    <p>
      {kind.effect}, right now. This spends {spentReset}, and it can't be undone. Your
      weekly limit still resets on its usual day.
    </p>
  </ConfirmDialog>
{/if}
