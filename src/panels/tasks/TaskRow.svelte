<script lang="ts">
  import type { RunnerTask } from "@/lib/stores/tasks/runners.svelte";
  import { isTaskRunning, taskKey } from "@/lib/stores/tasks/taskRuns.svelte";
  import type { TaskGroup } from "@/lib/types";
  import RunningMarker from "@/panels/tasks/RunningMarker.svelte";

  // One runnable task of a manifest's group: its name, marked while it runs,
  // its command, and the button that runs it in the dock.
  const { task, group, onrun }: {
    task: TaskGroup["tasks"][number];
    group: TaskGroup;
    onrun: (task: Omit<RunnerTask, "project">) => void;
  } = $props();

  const isRunning = $derived(
    isTaskRunning(
      taskKey({
        directory: group.dir,
        command: task.command
      })
    )
  );
</script>

<div class="meta">
  <!-- The running marker trails the name, so starting a task never shifts its
       name off the column the other names share. -->
  <span class="task-name">
    {task.name}
    {#if isRunning}
      <RunningMarker />
    {/if}
  </span>
  <code class="command">{task.command}</code>
</div>
<button
  class="run"
  data-tooltip="Run in the dock - ◆ pipes its output to the agent"
  onclick={() => onrun({
    label: task.name,
    command: task.command,
    cwd: group.dir,
    kind: group.kind
  })}
>Run</button>

<style>
  .meta {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-inline-size: 0;
  }

  .task-name {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    font-weight: 600;
    font-size: 0.8125rem;
  }

  .command {
    overflow: hidden;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 0.6875rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .run {
    flex: none;
    padding-block: 5px;
    padding-inline: 15px;
    border: none;
    border-radius: var(--radius-full);
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 700;
    font-size: 0.75rem;
    cursor: pointer;
    transition: background 140ms var(--ease);

    /* Dim by mixing the fill, not with opacity: opacity below 100% makes the
       button a stacking context, which traps its tooltip under the pinned
       manifest header. */
    &:hover {
      background: color-mix(in sRGB, var(--primary) 90%, var(--surface));
    }

    /* The long pipe-explainer would overflow the panel's right edge with the
       global centered-below tooltip - align it to this button's trailing edge
       instead. */
    &::after {
      inset-inline: auto anchor(right);
      justify-self: auto;
    }
  }
</style>
