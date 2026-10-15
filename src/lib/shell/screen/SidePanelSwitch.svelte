<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { SIDE_PANEL_TABS } from "@/lib/shell/side-panels";
  import type { Side } from "@/lib/shell/side-panels";

  // The top bar's segmented control: one tab per side panel; picking the open
  // one again closes it (`null` means no panel is open).
  let { side = $bindable() }: {
    side: Side | null;
  } = $props();
</script>

<div class="segmented-control" aria-label="Side panels" role="tablist">
  {#each SIDE_PANEL_TABS as tab (tab.id)}
    <button
      aria-selected={side === tab.id}
      data-tooltip={tab.label}
      onclick={() => (side = side === tab.id ? null : tab.id)}
      role="tab"
    ><Icon name={tab.icon} /> <span>{tab.short}</span></button>
  {/each}
</div>

<style>
  .segmented-control {
    display: inline-flex;
    flex-shrink: 0;
    gap: 2px;
    padding: 3px;
    border-radius: var(--radius-full);
    background: var(--surface-2);

    button {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      padding: 6px 12px;
      border: none;
      border-radius: var(--radius-full);
      background: transparent;
      color: var(--on-surface-variant);
      font: inherit;
      font-weight: 600;
      font-size: 12px;
      cursor: pointer;
      transition: background 200ms var(--ease), color 200ms var(--ease);
    }

    /* Selected tab - matched by state, not by qualifying the button type. */
    [aria-selected="true"] {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }
</style>
