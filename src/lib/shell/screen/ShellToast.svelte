<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { toastText } from "@/lib/stores/ui/toast.svelte";
</script>

{#if toastText()}
  <!-- <output> already carries role=status - a transient bottom pill that
       auto-dismisses via the showToast timer. -->
  <output class="toast">
    <span class="toast-icon"><Icon name="external" /></span>
    {toastText()}
  </output>
{/if}

<style>
  /* Transient status toast - sits just above the send FAB, auto-dismissed. */
  .toast {
    position: fixed;
    inset-block-end: 72px;
    inset-inline-start: 50%;
    z-index: 85;
    display: inline-flex;
    gap: 9px;
    align-items: center;
    padding-block: 10px;
    padding-inline: 18px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-full);
    background: var(--surface-3);
    color: var(--on-surface);
    font-weight: 600;
    font-size: 13px;
    box-shadow: 0 12px 34px var(--shadow-color);
    transform: translateX(-50%);
    animation: pop-in 240ms var(--spring);

    .toast-icon {
      display: inline-flex;
      color: var(--primary);
    }
  }
</style>
