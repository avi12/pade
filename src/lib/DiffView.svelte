<script lang="ts">
  import ColorText from "@/lib/ColorText.svelte";
  import { DiffKind, toSplitRows } from "@/lib/diff";
  import type { DiffLine } from "@/lib/diff";

  // The one authoritative renderer for a parsed diff — unified or split —
  // shared by the Change Feed, the VCS panel, and the commit modal (DRY).
  // Presentation only: callers own fetching/parsing, the scroll container,
  // and any interactivity (a wrapper can delegate clicks through the
  // `data-newline` each new-file line carries). Callers may widen the line
  // gutter via `--diff-line-padding` (defaults to 12px).
  const { diffLines, split = false }: {
    diffLines: DiffLine[];
    split?: boolean;
  } = $props();

  const splitRows = $derived(split ? toSplitRows(diffLines) : []);
</script>

{#if split}
  <div class="split">
    {#each splitRows as row, index (index)}
      {#if row.hunk}
        <div class="hunk">{row.hunkText}</div>
      {:else}
        <div class="cell" class:filled-del={row.leftFilled}>
          <span class="gutter" aria-hidden="true">{row.oldLine ?? ""}</span>
          <span class="code"><ColorText text={row.left} /></span>
        </div>
        <div
          class="cell right"
          class:filled-add={row.rightFilled}
          data-newline={row.newLine}
        >
          <span class="gutter" aria-hidden="true">{row.newLine ?? ""}</span>
          <span class="code"><ColorText text={row.right} /></span>
        </div>
      {/if}
    {/each}
  </div>
{:else}
  <div class="unified">
    {#each diffLines as line, index (index)}
      <div
        class="line"
        class:add={line.kind === DiffKind.add}
        class:del={line.kind === DiffKind.del}
        class:meta={line.kind === DiffKind.meta}
        data-newline={line.newLine}
      >
        <span class="gutter" aria-hidden="true">
          <span class="ln">{line.oldLine ?? ""}</span>
          <span class="ln">{line.newLine ?? ""}</span>
        </span>
        <span class="code"><ColorText text={line.text} /></span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .unified,
  .split {
    background: var(--code-background);
    font-family: var(--font-monospace);
    font-size: 12px;
    line-height: 1.5;
  }

  /* "Fully printed": every code line is visible in full — long lines wrap
     (preserving whitespace) rather than clip or hide behind a side-scroll,
     and unbroken tokens may break anywhere so nothing ever overflows. */

  /* A line-number gutter runs down the inline-start edge of every code row:
     two columns (old, new) in the unified view, one per side in split. It's
     top-aligned so the numbers sit against the first visual line of a wrapped
     row, muted, tabular so digits never jitter, and unselectable so dragging to
     select code (for send-to-agent) never grabs the numbers. */
  .gutter {
    display: flex;
    flex: none;
    gap: 8px;
    justify-content: flex-end;
    padding-inline-end: 10px;
    color: var(--on-surface-variant);
    font-variant-numeric: tabular-nums;
    text-align: end;
    white-space: nowrap;
    opacity: 55%;
    user-select: none;
  }

  .gutter .ln {
    display: inline-block;
    min-inline-size: 2.5ch;
  }

  .code {
    flex: 1;
    min-inline-size: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  /* content-visibility lets the engine skip layout/paint for offscreen rows —
     on a 9.4k-line diff (Notepad++ stress test) it cut the expand's main-thread
     block ~30% and the split toggle ~20%, with instant scrolling. The intrinsic
     size reserves one line-height (1.5 × 12px) per row to keep the scrollbar
     honest. */
  .unified .line {
    contain-intrinsic-block-size: auto 18px;
    display: flex;
    content-visibility: auto;
    align-items: flex-start;
    padding-inline: var(--diff-line-padding, 12px);
    color: var(--code-foreground);
  }

  .line.add {
    background: var(--tertiary-wash);
  }

  .line.del {
    background: var(--critical-wash);
  }

  .line.meta {
    color: var(--on-surface-variant);
  }

  /* A hunk/meta row has no line numbers, so its gutter reserves the same width
     the two number columns take (2 × 2.5ch + gap) to keep the code aligned. */
  .line.meta .gutter {
    min-inline-size: calc(5ch + 8px);
  }

  .split {
    display: grid;
    grid-template-columns: 1fr 1fr;

    .hunk {
      grid-column: 1 / -1;
      padding-inline: var(--diff-line-padding, 12px);
      color: var(--on-surface-variant);
      white-space: pre-wrap;
      overflow-wrap: anywhere;
    }

    .cell {
      contain-intrinsic-block-size: auto 18px;
      display: flex;
      content-visibility: auto;
      align-items: flex-start;
      min-block-size: 1.5em;
      padding-inline: 10px;
      border-inline-end: 1px solid var(--outline);
      color: var(--code-foreground);
    }

    .cell .gutter {
      min-inline-size: 2.5ch;
    }

    .cell.right {
      border-inline-end: none;
    }

    .cell.filled-del {
      background: var(--critical-wash);
    }

    .cell.filled-add {
      background: var(--tertiary-wash);
    }
  }
</style>
