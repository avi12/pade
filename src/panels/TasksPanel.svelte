<script lang="ts">
  import { collapseRow, emphasized, expandRow, flipDuration } from "@/lib/motion";
  import { baseName } from "@/lib/paths";
  import { setPanelHeader } from "@/lib/stores/sidePanel.svelte";
  import { taskCatalog } from "@/lib/stores/taskCatalog.svelte";
  import { isTaskRunning, taskKey } from "@/lib/stores/taskRuns.svelte";
  import type { TaskGroup } from "@/lib/types";
  import { flip } from "svelte/animate";

  const { project, onrun }: {
    /** The workspace displayed by this window, never the process-global cwd. */
    project: string;
    onrun: (task: {
      label: string;
      command: string;
      cwd: string;
      kind: TaskGroup["kind"];
    }) => void;
  } = $props();

  const snapshot = $derived(taskCatalog.snapshot);
  const groups = $derived(snapshot.groups);
  const error = $derived(snapshot.error);
  const manifests = $derived(snapshot.descriptors);

  async function refresh(workspace = project): Promise<void> {
    await taskCatalog.refresh(workspace);
  }

  // Unlike the feed, this lazy panel stays mounted across an in-window project
  // switch. Re-scan the newly supplied workspace immediately.
  $effect(() => {
    refresh(project);
  });

  // Publish the refresh action to the shared side-panel header.
  $effect(() => {
    setPanelHeader({
      count: null,
      refresh
    });
  });
</script>

<div class="tasks">
  {#if error}
    <p class="empty">Could not read project tasks.</p>
  {:else if groups.length === 0}
    <div class="empty">
      <p>No runnable tasks here. They appear as soon as the project carries one of these:</p>
      {#if manifests.length > 0}
        <ul class="manifest-list">
          {#each manifests as manifest (manifest.value)}
            <li>{manifest.value}</li>
          {/each}
        </ul>
      {/if}
    </div>
  {:else}
    <div class="scroll">
      <!-- A manifest appearing/vanishing glides its whole group in/out; a script
           added or removed inside one does the same per row, with survivors
           FLIPping to their new slots. -->
      {#each groups as group (group.manifest)}
        <section
          class="group"
          in:expandRow
          out:collapseRow
          animate:flip={{
            duration: flipDuration(),
            easing: emphasized
          }}
        >
          <h3>
            <span class="kind {group.kind}">{group.kind}</span>
            <span class="manifest" data-tooltip={group.dir}>{baseName(group.dir)}</span>
          </h3>
          {#each group.tasks as task (task.name)}
            {@const runningNow = isTaskRunning(
              taskKey({
                directory: group.dir,
                command: task.command
              })
            )}
            <div
              class="row"
              class:running={runningNow}
              in:expandRow
              out:collapseRow
              animate:flip={{
                duration: flipDuration(),
                easing: emphasized
              }}
            >
              <div class="meta">
                <span class="task-name">
                  {#if runningNow}
                    <span class="run-dot" aria-hidden="true"></span>
                  {/if}
                  {task.name}
                  {#if runningNow}
                    <output class="run-tag">running</output>
                  {/if}
                </span>
                <code class="command">{task.command}</code>
              </div>
              <button
                class="run"
                data-tooltip="Run in the dock — ◆ pipes its output to the agent"
                onclick={() => onrun({
                  label: task.name,
                  command: task.command,
                  cwd: group.dir,
                  kind: group.kind
                })}
              >Run</button>
            </div>
          {/each}
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .tasks {
    display: flex;
    flex-direction: column;
    block-size: 100%;
  }

  .empty {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 16px;
    color: var(--on-surface-variant);
    font-size: 13px;
    line-height: 1.5;

    p {
      margin: 0;
    }
  }

  /* The manifests the backend reads, straight from its registry — a wrapped set
     of tokens rather than a comma-run, because the list is long enough that a
     sentence stops being readable at panel width. */
  .manifest-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;

    li {
      padding-block: 2px;
      padding-inline: 8px;
      border-radius: var(--radius-small);
      background: var(--surface-3);
      color: var(--on-surface);
      font-family: var(--font-monospace);
      font-size: 12px;
    }
  }

  .scroll {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
    min-block-size: 0;
    padding-block: 8px;
    padding-inline: 10px;
    animation: panel-swap 280ms var(--ease);
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-block-start: 4px;
  }

  .group h3 {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0;
  }

  .kind {
    padding-block: 2px;
    padding-inline: 9px;
    border-radius: var(--radius-full);
    background: var(--surface-3);
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 10px;
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
    font-size: 12px;
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
    font-size: 13px;

    /* Green, dim-flashing while the agent is running this task. */
    .run-dot {
      flex: none;
      block-size: 7px;
      inline-size: 7px;
      border-radius: var(--radius-full);
      background: var(--tertiary);
      animation: pulse 1100ms var(--ease) infinite;
    }

    .run-tag {
      color: var(--tertiary);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.06em;
      text-transform: uppercase;
    }
  }

  .command {
    overflow: hidden;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 11px;
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
    font-size: 12px;
    cursor: pointer;
    transition: opacity 140ms var(--ease);

    &:hover {
      opacity: 90%;
    }

    /* The long pipe-explainer would overflow the panel's right edge with the
       global centered-below tooltip — anchor it to this button's trailing edge
       and float it above instead. */
    &::after {
      inset-block: auto calc(100% + 6px);
      inset-inline: auto 0;
      translate: 0 0;
    }
  }
</style>
