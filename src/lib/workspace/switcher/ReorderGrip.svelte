<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { Axis, beginReorder } from "@/lib/ui/drag-reorder";
  import { tooltip } from "@/lib/ui/truncation-tooltip";

  // The drag handle a reorderable switcher row leads with - a span, so grabbing
  // it never triggers the row button's own click. The row wrapper carrying
  // `idAttribute` is the reorder engine's drag sibling.
  const { isEnabled, itemSelector, idAttribute, oncommit }: {
    /** The list can be reordered right now - otherwise no grip is drawn. */
    isEnabled: boolean;
    /** Selects every draggable row among the siblings. */
    itemSelector: string;
    /** The attribute holding each row's id - what `oncommit` receives in order. */
    idAttribute: string;
    /** The drop landed: the row ids in their new order. */
    oncommit: (orderedIds: string[]) => void;
  } = $props();

  // Pixels a row's grip must travel before a press becomes a drag.
  const ROW_DRAG_THRESHOLD = 4;
</script>

{#if isEnabled}
  <span
    class="grip" {@attach tooltip("Drag to reorder")}
    aria-hidden="true"
    onpointerdown={e => beginReorder({
      e,
      itemSelector,
      idAttribute,
      axis: Axis.Vertical,
      threshold: ROW_DRAG_THRESHOLD,
      onCommit: oncommit
    })}
  ><Icon name="grip" size={14} /></span>
{/if}

<style>
  /* Muted, brightens on hover; touch-action:none so a touch-drag grabs the
     handle rather than scrolling the list. */
  .grip {
    display: inline-flex;
    flex: none;
    align-items: center;
    align-self: stretch;
    padding-inline: 1px;
    color: var(--on-surface-variant);
    opacity: 55%;
    cursor: grab;
    touch-action: none;
    transition: color 120ms var(--ease), opacity 120ms var(--ease);

    &:hover {
      color: var(--on-surface);
      opacity: 100%;
    }
  }
</style>
