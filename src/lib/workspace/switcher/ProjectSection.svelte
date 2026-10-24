<script generics="Item" lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { Snippet } from "svelte";

  // One titled section of the rail's project list (Pinned or Recent): a sticky
  // head, then one row per item. Nothing renders for an empty section.
  const { heading, items, key, onclear, row }: {
    heading: string;
    items: readonly Item[];
    /** Each item's stable identity, keying its row. */
    key: (item: Item) => string;
    /** Clear the whole section - offered from its head when given. */
    onclear?: () => void;
    /** One item's row. */
    row: Snippet<[Item]>;
  } = $props();
</script>

{#if items.length > 0}
  <div class="list-head">
    <span>{heading}</span>
    {#if onclear}
      <button class="clear" onclick={onclear} type="button">
        <Icon name="trash" size={12} /> Clear
      </button>
    {/if}
  </div>
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
    gap: 8px;
    justify-content: space-between;
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

    .clear {
      display: inline-flex;
      gap: 4px;
      align-items: center;
      padding-block: 2px;
      padding-inline: 7px;
      border: none;
      border-radius: var(--radius-full);
      background: transparent;
      color: var(--on-surface-variant);
      font: inherit;
      font-weight: 600;
      font-size: 10px;
      letter-spacing: normal;
      text-transform: none;
      cursor: pointer;
      transition: color 120ms var(--ease), background 120ms var(--ease);

      &:hover {
        background: var(--critical-wash);
        color: var(--critical);
      }
    }
  }
</style>
