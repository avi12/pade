<script lang="ts">
  import { MERGE_INTO_TOOLTIP } from "@/lib/shell/handoff/window-handoff";
  import type { WindowInfo } from "@/lib/types";
  import { tooltip } from "@/lib/ui/tooltip/truncation-tooltip";

  // An open-window row's secondary action: merge this window into that one.
  // It rides the row's end, shown while the row is hovered or anything in it
  // has focus, and stays in the arrow-key walk so the keyboard reaches it even
  // while it is faded out. This window's own row has none.
  const { windowRow, onmerge }: {
    windowRow: WindowInfo;
    onmerge: (windowLabel: string) => void;
  } = $props();

  const MERGE_INTO = "Merge into";
</script>

{#if !windowRow.isCurrent}
  <button
    class="merge-into"
    {@attach tooltip(MERGE_INTO_TOOLTIP)}
    data-arrow-stop
    onclick={() => onmerge(windowRow.label)}
    role="menuitem"
    type="button"
  >{MERGE_INTO}</button>
{/if}

<style>
  .merge-into {
    flex: none;
    padding-block: 4px;
    padding-inline: 8px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    font: inherit;
    font-size: 0.6875rem;
    white-space: nowrap;
    opacity: 0%;
    cursor: pointer;
    transition: opacity 120ms var(--ease), background 120ms var(--ease), color 120ms var(--ease);

    @media (prefers-reduced-motion: reduce) {
      transition: none;
    }

    :global(:where(:hover, :focus-within)) > & {
      opacity: 100%;
    }

    &:hover,
    &:focus-visible {
      background: var(--primary-container);
      color: var(--on-primary-container);
      outline: none;
    }
  }
</style>
