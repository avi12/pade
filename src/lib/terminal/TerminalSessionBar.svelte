<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import SessionBadge from "@/lib/session/SessionBadge.svelte";
  import type { AgentSession, SessionStatus } from "@/lib/types";
  import { Axis, beginReorder } from "@/lib/ui/drag-reorder";
  import type { DragHint } from "@/lib/ui/drag-reorder";

  const {
    session, status, isRemovable,
    onremove = () => {},
    onpopout = () => {},
    onreorder = () => {},
    ondraghint = () => {}
  }: {
    session: AgentSession;
    status: SessionStatus;
    /** Show a trailing remove-from-split button, and make the bar a drag handle. */
    isRemovable: boolean;
    onremove?: () => void;
    onpopout?: () => void;
    onreorder?: (orderedIds: string[]) => void;
    ondraghint?: (hint: DragHint | null) => void;
  } = $props();

  const label = $derived.by(() => {
    if (session.branch) {
      return `${session.agent.label} · ${session.branch}`;
    }

    return session.agent.label;
  });
</script>

<!-- Pointer-only reorder handle for the split; the remove button stays
     keyboard-reachable, so the drag is a pure enhancement. Drag the bar past a
     4px threshold to reorder the visible split panes, or up onto the tab strip
     to pop the pane back out to a plain tab (the mirror of dragging a tab down
     to split it). The `[data-pane-id]` slot the engine reorders lives in App,
     two levels up; `closest` reaches it across the component boundary. The
     remove button carries `data-noreorder` so pressing it stays a click. Only a
     pane in a live split reorders - `isRemovable` is true exactly then. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<header
  class="session-bar"
  class:reorderable={isRemovable}
  onpointerdown={e => {
    if (!isRemovable) {
      return;
    }

    beginReorder({
      e,
      itemSelector: "[data-pane-id]",
      idAttribute: "data-pane-id",
      axis: Axis.Horizontal,
      threshold: 4,
      ignoreSelector: "[data-noreorder]",
      onCommit: ids => onreorder(ids),
      onHint: hint => ondraghint(hint),
      outsideSelector: "[data-tab-strip]",
      onDropOutside: () => onpopout()
    });
  }}
>
  <SessionBadge {label} {status} />
  {#if isRemovable}
    <button
      class="remove-pane"
      aria-label="Remove from split"
      data-noreorder
      data-tooltip="Remove from split"
      onclick={() => onremove()}
    >
      <Icon name="close" size={16} />
    </button>
  {/if}
</header>

<style>
  /* Thin session bar on surface-1 with a hairline divider; the SessionBadge
     (dot + mono label + state phrase) sits flush at the start. */
  .session-bar {
    display: flex;
    flex-shrink: 0;
    gap: 10px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 14px;
    border-block-end: 1px solid var(--outline);
    background: var(--surface-1);

    /* In a split, the bar is a drag handle for reordering the panes; a touch-drag
       must grab it, not scroll. A lone pane has nothing to sort, so no affordance. */
    &.reorderable {
      cursor: grab;
      touch-action: none;

      &:active {
        cursor: grabbing;
      }
    }
  }

  /* Inline remove-from-split action at the end of the bar - transparent until
     hovered, then a soft crit wash (canvas line 276). */
  .remove-pane {
    display: inline-flex;
    flex-shrink: 0;
    justify-content: center;
    align-items: center;
    block-size: 24px;
    inline-size: 24px;
    margin-inline-start: auto;
    border: none;
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: color 150ms var(--ease), background 150ms var(--ease);

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
    }
  }
</style>
