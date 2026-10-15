<script lang="ts">
  import { isEveryTypeSelected } from "@/lib/feed/type-filter";
  import type { ExtensionCount } from "@/lib/feed/type-filter";
  import { collapseRow, emphasized, expandRow, flipDuration } from "@/lib/ui/motion";
  import { formatCount } from "@/lib/util/format";
  import { flip } from "svelte/animate";
  import type { SvelteSet } from "svelte/reactivity";

  // The file-type filter's native popover: one checkbox per extension in the
  // feed, ticking into the feed's selection set (it narrows the cards).
  const { id, anchor, extensionCounts, selectedExtensions }: {
    /** The popover's id, which its trigger's `popovertarget` names. */
    id: string;
    /** The trigger's `anchor-name`, so the popover opens below it. */
    anchor: string;
    extensionCounts: ExtensionCount[];
    selectedExtensions: SvelteSet<string>;
  } = $props();

  const allTypesSelected = $derived(
    isEveryTypeSelected({
      counts: extensionCounts,
      selected: selectedExtensions
    })
  );
</script>

<div {id} style:position-anchor={anchor} class="type-popover popover-menu" popover>
  <div class="popover-header">
    <span class="pop-title">File type</span>
    <!-- Fills every box when not all are ticked, else clears the selection
         back to the "show all" default. -->
    <button
      class="selectall"
      onclick={() => {
        if (allTypesSelected) {
          selectedExtensions.clear();
          return;
        }

        for (const item of extensionCounts) {
          selectedExtensions.add(item.extension);
        }
      }}
    >
      {#if allTypesSelected}
        Clear{:else}Select all{/if}
    </button>
  </div>
  <ul class="typelist">
    {#each extensionCounts as item (item.extension)}
      <li
        in:expandRow
        out:collapseRow
        animate:flip={{
          duration: flipDuration(),
          easing: emphasized
        }}
      >
        <label class="check typerow">
          <span class="checkbox">
            <input
              checked={selectedExtensions.has(item.extension)}
              onchange={() => {
                if (selectedExtensions.has(item.extension)) {
                  selectedExtensions.delete(item.extension);
                  return;
                }

                selectedExtensions.add(item.extension);
              }}
              type="checkbox"
            />
            <span class="box" aria-hidden="true">
              <svg fill="none" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7" /></svg>
            </span>
          </span>
          <span class="extension">{item.extension}</span>
          <span class="count">{formatCount(item.count)}</span>
        </label>
      </li>
    {/each}
  </ul>
</div>

<style>
  /* Anchored below the chip. Uses the shared .popover-menu shell for its flip
     fallbacks, but overrides the surface to a tonal surface-container card with
     the M3 large radius, softer elevation, and airier padding to match design. */
  .type-popover {
    overflow-y: auto;
    max-block-size: 60vh;
    min-inline-size: 224px;
    padding: 6px;
    border: none;
    border-radius: var(--radius-large);
    background: var(--surface-2);
    box-shadow: 0 8px 28px var(--shadow-color);
    position-area: bottom span-right;

    .popover-header {
      display: flex;
      gap: 12px;
      justify-content: space-between;
      align-items: center;
      padding-block: 8px 6px;
      padding-inline: 12px;
    }

    .pop-title {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    /* "Select all" is a text action, not a checkbox: a plain blue link-button. */
    .selectall {
      padding: 0;
      border: none;
      background: none;
      color: var(--primary);
      font-family: inherit;
      font-weight: 600;
      font-size: 12px;
      cursor: pointer;

      &:hover {
        text-decoration: underline;
      }
    }

    .typelist {
      display: flex;
      flex-direction: column;
      gap: 2px;
      margin: 0;
      padding-block: 2px 0;
      padding-inline: 0;
      list-style: none;
    }

    .check {
      display: flex;
      gap: 10px;
      align-items: center;
      font-size: 13px;
      cursor: pointer;
    }

    /* Shrink the shared 20px .checkbox to a menu-fit 18px. */
    .checkbox {
      block-size: 18px;
      inline-size: 18px;

      .box {
        block-size: 18px;
        inline-size: 18px;
      }
    }

    .typerow {
      padding-block: 7px;
      padding-inline: 12px;
      border-radius: var(--radius-small);

      &:hover {
        background: var(--surface-3);
      }

      .extension {
        flex: 1;
        overflow: hidden;
        font-family: var(--font-monospace);
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .count {
        color: var(--on-surface-variant);
      }
    }
  }
</style>
