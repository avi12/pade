<script lang="ts">
  // Text coloured by how hard a model thinks, in one of the theme's five
  // effort tones: blue, green, amber, red, purple, coolest to hottest. A tab
  // tints its model name with it, and the effort picker each level's name, so
  // its row runs cool to hot. No `tone` (a level no ladder knows) leaves the
  // surrounding text colour.

  import type { Snippet } from "svelte";

  const { tone, children }: {
    /** 1 (coolest) to 5 (hottest), from `effortTone`, or null
     *  when the level's rank is unknown. */
    tone: number | null;
    children: Snippet;
  } = $props();

  const toneColor = $derived.by(() => {
    if (tone === null) {
      return undefined;
    }

    return `var(--effort-tone-${tone})`;
  });
</script>

<span style:--tone-color={toneColor} class="effort-tint" class:unranked={tone === null}>
  {@render children()}
</span>

<style>
  .effort-tint {
    display: inline-flex;
    gap: 3px;
    align-items: center;
    color: var(--tone-color);
    font-weight: 700;
    white-space: nowrap;
    transition: color 140ms var(--ease);

    &.unranked {
      --tone-color: currentColor;

      font-weight: inherit;
    }
  }
</style>
