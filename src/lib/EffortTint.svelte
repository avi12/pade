<script lang="ts">
  // Text tinted by how hot an effort runs: cool green at its ladder's lowest,
  // amber in the middle, red at the top. A tab tints its model name with it; the
  // effort picker fills a chip per level so its row runs cool to hot.
  // No `heat` (a level no ladder knows) leaves the surrounding text colour.

  import type { Snippet } from "svelte";

  const { heat, isFilled = false, children }: {
    /** 0 (coolest) to 1 (hottest), or null when the level's rank is unknown. */
    heat: number | null;
    /** Sits on a chip of its own colour instead of bare text. */
    isFilled?: boolean;
    children: Snippet;
  } = $props();
</script>

<span style:--heat={heat} class="effort-tint" class:filled={isFilled} class:unranked={heat === null}>
  {@render children()}
</span>

<style>
  .effort-tint {
    /* Two legs, low to mid then mid to high, so the middle lands on the
       theme's own amber instead of whatever a straight green-to-red mix passes. */
    --heat-color:
      color-mix(
        in oklch,
        color-mix(in oklch, var(--effort-low), var(--effort-mid) clamp(0%, var(--heat) * 200%, 100%)),
        var(--effort-high) clamp(0%, (var(--heat) * 2 - 1) * 100%, 100%)
      );

    display: inline-flex;
    gap: 3px;
    align-items: center;
    color: var(--heat-color);
    font-weight: 700;
    white-space: nowrap;
    transition: color 140ms var(--ease);

    &.filled {
      padding-inline: 5px;
      border-radius: var(--radius-small);
      background: color-mix(in oklch, var(--heat-color) 18%, transparent);
      line-height: 1.5;
    }

    &.unranked {
      --heat-color: currentColor;

      font-weight: inherit;
    }
  }
</style>
