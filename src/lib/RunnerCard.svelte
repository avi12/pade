<script lang="ts">
  import { Axis, beginReorder } from "@/lib/drag-reorder";
  import Icon from "@/lib/Icon.svelte";
  import RunnerOutput from "@/lib/RunnerOutput.svelte";
  import { pipeRunner, rerunRunner, setRunnerOrder, stopRunner } from "@/lib/stores/runners.svelte";
  import type { RunnerRow } from "@/lib/stores/runners.svelte";

  // One task runner in the dock: its bar (kind, status, controls - and the drag
  // handle that sorts it among its siblings) over its streamed output, or over a
  // note when the agent owns the process and PADE has no output to show.
  const { row, isReorderable, activeSessionId }: {
    row: RunnerRow;
    /** A second card exists to sort against, so the bar is a drag handle. */
    isReorderable: boolean;
    /** The active agent session - pipe target for this runner's output. */
    activeSessionId: string | null;
  } = $props();

  const isSucceeded = $derived(row.done && !row.failed);

  // Human-readable status for the dot, used for both the tooltip and the
  // accessible name.
  const status = $derived.by(() => {
    if (row.failed) {
      return "Failed";
    }

    if (row.done) {
      return "Done";
    }

    return "Running";
  });
</script>

<article class="runner" class:attached={row.attached} data-runner-id={row.id}>
  <!-- The whole bar is the drag handle (canvas: header drag-to-reorder via
       the shared engine); its buttons are controls, not handles. -->
  <header
    class="bar"
    class:reorderable={isReorderable}
    aria-label="{row.label} controls"
    onpointerdown={e => {
      if (!isReorderable) {
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
      class:done={isSucceeded}
      class:failed={row.failed}
      aria-label={status}
      data-tooltip={status}
    ></span>
    <span class="name">{row.label}</span>
    {#if row.attached}
      <!-- The agent owns this process; PADE only reflects + can stop it. -->
      <span
        class="attached-tag"
        data-tooltip="Started by the agent · PADE can stop it, but its output stays in the agent's terminal"
      >agent</span>
    {:else}
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
      <p>Running in the agent's terminal</p>
      <p class="muted">Its output stays there - PADE didn't spawn this process. Stop ends it.</p>
    </div>
  {:else}
    <RunnerOutput id={row.id} lines={row.lines} />
  {/if}
</article>

<style>
  .runner {
    display: flex;
    flex-direction: column;

    /* Each runner stays readable (canvas floor); the grid scrolls when several
       stack. The output pane below scrolls independently via its own min-size. */
    min-block-size: 168px;
    background: var(--surface-1);

    /* An attached card has only a short note, no output pane - it must not hold
       the streaming-output floor, or a compact attached-only dock can't shrink. */
    &.attached {
      min-block-size: 0;
    }
  }

  /* The bar doubles as the drag-to-reorder handle for its runner - but only
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
    border-radius: var(--radius-full);
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
    border-radius: var(--radius-full);
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
    border-radius: var(--radius-full);
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

  /* Marks a card whose process the agent owns (no output; Stop still kills it). */
  .attached-tag {
    flex: none;
    padding-block: 2px;
    padding-inline: 7px;
    border-radius: var(--radius-full);
    background: var(--tertiary-wash);
    color: var(--tertiary);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  /* Stand-in for the output pane on an attached runner - there is nothing to
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
</style>
