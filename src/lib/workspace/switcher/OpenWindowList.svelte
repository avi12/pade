<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { WindowInfo } from "@/lib/types";
  import { isTemporaryWorkspace, shortDisplayName } from "@/lib/util/paths";
  import ProjectKindIcon from "@/lib/workspace/display/ProjectKindIcon.svelte";
  import TempBadge from "@/lib/workspace/display/TempBadge.svelte";
  import MergeIntoButton from "@/lib/workspace/switcher/MergeIntoButton.svelte";
  import MergeIntoItem from "@/lib/workspace/switcher/MergeIntoItem.svelte";
  import ReorderGrip from "@/lib/workspace/switcher/ReorderGrip.svelte";
  import { windowRowTransitionName } from "@/lib/workspace/switcher/switcher-rows";

  // The switcher's "Open windows" section - every PADE window in creation
  // order, which is also the cycle order for Ctrl+Alt+[ / ]. Click a non-current
  // one to focus its window; drag a grip to reorder them. With another window
  // open, this one can merge into it: its sessions move there still running,
  // and this window closes - from that window's row, or from the one item
  // below the list when there is exactly one other window.
  let { windowRows, labels, elList = $bindable(null), onreorder, onfocuswindow, onmerge }: {
    windowRows: WindowInfo[];
    labels: Record<string, string>;
    /** The rows' own container, so the switcher's scoped view transition morphs
     *  just these rows when the order changes - locally or when another window's
     *  reorder broadcasts. */
    elList?: HTMLElement | null;
    /** A drag landed: the window labels in their new order. */
    onreorder: (windowLabels: string[]) => void;
    /** Focus another window by its label. */
    onfocuswindow: (windowLabel: string) => void;
    /** Merge this window into the window `windowLabel`. */
    onmerge: (windowLabel: string) => void;
  } = $props();

  // A lone window can't be reordered - mirror the pins guard so the grip only
  // appears when there's more than one open window to drag among.
  const isReorderable = $derived(windowRows.length > 1);
</script>

<div class="menu-separator">Open windows</div>
<div bind:this={elList} class="open-window-list">
  {#each windowRows as windowRow (windowRow.label)}
    <!-- Grip + focus button; data-window-id makes the wrapper the reorder
         engine's drag sibling, view-transition-name lets it glide to its new
         slot instead of the list cross-fading. -->
    <div
      style:view-transition-name={windowRowTransitionName(windowRow.label)}
      class="open-window-row-item"
      data-window-id={windowRow.label}
    >
      <ReorderGrip
        idAttribute="data-window-id"
        isEnabled={isReorderable}
        itemSelector="[data-window-id]"
        oncommit={onreorder}
      />
      <button
        class="open-window-row"
        class:current={windowRow.isCurrent}
        data-arrow-stop
        onclick={() => {
          if (!windowRow.isCurrent) {
            onfocuswindow(windowRow.label);
          }
        }}
        role="menuitem"
        type="button"
      >
        <ProjectKindIcon path={windowRow.path} />
        <span class="open-window-row-name">{shortDisplayName({
          path: windowRow.path,
          labels
        })}</span>
        {#if isTemporaryWorkspace(windowRow.path)}
          <TempBadge />
        {/if}
        <span class="open-window-row-spacer"></span>
        {#if windowRow.isCurrent}
          <span class="this-window">this window</span>
        {:else}
          <span class="open-window-row-focus" aria-hidden="true"><Icon name="external" size={14} /></span>
        {/if}
      </button>
      <MergeIntoButton {onmerge} {windowRow} />
    </div>
  {/each}
</div>
<MergeIntoItem {labels} {onmerge} {windowRows} />

<style>
  /* A window row: a drag grip (when reorderable) beside its focus button, laid out
     like a pinned project row so the grip sits flush to the row's left edge. */
  .open-window-row-item {
    position: relative;
    display: flex;
    gap: 2px;
    align-items: center;
  }

  /* An open-window row: focus another window, or "this window" for the current one. */
  .open-window-row {
    display: flex;
    flex: 1;
    gap: 9px;
    align-items: center;
    min-inline-size: 0;
    padding-block: 7px;
    padding-inline: 10px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: color 120ms var(--ease), background 120ms var(--ease);

    &.current {
      cursor: default;
    }

    &:not(.current):hover,
    &:not(.current):focus-visible {
      background: var(--primary-container);
      color: var(--on-primary-container);
      outline: none;
    }

    .open-window-row-name {
      overflow: hidden;
      min-inline-size: 0;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 0.8125rem;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .open-window-row-spacer {
      flex: 1;
      min-inline-size: 8px;
    }

    .this-window {
      flex: none;
      color: var(--primary);
      font-weight: 700;
      font-size: 0.5625rem;
      letter-spacing: 0.06em;
      text-transform: uppercase;
    }

    .open-window-row-focus {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
    }

    &:hover .open-window-row-focus,
    &:focus-visible .open-window-row-focus {
      color: inherit;
    }
  }
</style>
