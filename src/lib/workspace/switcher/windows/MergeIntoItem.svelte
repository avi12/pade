<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { MERGE_INTO_TOOLTIP, mergeMenuItemLabel, mergeTargets } from "@/lib/shell/window-handoff";
  import type { WindowInfo } from "@/lib/types";

  // The one menu item below the open-window list that merges this window into
  // the only other one. With several other windows each of their rows carries
  // its own merge action, so the item stays away.
  const { windowRows, labels, onmerge }: {
    windowRows: WindowInfo[];
    labels: Record<string, string>;
    onmerge: (windowLabel: string) => void;
  } = $props();

  const itemLabel = $derived(
    mergeMenuItemLabel({
      windowRows,
      labels
    })
  );
  const target = $derived(mergeTargets(windowRows)[0]);
</script>

{#if itemLabel !== null && target !== undefined}
  <button
    class="menu-item merge-item"
    data-arrow-stop
    data-tooltip={MERGE_INTO_TOOLTIP}
    onclick={() => onmerge(target.label)}
    role="menuitem"
    type="button"
  >
    <Icon name="external" size={15} />
    <span>{itemLabel}</span>
  </button>
{/if}

<style>
  .merge-item {
    display: flex;
    gap: 9px;
    align-items: center;
  }
</style>
