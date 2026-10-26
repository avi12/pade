<script lang="ts">
  import type { RunnerTask } from "@/lib/stores/tasks/runners.svelte";
  import type { TaskGroup } from "@/lib/types";
  import { collapseRow, emphasized, expandRow, flipDuration } from "@/lib/ui/motion";
  import { baseName } from "@/lib/util/paths";
  import TaskRow from "@/panels/tasks/TaskRow.svelte";
  import { flip } from "svelte/animate";

  // One manifest's group: its pinned header, then a row per task - a script
  // added or removed glides in or out, survivors FLIPping to their new slots.
  const { group, onrun }: {
    group: TaskGroup;
    onrun: (task: Omit<RunnerTask, "project">) => void;
  } = $props();
</script>

<h3>
  <span class="kind {group.kind}">{group.kind}</span>
  <span class="manifest" data-tooltip={group.dir}>{baseName(group.dir)}</span>
</h3>
{#each group.tasks as task (task.name)}
  <div
    class="row"
    in:expandRow
    out:collapseRow
    animate:flip={{
      duration: flipDuration(),
      easing: emphasized
    }}
  >
    <TaskRow {group} {onrun} {task} />
  </div>
{/each}

<style>
  /* Each manifest's header sticks to the top of the list while its tasks scroll
     under it, until the next group's header pushes it out - sticky is bounded by
     its own group. It bleeds over the list's padding (inline and the top edge)
     with the side pane's own surface, so no row peeks out beside or above it.
     The space above its label is the same pinned as at rest: the header's box
     reaches up over the list padding and the group's spacing (negative margin)
     and takes that space back as padding, so the label never moves when it
     sticks - only the surface behind it starts covering the rows. The list and
     the group set the spacings it reads (TasksPanel). */
  h3 {
    --header-padding-block: 4px;
    --space-above-header: calc(var(--scroll-padding-block) + var(--group-space-above));

    position: sticky;
    inset-block-start: calc(-1 * var(--scroll-padding-block));
    z-index: 1;
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0;
    margin-block-start: calc(-1 * var(--space-above-header));
    margin-inline: calc(-1 * var(--scroll-padding-inline));
    padding-block: calc(var(--space-above-header) + var(--header-padding-block)) var(--header-padding-block);
    padding-inline: var(--scroll-padding-inline);
    background: var(--surface);
  }

  .kind {
    padding-block: 2px;
    padding-inline: 9px;
    border-radius: var(--radius-full);
    background: var(--surface-3);
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 0.625rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;

    /* npm → primary container; cargo → tertiary wash; make/python keep the
       neutral surface-3 default above. */
    &.npm {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    &.cargo {
      background: var(--tertiary-wash);
      color: var(--tertiary);
    }
  }

  .manifest {
    overflow: hidden;
    min-inline-size: 0;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 0.75rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 10px;
    border-radius: var(--radius-small);
    transition: background 140ms var(--ease);

    &:hover {
      background: var(--surface-2);
    }
  }
</style>
