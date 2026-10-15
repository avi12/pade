<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { Snippet } from "svelte";

  // One titled section of the switcher's project list (Pinned or Recent): a
  // sticky head, then one row per project. Nothing renders for an empty section.
  const { title, projects, onclear, row }: {
    title: string;
    projects: string[];
    /** Clear the whole section - offered from its head when given. */
    onclear?: () => void;
    /** One project's row. */
    row: Snippet<[string]>;
  } = $props();
</script>

{#if projects.length > 0}
  <div class="list-head">
    <span>{title}</span>
    {#if onclear}
      <button class="clear" onclick={onclear} type="button">
        <Icon name="trash" size={12} /> Clear
      </button>
    {/if}
  </div>
  {#each projects as project (project)}
    {@render row(project)}
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
    padding-block: 7px 3px;
    padding-inline: 10px;
    background: var(--surface-2);
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
