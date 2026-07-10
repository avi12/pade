<script lang="ts">
  import { feed, tasks as tasksApi } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import { baseName } from "@/lib/paths";
  import type { TaskGroup } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";

  const { onrun }: {
    onrun: (task: {
      label: string;
      command: string;
      cwd: string;
      kind: TaskGroup["kind"];
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

  // Debounced re-scan so a burst of manifest edits triggers one fetch.
  let timer: ReturnType<typeof setTimeout> | undefined;
  function scheduleRefresh() {
    clearTimeout(timer);
    timer = setTimeout(refresh, 300);
  }

  onMount(async () => {
    await refresh();
    unlisten = await feed.onChange(event => {
      if (MANIFESTS.includes(baseName(event.path))) {
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
    <button class="refresh" aria-label="Refresh" data-tooltip="Refresh" onclick={refresh}>
      <Icon name="refresh" />
    </button>
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
          {#each group.tasks as task (task.name)}
            <div class="row">
              <div class="meta">
                <span class="tname">{task.name}</span>
                <code class="cmd">{task.command}</code>
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
    display: grid;
    place-items: center;
    block-size: 30px;
    inline-size: 30px;
    margin-inline-start: auto;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-var);
    cursor: pointer;
    transition: color 140ms var(--ease);

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
    gap: 10px;
    overflow-y: auto;
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
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-var);
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
    padding-block: 8px;
    padding-inline: 10px;
    border-radius: var(--r-sm);
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

  .tname {
    font-weight: 600;
    font-size: 13px;
  }

  .cmd {
    overflow: hidden;
    color: var(--on-surface-var);
    font-family: var(--font-mono);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .run {
    flex: none;
    padding-block: 5px;
    padding-inline: 15px;
    border: none;
    border-radius: 999px;
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
