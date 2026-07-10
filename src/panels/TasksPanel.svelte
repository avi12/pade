<script lang="ts">
  import { feed, tasks as tasksApi } from "../lib/bridge";
  import type { TaskGroup } from "../lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";

  const { onrun }: {
    onrun: (task: {
      label: string;
      command: string;
      cwd: string;
    }) => void;
  } = $props();

  // Manifest basenames — a change to one of these re-scans the task list.
  const MANIFESTS = ["package.json", "Cargo.toml", "Makefile", "pyproject.toml"];

  let groups = $state<TaskGroup[]>([]);
  let error = $state<string | null>(null);
  let unlisten: UnlistenFn | undefined;

  async function refresh() {
    try {
      groups = await tasksApi.list();
      error = null;
    } catch (err) {
      error = String(err);
      groups = [];
    }
  }

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  // Debounced re-scan so a burst of manifest edits triggers one fetch.
  let timer: ReturnType<typeof setTimeout> | undefined;
  function scheduleRefresh() {
    clearTimeout(timer);
    timer = setTimeout(refresh, 300);
  }

  onMount(async () => {
    await refresh();
    unlisten = await feed.onChange(event => {
      if (MANIFESTS.includes(basename(event.path))) {
        scheduleRefresh();
      }
    });
  });
  onDestroy(() => {
    unlisten?.();
    clearTimeout(timer);
  });
</script>

<div class="tasks">
  <header class="head">
    <h2>Tasks</h2>
    <button class="refresh" aria-label="Refresh" data-tooltip="Refresh" onclick={refresh}>⟳</button>
  </header>

  {#if error}
    <p class="empty">Could not read project tasks.</p>
  {:else if groups.length === 0}
    <p class="empty">
      No runnable tasks found. Add a manifest — package.json, Cargo.toml, a
      Makefile, or pyproject.toml — and its tasks appear here.
    </p>
  {:else}
    <div class="scroll">
      {#each groups as group (group.manifest)}
        <section class="group">
          <h3>
            <span class="kind {group.kind}">{group.kind}</span>
            <span class="manifest" data-tooltip={group.dir}>{group.manifest}</span>
          </h3>
          {#each group.tasks as task (task.command)}
            <div class="row">
              <span class="tname">{task.name}</span>
              <code class="cmd">{task.command}</code>
              <button
                class="run"
                onclick={() => onrun({
                  label: task.name,
                  command: task.command,
                  cwd: group.dir
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

  .head {
    display: flex;
    gap: 8px;
    align-items: center;
    padding-block: 12px;
    padding-inline: 16px;
    border-block-end: 1px solid var(--outline);
  }

  .head h2 {
    margin: 0;
    font-size: 15px;
  }

  .refresh {
    block-size: 30px;
    inline-size: 30px;
    margin-inline-start: auto;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-var);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;

    &:hover {
      color: var(--primary);
    }
  }

  .empty {
    margin: 16px;
    color: var(--on-surface-var);
    font-size: 13px;
    line-height: 1.5;
  }

  .scroll {
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow-y: auto;
    padding-block: 8px;
    padding-inline: 10px;
  }

  .group h3 {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-block: 4px 8px;
    margin-inline: 4px;
  }

  .kind {
    padding-block: 2px;
    padding-inline: 8px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-var);
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;

    &.npm {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    &.cargo {
      background: color-mix(in sRGB, var(--tertiary) 30%, var(--surface-3));
    }
  }

  .manifest {
    overflow: hidden;
    color: var(--on-surface-var);
    font-family: var(--font-mono);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    padding-block: 6px;
    padding-inline: 8px;
    border-radius: var(--r-sm);

    &:hover {
      background: var(--surface-2);
    }
  }

  .tname {
    flex: none;
    font-weight: 600;
    font-size: 13px;
  }

  .cmd {
    flex: 1;
    overflow: hidden;
    color: var(--on-surface-var);
    font-family: var(--font-mono);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .run {
    flex: none;
    padding-block: 4px;
    padding-inline: 14px;
    border: none;
    border-radius: 999px;
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;

    &:hover {
      background: color-mix(in sRGB, var(--primary) 88%, var(--on-primary));
    }
  }
</style>
