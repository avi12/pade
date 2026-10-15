<script lang="ts">
  import PacingNote from "@/lib/shell/PacingNote.svelte";
  import StatusPill from "@/lib/shell/StatusPill.svelte";
  import type { createUsagePacing } from "@/lib/stores/usage/usagePacing.svelte";

  // The status pill an automatic recovery (handoff, usage resume, API-error
  // retry) shows while it works; with none running, one pill per agent the
  // usage pacing has paused, each with its continue actions.
  const { note, pacing }: {
    note: string;
    pacing: Pick<ReturnType<typeof createUsagePacing>, "pauses" | "continueNow" | "continueWhenReady">;
  } = $props();
</script>

<div class="recovery-notes">
  {#if note}
    <StatusPill>{note}</StatusPill>
  {:else}
    {#each pacing.pauses as pause (pause.id)}
      <PacingNote {pacing} {pause} />
    {/each}
  {/if}
</div>

<style>
  /* Pinned under the top bar, centered; pacing notes stack one per agent. */
  .recovery-notes {
    position: fixed;
    inset-block-start: 60px;
    inset-inline-start: 50%;
    z-index: 80;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: center;
    translate: -50% 0;
  }
</style>
