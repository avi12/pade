<script lang="ts">
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import {
    moveRunnerBefore,
    moveRunnerBy,
    pipeRunner,
    runnerRows,
    stopRunner
  } from "@/lib/stores/runners.svelte";
  import { RunnerStream } from "@/lib/types";

  // The active agent session — pipe target for a runner's output.
  const { activeSessionId }: {
    activeSessionId: string | null;
  } = $props();

  const rows = $derived(runnerRows());

  // NOTE: the feed-toggle chip and the per-runner maximize/float control from the
  // canvas are backend/planned — deferred until the runner backend supports them.

  // Dock height, drag-resizable from the top grip. Clamped so the dock can never
  // swallow the whole window nor collapse below a usable minimum.
  const MIN_DOCK = 140;
  let dockHeight = $state(clampDock(360));

  function maxDock(): number {
    return innerHeight * 0.75;
  }

  function clampDock(height: number): number {
    return Math.min(Math.max(height, MIN_DOCK), maxDock());
  }

  // The dock height is clamped against the viewport; a window resize can push the
  // stored height past the new max, so re-clamp on every resize.
  $effect(() => {
    function reclamp(): void {
      dockHeight = clampDock(dockHeight);
    }
    addEventListener("resize", reclamp);
    return () => removeEventListener("resize", reclamp);
  });

  // ── Drag-to-reorder ─────────────────────────────────────────────────────────
  // The grip at the start of each runner's bar reorders it among its siblings.
  // A pointer drag hit-tests the runner under the cursor (each carries a
  // data-runner-id) and moves the dragged one before it; arrow keys nudge it one
  // slot, so reordering works without a mouse too.
  let draggingId = $state<string | null>(null);

  // Human-readable status for a runner's dot, used for both the tooltip and the
  // accessible name.
  function statusLabel({ done, failed }: {
    done: boolean;
    failed: boolean;
  }): string {
    if (failed) {
      return "Failed";
    }

    if (done) {
      return "Done";
    }

    return "Running";
  }

  // Keep an output pane pinned to its newest line as it streams.
  function autoscroll(node: HTMLElement) {
    const observer = new MutationObserver(() => {
      node.scrollTop = node.scrollHeight;
    });
    observer.observe(node, {
      childList: true,
      subtree: true
    });
    return {
      destroy() {
        observer.disconnect();
      }
    };
  }
</script>

{#if rows.length > 0}
  <section style:block-size="{dockHeight}px" class="dock" aria-label="Task runners">
    <!-- Drag-to-resize grip along the dock's top edge. -->
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

        const grip = e.currentTarget;
        const startY = e.clientY;
        const startHeight = dockHeight;
        grip.setPointerCapture(e.pointerId);

        function onMove(move: PointerEvent): void {
          dockHeight = clampDock(startHeight + (startY - move.clientY));
        }
        function cleanup(): void {
          grip.removeEventListener("pointermove", onMove);
          grip.removeEventListener("pointerup", cleanup);
          grip.removeEventListener("pointercancel", cleanup);
        }
        grip.addEventListener("pointermove", onMove);
        grip.addEventListener("pointerup", cleanup);
        grip.addEventListener("pointercancel", cleanup);
      }}
      role="separator"
    ><span class="grabber"></span></div>

    <header class="head">
      <h2>Task runners</h2>
      <span class="count">{formatCount(rows.length)}</span>
      <span class="spacer"></span>
      <span class="hint">Running side by side</span>
    </header>

    <div class="grid">
      {#each rows as row (row.id)}
        <article class="runner" class:dragging={draggingId === row.id} data-runner-id={row.id}>
          <div class="bar">
            <button
              class="grab"
              aria-label="Reorder task runner — drag, or use arrow keys"
              data-tooltip="Drag to reorder"
              onkeydown={e => {
                const earlier = e.key === "ArrowLeft" || e.key === "ArrowUp";
                const later = e.key === "ArrowRight" || e.key === "ArrowDown";
                if (earlier || later) {
                  e.preventDefault();
                  moveRunnerBy({
                    id: row.id,
                    delta: earlier ? -1 : 1
                  });
                }
              }}
              onpointerdown={e => {
                if (!(e.currentTarget instanceof HTMLElement)) {
                  return;
                }

                e.preventDefault();
                const grip = e.currentTarget;
                // Capture the id up front: the each-binding `row` can't be read
                // from inside the hoisted nested closures below.
                const id = row.id;
                draggingId = id;
                grip.setPointerCapture(e.pointerId);

                function onMove(move: PointerEvent): void {
                  const under = document.elementFromPoint(move.clientX, move.clientY);
                  const overRunner = under instanceof Element ? under.closest("[data-runner-id]") : null;
                  const beforeId = overRunner?.getAttribute("data-runner-id");
                  if (beforeId) {
                    moveRunnerBefore({
                      id,
                      beforeId
                    });
                  }
                }
                function cleanup(): void {
                  draggingId = null;
                  grip.removeEventListener("pointermove", onMove);
                  grip.removeEventListener("pointerup", cleanup);
                  grip.removeEventListener("pointercancel", cleanup);
                }
                grip.addEventListener("pointermove", onMove);
                grip.addEventListener("pointerup", cleanup);
                grip.addEventListener("pointercancel", cleanup);
              }}
            ><Icon name="grip" /></button>
            <span class="kind {row.kind}">{row.kind}</span>
            <span
              class="dot"
              class:done={row.done && !row.failed}
              class:failed={row.failed}
              aria-label={statusLabel({
                done: row.done,
                failed: row.failed
              })}
              data-tooltip={statusLabel({
                done: row.done,
                failed: row.failed
              })}
            ></span>
            <span class="name">{row.label}</span>
            {#if activeSessionId}
              <button
                class="pipe"
                aria-label="Send output to the active agent"
                data-tooltip="Send output to the active agent"
                onclick={async () => await pipeRunner({
                  id: row.id,
                  sessionId: activeSessionId
                })}
              >◆</button>
            {/if}
            <button
              class="stop"
              aria-label="Stop runner"
              data-tooltip="Stop"
              onclick={async () => await stopRunner(row.id)}
            ><Icon name="close" /></button>
          </div>
          <div
            style:view-transition-name={`runner-output-${row.id}`}
            class="out"
            use:autoscroll
          >
            {#each row.lines as line, i (i)}
              <div
                class="line"
                class:err={line.stream === RunnerStream.enum.stderr}
              >{line.text || " "}</div>
            {/each}
          </div>
        </article>
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
      border-radius: 999px;
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
  }

  .head h2 {
    margin: 0;
    font-weight: 700;
    font-size: 13px;
  }

  .count {
    padding-block: 2px;
    padding-inline: 8px;
    border-radius: 999px;
    background: var(--primary-container);
    color: var(--on-primary-container);
    font-weight: 700;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .spacer {
    flex: 1;
  }

  .hint {
    color: var(--on-surface-variant);
    font-size: 11px;
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

  .runner {
    display: flex;
    flex-direction: column;

    /* Each runner stays readable (canvas floor); the grid scrolls when several
       stack. The output pane below scrolls independently via its own min-size. */
    min-block-size: 168px;
    background: var(--surface-1);

    &.dragging {
      opacity: 60%;
    }
  }

  .bar {
    display: flex;
    gap: 8px;
    align-items: center;
    padding-block: 7px;
    padding-inline: 10px;
    background: var(--surface-2);
  }

  /* Drag handle at the start of the bar — grab to reorder the runner. */
  .grab {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 22px;
    inline-size: 16px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--on-surface-variant);
    cursor: grab;
    touch-action: none;
    transition: color 140ms var(--ease), background 140ms var(--ease);

    &:hover {
      color: var(--on-surface);
    }

    &:active {
      cursor: grabbing;
    }
  }

  .kind {
    flex: none;
    padding-block: 2px;
    padding-inline: 8px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.05em;
    text-transform: uppercase;

    &.npm {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    &.cargo {
      background: var(--tertiary-wash);
      color: var(--tertiary);
    }
  }

  .dot {
    flex: none;
    block-size: 8px;
    inline-size: 8px;
    border-radius: 999px;
    background: var(--primary);
    animation: pulse 1100ms var(--ease) infinite;

    &.done {
      background: var(--tertiary);
      box-shadow: 0 0 0 4px var(--tertiary-wash);
      animation: none;
    }

    /* Non-zero exit: crit dot, no success halo. */
    &.failed {
      background: var(--critical);
      animation: none;
    }
  }

  .name {
    flex: 1;
    overflow: hidden;
    min-inline-size: 0;
    font-weight: 600;
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pipe,
  .stop {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 24px;
    inline-size: 24px;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: background 140ms var(--ease), color 140ms var(--ease);
  }

  .pipe {
    color: var(--primary);

    &:hover {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .stop:hover {
    background: var(--critical-wash);
    color: var(--critical);
  }

  .out {
    flex: 1;
    overflow: auto;
    min-block-size: 0;
    padding-block: 8px;
    padding-inline: 10px;
    background: var(--code-background);
    color: var(--code-foreground);
    font-family: var(--font-monospace);
    font-size: 12px;
    line-height: 1.5;
  }

  .line {
    min-block-size: 1.5em;
    white-space: pre-wrap;
    animation: line-in 180ms var(--ease);

    /* stderr in a crit tint so failures read at a glance. */
    &.err {
      color: color-mix(in sRGB, var(--critical) 82%, var(--code-foreground));
    }
  }
</style>
