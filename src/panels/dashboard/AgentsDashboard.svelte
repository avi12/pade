<script lang="ts">
  import { AGENT_STATE_TEXT, BOARD_SORTS, boardTally, orderedWindows } from "@/lib/dashboard/agent-board";
  import { rememberedSort, rememberSort } from "@/lib/dashboard/board-sort";
  import { createBoardReader } from "@/lib/dashboard/boardReader.svelte";
  import { setWindowTitle } from "@/lib/ipc/platform";
  import { effective } from "@/lib/settings/prefs.svelte";
  import Segmented from "@/lib/ui/controls/Segmented.svelte";
  import { flipDuration } from "@/lib/ui/motion";
  import OverflowTooltipLayer from "@/lib/ui/tooltip/OverflowTooltipLayer.svelte";
  import ProjectCard from "@/panels/dashboard/ProjectCard.svelte";
  import { onMount } from "svelte";
  import { flip } from "svelte/animate";

  // The agents dashboard window: every open project window's agents at a
  // glance - the one that needs you first - each with what it is doing and
  // what it has done. A click on a project or agent brings that window up.
  const WINDOW_TITLE = "PADE · Agents";
  /** How often the relative ages ("working · 3m") move on. */
  const CLOCK_TICK_MS = 15_000;

  const reader = createBoardReader();
  let sort = $state(rememberedSort());
  const windows = $derived(
    orderedWindows({
      windows: reader.windows.values(),
      order: reader.order,
      sort
    })
  );
  const tally = $derived(boardTally(reader.windows.values()));
  let now = $state(Date.now());

  onMount(async () => {
    await setWindowTitle(WINDOW_TITLE);
  });
  onMount(() => {
    const clock = setInterval(() => (now = Date.now()), CLOCK_TICK_MS);
    return () => clearInterval(clock);
  });
</script>

<div style:--font-ui={effective.uiFamily} style:--font-monospace={effective.monospaceFamily} class="dashboard">
  <header>
    <h1>Agents</h1>
    <ul class="tally">
      {#each tally as entry (entry.state)}
        <li data-state={entry.state}><span class="count">{entry.count}</span> {AGENT_STATE_TEXT[entry.state]}</li>
      {/each}
    </ul>
    <div class="sort">
      <Segmented
        chosen={sort}
        label="Sort projects"
        onchoose={chosen => {
          sort = chosen;
          rememberSort(chosen);
        }}
        options={BOARD_SORTS}
      />
    </div>
  </header>

  {#if windows.length === 0}
    <p class="empty">No project windows are open - agents show up here as soon as one is</p>
  {:else}
    <div class="grid">
      {#each windows as projectWindow (projectWindow.label)}
        <div animate:flip={{ duration: flipDuration() }}>
          <ProjectCard
            board={projectWindow}
            {now}
            onfocus={sessionId => reader.focus({
              label: projectWindow.label,
              sessionId
            })}
          />
        </div>
      {/each}
    </div>
  {/if}

  <OverflowTooltipLayer />
</div>

<style>
  .dashboard {
    display: flex;
    flex-direction: column;
    gap: 18px;
    overflow-y: auto;
    block-size: 100%;
    padding: 20px clamp(16px, 3vw, 32px);
    color: var(--on-surface);
  }

  /* Title and tally on the left, the sort control pushed right. */
  header {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    align-items: center;
  }

  .sort {
    margin-inline-start: auto;
  }

  h1 {
    margin: 0;
    font-size: 1.375rem;
  }

  .tally {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;

    li {
      padding: 4px 10px;
      border: 1px solid var(--outline);
      border-radius: var(--radius-full);
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 0.75rem;
    }

    [data-state="waiting"] {
      border-color: var(--critical);
      color: var(--critical);
    }

    [data-state="working"] {
      color: var(--tertiary);
    }

    .count {
      font-weight: 700;
    }
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 360px), 1fr));
    gap: 16px;
    align-items: start;
  }

  .empty {
    margin: 0;
    color: var(--on-surface-variant);
  }
</style>
