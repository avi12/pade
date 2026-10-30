<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { createUsagePacing } from "@/lib/stores/usage/usagePacing.svelte";

  type UsagePacing = ReturnType<typeof createUsagePacing>;

  // One session the usage pacing has paused: a strip across the top of that
  // session's own pane, so its status and the ways to pick it back up sit on
  // the session they act on - never in a stack of look-alike banners.
  const { pause, pacing }: {
    pause: UsagePacing["pauses"][number];
    pacing: Pick<UsagePacing, "continueNow" | "continueWhenReady">;
  } = $props();
</script>

<output class="pause-strip">
  <span class="pause-icon"><Icon name="pause" /></span>
  <span class="note">{pause.note}</span>
  <span class="actions">
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
  </span>
</output>

<style>
  .pause-strip {
    display: flex;
    flex: none;
    flex-wrap: wrap;
    gap: 6px 10px;
    align-items: center;
    padding-block: 6px;
    padding-inline: 12px;
    border-block-end: 1px solid var(--primary);
    background: var(--primary-container);
    color: var(--on-primary-container);
    font-weight: 600;
    font-size: 0.8125rem;
    animation: pop-in 220ms var(--ease);
  }

  .pause-icon {
    display: inline-flex;
    flex: none;
    color: var(--primary);
  }

  .note {
    flex: 1 1 240px;
    min-inline-size: 0;
  }

  .actions {
    display: flex;
    flex: none;
    gap: 8px;
    margin-inline-start: auto;
  }

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
