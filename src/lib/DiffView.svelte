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
        <div class="cell" class:filled-del={row.leftFilled}><ColorText text={row.left} /></div>
        <div
          class="cell right"
          class:filled-add={row.rightFilled}
          data-newline={row.newLine}
        ><ColorText text={row.right} /></div>
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
      ><ColorText text={line.text} /></div>
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

  .unified .line {
    padding-inline: var(--diff-line-padding, 12px);
    color: var(--code-foreground);
    white-space: pre;
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

  .split {
    display: grid;
    grid-template-columns: 1fr 1fr;

    .hunk {
      grid-column: 1 / -1;
      padding-inline: var(--diff-line-padding, 12px);
      color: var(--on-surface-variant);
      white-space: pre;
    }

    .cell {
      overflow: hidden;
      min-block-size: 1.5em;
      padding-inline: 10px;
      border-inline-end: 1px solid var(--outline);
      color: var(--code-foreground);
      white-space: pre;
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
