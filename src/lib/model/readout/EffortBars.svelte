<script lang="ts">
  // A running model's effort as signal bars (`effortBars`): one bar per level
  // of its ladder, climbing in height, lit up to the level in use. Decorative -
  // the chip it sits in names the level in words. The chip sizes it through
  // `--effort-bars-height` and `--effort-bar-width`.

  import type { runningModelLook } from "@/lib/model/readout/running-model";

  const { bars }: { bars: ReturnType<typeof runningModelLook>["bars"] } = $props();
</script>

{#if bars.length > 0}
  <span class="bars" aria-hidden="true">
    {#each bars as bar (bar.level)}
      <span style:--bar-height="{bar.heightPercent}%" class="bar" class:lit={bar.isLit}></span>
    {/each}
  </span>
{/if}

<style>
  .bars {
    display: inline-flex;
    flex-shrink: 0;
    gap: 1px;
    align-items: flex-end;
    block-size: var(--effort-bars-height, 10px);
  }

  .bar {
    block-size: var(--bar-height);
    inline-size: var(--effort-bar-width, 2px);
    border-radius: 1px;
    background: var(--outline);

    &.lit {
      background: var(--primary);
    }

    /* Cyberpunk's bars are square. */
    :global(:root[data-palette="cyberpunk"]) & {
      border-radius: 0;
    }
  }
</style>
