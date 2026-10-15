<script lang="ts">
  import StatusPill from "@/lib/shell/screen/StatusPill.svelte";
  import type { createUsagePacing } from "@/lib/stores/usage/usagePacing.svelte";

  type UsagePacing = ReturnType<typeof createUsagePacing>;

  // One agent the usage pacing has paused: its status line and the ways to pick
  // it back up.
  const { pause, pacing }: {
    pause: UsagePacing["pauses"][number];
    pacing: Pick<UsagePacing, "continueNow" | "continueWhenReady">;
  } = $props();
</script>

<StatusPill>
  {pause.note}
  {#if pause.canContinueWhenReady}
    <button
      class="note-action quiet" onclick={() => pacing.continueWhenReady(pause.id)}
      type="button">Continue when there's room</button>
  {/if}
  <button
    class="note-action" onclick={async () => await pacing.continueNow(pause.id)}
    type="button">
    {#if pause.isReady}
      Continue
    {:else}
      Continue anyway
    {/if}
  </button>
</StatusPill>

<style>
  .note-action {
    flex: none;
    padding-block: 4px;
    padding-inline: 12px;
    border-radius: var(--radius-full);
    background: var(--primary);
    color: var(--on-primary);
    font-weight: 600;

    &:hover {
      background: color-mix(in sRGB, var(--primary) 88%, var(--on-primary));
    }

    &.quiet {
      background: transparent;
      color: var(--on-primary-container);
      outline: 1px solid var(--primary);

      &:hover {
        background: color-mix(in sRGB, var(--primary) 14%, transparent);
      }
    }
  }
</style>
