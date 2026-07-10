<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { pipeRunner, runnerRows, stopRunner } from "@/lib/stores/runners.svelte";

  // The active agent session — pipe target for a runner's output.
  const { activeSessionId }: {
    activeSessionId: string | null;
  } = $props();

  const rows = $derived(runnerRows());

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
  <section class="dock" aria-label="Task runners">
    <header class="head">
      <h2>Task runners</h2>
      <span class="count">{rows.length}</span>
      <span class="spacer"></span>
      <span class="hint">Running side by side</span>
    </header>

    <div class="grid">
      {#each rows as row (row.id)}
        <article class="runner">
          <div class="bar">
            <span class="kind {row.kind}">{row.kind}</span>
            <span class="dot" class:done={row.done}></span>
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
          <div class="out" use:autoscroll>
            {#each row.lines as line, i (i)}
              <div class="line">{line || " "}</div>
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
    block-size: min(40%, 360px);
    border-block-start: 1px solid var(--outline);
    background: var(--surface-1);
    animation: rise 280ms var(--ease);
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
    color: var(--on-surface-var);
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
    min-block-size: 0;
    background: var(--surface-1);
  }

  .bar {
    display: flex;
    gap: 8px;
    align-items: center;
    padding-block: 7px;
    padding-inline: 10px;
    background: var(--surface-2);
  }

  .kind {
    flex: none;
    padding-block: 2px;
    padding-inline: 8px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-var);
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
    color: var(--on-surface-var);
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
    background: var(--crit-wash);
    color: var(--crit);
  }

  .out {
    flex: 1;
    overflow: auto;
    min-block-size: 0;
    padding-block: 8px;
    padding-inline: 10px;
    background: var(--code-bg);
    color: var(--code-fg);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
  }

  .line {
    min-block-size: 1.5em;
    white-space: pre-wrap;
    animation: line-in 180ms var(--ease);
  }
</style>
