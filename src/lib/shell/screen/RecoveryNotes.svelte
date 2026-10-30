<script lang="ts">
  import StatusPill from "@/lib/shell/screen/StatusPill.svelte";

  // The status pill an automatic recovery (handoff, usage resume, API-error
  // retry) shows while it works. A usage pause is shown on its own session
  // instead: across its pane, and as a mark on its rail row.
  const { note }: {
    note: string;
  } = $props();
</script>

{#if note}
  <div class="recovery-notes">
    <StatusPill>{note}</StatusPill>
  </div>
{/if}

<style>
  /* Pinned under the top bar, centered. */
  .recovery-notes {
    position: fixed;
    inset-block-start: 60px;
    inset-inline-start: 50%;
    z-index: 80;
    translate: -50% 0;
  }
</style>
