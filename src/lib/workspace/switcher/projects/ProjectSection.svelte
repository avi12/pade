<script generics="Item" lang="ts">
  import { flipDuration, toggleRow } from "@/lib/ui/motion";
  import { createReorderGlide } from "@/lib/ui/reorder-glide";
  import type { Snippet } from "svelte";
  import { untrack } from "svelte";

  // One titled section of the rail's project list (Pinned or Recent): a sticky
  // head, then one row per item. Nothing renders for an empty section.
  const { heading, items, key, itemAttributes, row, isReorderAnimated = false }: {
    heading: string;
    items: readonly Item[];
    /** Each item's stable identity, keying its row. */
    key: (item: Item) => string;
    /** Attributes for an item's wrapper, the list's direct child (a drag
     *  engine's item id). */
    itemAttributes?: (item: Item) => Record<string, string | undefined>;
    /** One item's row. */
    row: Snippet<[Item]>;
    /** Rows glide to a new place when the order changes on its own (a project
     *  whose last session stopped sinking below the running ones) - off where a
     *  drag already moves them. */
    isReorderAnimated?: boolean;
  } = $props();

  const glide = createReorderGlide({ duration: flipDuration });

  // Before the DOM takes a new order, note where the rows are painted. Only a
  // real change of order counts: the list is re-derived on every status tick,
  // and a row merely nudged by its neighbour's growing height must keep
  // following that growth, not glide against it.
  let paintedOrder = "";
  $effect.pre(() => {
    const order = items.map(key).join("\n");
    const isReordered = isReorderAnimated && paintedOrder !== "" && order !== paintedOrder;
    paintedOrder = order;

    if (!isReordered) {
      return;
    }

    untrack(glide.beforeReorder);
  });
</script>

{#if items.length > 0}
  <div class="list-head">{heading}</div>
  {#each items as item (key(item))}
    <!-- An item that leaves (a narrowed search, Remove from list) closes up
         while the ones below glide into its place; one that joins opens in,
         and one that comes back mid-flight reverses from where it is. On this
         wrapper, the each's own element, so the transition is local: it plays
         for an item coming or going, never for the whole rail. -->
    <div
      class="item"
      {...itemAttributes?.(item)}
      {@attach glide.row}
      transition:toggleRow
    >
      {@render row(item)}
    </div>
  {/each}
{/if}

<style>
  /* Sticky section label (Pinned / Recent) over the scrolling list. Neither
     it nor an item ever shrinks: the rail's list is a flex column, and once
     its projects outgrow the rail it would squeeze them - the label to its
     text, an item closing up (its transition clips it) shut in a frame -
     jerking every row under them up. */
  .list-head {
    position: sticky;
    inset-block-start: 0;
    z-index: 2;
    display: flex;
    flex: none;
    align-items: center;
    block-size: var(--section-head-block-size, auto);
    padding-block: 7px 3px;
    padding-inline: 8px;
    background: var(--surface-1);
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 0.5625rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;

    /* Cyberpunk: a section label is a titled section, so it takes the same
       look as a menu's `.menu-separator` - the console's 44px accent stub
       riding a rule under the title, set in the skin's mono face. It keeps its
       own spacing against the list that scrolls under it. */
    :global(:root[data-palette="cyberpunk"]) & {
      border-block-end: 2px solid var(--outline);
      background-image: var(--header-stub);
      background-position: bottom left;
      background-size: 100% 4px;
      background-repeat: no-repeat;
      font-family: var(--font-monospace);
    }
  }

  .item {
    flex: none;
  }
</style>
