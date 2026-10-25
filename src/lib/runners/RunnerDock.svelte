<script lang="ts">
  import RunnerCard from "@/lib/runners/RunnerCard.svelte";
  import { projectRunnerRows } from "@/lib/stores/tasks/runners.svelte";
  import { formatCount } from "@/lib/util/format";

  const { activeSessionId, project }: {
    /** The active agent session - pipe target for a runner's output. */
    activeSessionId: string | null;
    /** The project in front: the dock shows its runners alone, and hides when
     *  it has none. */
    project: string;
  } = $props();

  // Dock height, drag-resizable from the top grip. Clamped so the dock can never
  // swallow the whole window nor collapse below a usable minimum.
  const MINIMUM_DOCK_HEIGHT = 140;
  const INITIAL_DOCK_HEIGHT = 360;
  const MAXIMUM_DOCK_VIEWPORT_SHARE = 0.75;
  // Header + one attached card's bar + its note, no output pane.
  const COMPACT_DOCK_HEIGHT = 132;

  const rows = $derived(projectRunnerRows(project));
  // A lone runner has nothing to sort - its header only becomes a drag handle
  // (grab cursor + tooltip) once a second card exists, like every other list.
  const isReorderable = $derived(rows.length > 1);

  // NOTE: the feed-toggle chip and the per-runner maximize/float control from the
  // canvas are backend/planned - deferred until the runner backend supports them.

  let dockHeight = $state(clampDock(INITIAL_DOCK_HEIGHT));

  function clampDock(height: number) {
    return Math.min(Math.max(height, MINIMUM_DOCK_HEIGHT), innerHeight * MAXIMUM_DOCK_VIEWPORT_SHARE);
  }

  // The dock height is clamped against the viewport; a window resize can push the
  // stored height past the new max, so re-clamp on every resize.
  $effect(() => {
    function reclamp() {
      dockHeight = clampDock(dockHeight);
    }
    addEventListener("resize", reclamp);
    return () => removeEventListener("resize", reclamp);
  });

  // When every card is an ATTACHED runner (a task the AGENT runs, tracked but not
  // owned), the dock carries no streaming output - just a one-line note - so it
  // stays compact instead of taking the full streaming height and needlessly
  // squeezing the terminal above it (which, while a busy agent hasn't repainted at
  // the new size, shows squished). Any spawned runner restores the resizable height.
  const isAttachedOnly = $derived(rows.length > 0 && rows.every(row => row.attached));
  const shownHeight = $derived.by(() => {
    if (isAttachedOnly) {
      return COMPACT_DOCK_HEIGHT;
    }

    return dockHeight;
  });
</script>

{#if rows.length > 0}
  <section style:block-size="{shownHeight}px" class="dock" aria-label="Task runners">
    {#if !isAttachedOnly}
      <!-- Drag-to-resize grip along the dock's top edge. A compact attached-only
         dock has nothing to grow into, so it has no grip. -->
      <div
        class="grip"
        aria-label="Resize task runner dock"
        aria-orientation="horizontal"
        data-tooltip="Drag to resize"
        onpointerdown={e => {
          e.preventDefault();

          if (!(e.currentTarget instanceof HTMLElement)) {
            return;
          }

          const elGrip = e.currentTarget;
          const startY = e.clientY;
          const startHeight = dockHeight;
          elGrip.setPointerCapture(e.pointerId);

          function onMove(move: PointerEvent) {
            dockHeight = clampDock(startHeight + (startY - move.clientY));
          }
          function cleanup() {
            elGrip.removeEventListener("pointermove", onMove);
            elGrip.removeEventListener("pointerup", cleanup);
            elGrip.removeEventListener("pointercancel", cleanup);
          }
          elGrip.addEventListener("pointermove", onMove);
          elGrip.addEventListener("pointerup", cleanup);
          elGrip.addEventListener("pointercancel", cleanup);
        }}
        role="separator"
      ><span class="grabber"></span></div>
    {/if}

    <header class="head">
      <h2>Task runners</h2>
      <span class="count">{formatCount(rows.length)}</span>
      <span class="spacer"></span>
      <span class="hint">Running side by side</span>
    </header>

    <div class="grid">
      {#each rows as row (row.id)}
        <RunnerCard {activeSessionId} {isReorderable} {row} />
      {/each}
    </div>
  </section>
{/if}

<style>
  .dock {
    display: flex;
    flex: none;
    flex-direction: column;
    border-block-start: 1px solid var(--outline);
    background: var(--surface-1);
    animation: rise 280ms var(--ease);
  }

  .grip {
    display: flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 7px;
    margin-block-end: -4px;
    cursor: row-resize;
    touch-action: none;

    &:hover {
      background: var(--primary-container);
    }

    .grabber {
      block-size: 3px;
      inline-size: 36px;
      border-radius: var(--radius-full);
      background: var(--outline);
    }
  }

  .head {
    display: flex;
    gap: 9px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 14px;
    border-block-end: 1px solid var(--outline);

    h2 {
      margin: 0;
      font-weight: 700;
      font-size: 0.8125rem;
    }
  }

  .count {
    padding-block: 2px;
    padding-inline: 8px;
    border-radius: var(--radius-full);
    background: var(--primary-container);
    color: var(--on-primary-container);
    font-weight: 700;
    font-size: 0.6875rem;
  }

  .spacer {
    flex: 1;
  }

  .hint {
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
  }

  .grid {
    display: grid;
    flex: 1;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 1px;
    overflow: auto;
    min-block-size: 0;
    background: var(--outline);
  }
</style>
