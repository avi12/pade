<script lang="ts">
  import type { RunnerTask } from "@/lib/stores/tasks/runners.svelte";
  import { taskCatalog } from "@/lib/stores/tasks/taskCatalog.svelte";
  import { setPanelHeader } from "@/lib/stores/ui/sidePanel.svelte";
  import { collapseRow, emphasized, expandRow, flipDuration } from "@/lib/ui/motion";
  import NoTasks from "@/panels/tasks/NoTasks.svelte";
  import TaskGroupRows from "@/panels/tasks/TaskGroupRows.svelte";
  import { flip } from "svelte/animate";

  const { project, onrun }: {
    /** The workspace displayed by this window, never the process-global cwd. */
    project: string;
    /** Run a task in the dock; the caller files it under its project. */
    onrun: (task: Omit<RunnerTask, "project">) => void;
  } = $props();

  const snapshot = $derived(taskCatalog.snapshot);
  const groups = $derived(snapshot.groups);
  const error = $derived(snapshot.error);
  const manifests = $derived(snapshot.descriptors);

  // The catalog follows the front project on its own (the window's services
  // re-scan it on a switch, a manifest edit re-scans it live), so the panel only
  // reads it - and offers a manual re-scan.
  async function refresh() {
    await taskCatalog.refresh(project);
  }

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
    <p class="empty">Could not read project tasks</p>
  {:else if groups.length === 0}
    <NoTasks {manifests} />
  {:else}
    <div class="scroll">
      <!-- A manifest appearing/vanishing glides its whole group in/out. -->
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
          <TaskGroupRows {group} {onrun} />
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
    font-size: 0.8125rem;
    line-height: 1.5;
  }

  .scroll {
    --scroll-padding-block: 8px;
    --scroll-padding-inline: 10px;

    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
    min-block-size: 0;
    padding-block: var(--scroll-padding-block);
    padding-inline: var(--scroll-padding-inline);
    animation: panel-swap 280ms var(--ease);
  }

  .group {
    --group-space-above: 4px;

    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-block-start: var(--group-space-above);
  }
</style>
