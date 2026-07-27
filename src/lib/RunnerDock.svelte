<script lang="ts">
  import { parseAnsi } from "@/lib/ansi";
  import { Axis, beginReorder } from "@/lib/drag-reorder";
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import {
    pipeRunner,
    rerunRunner,
    runnerRows,
    setRunnerOrder,
    stopRunner
  } from "@/lib/stores/runners.svelte";
  import { RunnerStream } from "@/lib/types";

  // The active agent session — pipe target for a runner's output.
  const { activeSessionId }: {
    activeSessionId: string | null;
  } = $props();

  const rows = $derived(runnerRows());
  // A lone runner has nothing to sort — its header only becomes a drag handle
  // (grab cursor + tooltip) once a second card exists, like every other list.
  const reorderable = $derived(rows.length > 1);

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
        <article class="runner" data-runner-id={row.id}>
          <!-- The whole bar is the drag handle (canvas: header drag-to-reorder via
               the shared engine); its buttons are controls, not handles. -->
          <header
            class="bar"
            class:reorderable
            aria-label="{row.label} controls"
            onpointerdown={e => {
              if (!reorderable) {
                return;
              }

              beginReorder({
                e,
                itemSelector: "[data-runner-id]",
                idAttribute: "data-runner-id",
                axis: Axis.Horizontal,
                ignoreSelector: "button",
                onCommit: orderedIds => setRunnerOrder(orderedIds)
              });
            }}
            role="toolbar"
            tabindex={0}
          >
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
            {#if row.attached}
              <!-- The agent owns this process; PADE only reflects + can stop it. -->
              <span
                class="attached-tag"
                data-tooltip="Started by the agent · PADE can stop it, but its output stays in the agent's terminal"
              >agent</span>
            {/if}
            {#if activeSessionId && !row.attached}
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
            {#if !row.attached}
              <button
                class="rerun"
                aria-label="Re-run task"
                data-tooltip="Re-run · Shift-click keeps previous output"
                onclick={async e => await rerunRunner({
                  id: row.id,
                  preserve: e.shiftKey
                })}
              ><Icon name="refresh" /></button>
            {/if}
            <button
              class="stop"
              aria-label="Stop runner"
              data-tooltip="Stop"
              onclick={async () => await stopRunner(row.id)}
            ><Icon name="close" /></button>
          </header>
          {#if row.attached}
            <!-- No captured output: the task runs in the agent's own terminal,
                 which PADE can't tee. The card still tracks it and can stop it. -->
            <div class="attached-note">
              <p>Running in the agent's terminal.</p>
              <p class="muted">Its output stays there — PADE didn't spawn this process. Stop ends it.</p>
            </div>
          {:else}
            <div
              style:view-transition-name={`runner-output-${row.id}`}
              class="output"
              use:autoscroll
            >
              {#each row.lines as line, i (i)}
                <div
                  class="line"
                  class:error={line.stream === RunnerStream.enum.stderr}
                >{#each parseAnsi(line.text) as segment, segmentIndex (segmentIndex)}<span
                  style:color={segment.color}
                  style:background={segment.background}
                  class:bold={segment.bold}
                  class:dim={segment.dim}
                  class:italic={segment.italic}
                  class:underline={segment.underline}
                >{segment.text || " "}</span>{/each}</div>
              {/each}
            </div>
          {/if}
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
  }

  /* The bar doubles as the drag-to-reorder handle for its runner — but only
     once a second card exists to sort against. */
  .bar {
    display: flex;
    gap: 8px;
    align-items: center;
    padding-block: 7px;
    padding-inline: 10px;
    background: var(--surface-2);

    &.reorderable {
      cursor: grab;
      touch-action: none;
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
  .rerun,
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
    font-weight: 700;
    font-size: 12px;

    &:hover {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .rerun:hover {
    background: var(--surface-3);
    color: var(--on-surface);
  }

  .stop:hover {
    background: var(--critical-wash);
    color: var(--critical);
  }

  .output {
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

  /* Marks a card whose process the agent owns (no output; Stop still kills it). */
  .attached-tag {
    flex: none;
    padding-block: 2px;
    padding-inline: 7px;
    border-radius: 999px;
    background: var(--tertiary-wash);
    color: var(--tertiary);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  /* Stand-in for the output pane on an attached runner — there is nothing to
     stream, so it explains why rather than showing an empty terminal. */
  .attached-note {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 4px;
    justify-content: center;
    padding-block: 8px;
    padding-inline: 12px;
    background: var(--code-background);

    p {
      margin: 0;
      color: var(--code-foreground);
      font-size: 12px;
    }

    .muted {
      color: var(--on-surface-variant);
      font-size: 11px;
    }
  }

  .line {
    min-block-size: 1.5em;
    white-space: pre-wrap;
    animation: line-in 180ms var(--ease);

    /* stderr in a crit tint so failures read at a glance. It's the base colour the
       spans inherit — an SGR foreground colour overrides it per segment. */
    &.error {
      color: color-mix(in sRGB, var(--critical) 82%, var(--code-foreground));
    }

    /* Each segment span carries its SGR styles; colour/background come from the
       shared terminal palette via the `style:` directive. */
    .bold {
      font-weight: 700;
    }

    .dim {
      opacity: 70%;
    }

    .italic {
      font-style: italic;
    }

    .underline {
      text-decoration: underline;
    }
  }
</style>
