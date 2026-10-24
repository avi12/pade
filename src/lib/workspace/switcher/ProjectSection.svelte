<script generics="Item" lang="ts">
  import type { Snippet } from "svelte";

  // One titled section of the rail's project list (Pinned or Recent): a sticky
  // head, then one row per item. Nothing renders for an empty section.
  const { heading, items, key, row }: {
    heading: string;
    items: readonly Item[];
    /** Each item's stable identity, keying its row. */
    key: (item: Item) => string;
    /** One item's row. */
    row: Snippet<[Item]>;
  } = $props();
</script>

{#if items.length > 0}
  <div class="list-head">{heading}</div>
  {#each items as item (key(item))}
    {@render row(item)}
  {/each}
{/if}

<style>
  /* Sticky section label (Pinned / Recent) over the scrolling list. */
  .list-head {
    position: sticky;
    inset-block-start: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    block-size: var(--section-head-block-size, auto);
    padding-block: 7px 3px;
    padding-inline: 8px;
    background: var(--surface-1);
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
</style>
