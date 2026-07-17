<script lang="ts">
  import { vcs } from "@/lib/bridge";
  import { parseDiff } from "@/lib/diff";
  import DiffView from "@/lib/DiffView.svelte";
  import { formatCount } from "@/lib/format";
  import { baseName } from "@/lib/paths";
  import { effective } from "@/lib/prefs.svelte";
  import { DiffStyle, VcsKind } from "@/lib/types";
  import type { StatusEntry } from "@/lib/types";

  // Working-tree changes: the unreviewed/staged groups and the inline diff of
  // the selected file. Agent-oriented review lives in the "unreviewed"
  // (unstaged) group; "approve" moves entries into the staged group.
  const { entries }: {
    entries: StatusEntry[];
  } = $props();

  let selected = $state<StatusEntry | null>(null);
  let diff = $state("");

  const unstaged = $derived(entries.filter(e => !e.staged));
  const staged = $derived(entries.filter(e => e.staged));

  async function open(entry: StatusEntry) {
    selected = entry;
    const isUntracked = entry.kind === VcsKind.enum.untracked;
    diff = isUntracked
      ? "(new file — not yet tracked)"
      : await vcs.diff({
        path: entry.path,
        staged: entry.staged
      });
  }

  // Diff rendering funnels through the shared parser and the shared DiffView
  // renderer (DRY) — one authoritative line classifier and one painter for the
  // Change Feed, this panel, and the commit modal.
  const diffLines = $derived(parseDiff(diff));
  const isSplit = $derived(effective.diffStyle === DiffStyle.enum.split);
</script>

{#if unstaged.length}
  <section class="group">
    <h3><span class="dot agent"></span> Unreviewed <span class="n">{formatCount(unstaged.length)}</span></h3>
    {#each unstaged as e (e.path)}
      <button class="row" class:sel={selected?.path === e.path} onclick={() => open(e)}>
        <span class="k {e.kind}">{e.kind[0].toUpperCase()}</span>
        <span class="fname">{baseName(e.path)}</span>
      </button>
    {/each}
  </section>
{/if}

{#if staged.length}
  <section class="group">
    <h3><span class="dot staged"></span> Staged <span class="n">{formatCount(staged.length)}</span></h3>
    {#each staged as e (e.path)}
      <button class="row" class:sel={selected?.path === e.path} onclick={() => open(e)}>
        <span class="k {e.kind}">{e.kind[0].toUpperCase()}</span>
        <span class="fname">{baseName(e.path)}</span>
      </button>
    {/each}
  </section>
{/if}

{#if !entries.length}
  <p class="empty">Working tree clean.</p>
{/if}

{#if selected}
  <section class="diff">
    <h3 class="difftitle">{baseName(selected.path)}</h3>
    <div class="diffbody">
      <DiffView {diffLines} split={isSplit} />
    </div>
  </section>
{/if}

<style>
  .n {
    padding-block: 1px;
    padding-inline: 7px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface);
    font-weight: 700;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .dot {
    block-size: 8px;
    inline-size: 8px;
    border-radius: 50%;
  }

  .dot.agent {
    background: var(--tertiary);
  }

  .dot.staged {
    background: var(--primary);
  }

  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding-block: 7px;
    padding-inline: 9px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    text-align: start;
    cursor: pointer;
  }

  .row:hover {
    background: var(--surface-2);
  }

  .row.sel {
    background: var(--primary-container);
    color: var(--on-primary-container);
  }

  .k {
    display: grid;
    flex: none;
    place-items: center;
    block-size: 18px;
    inline-size: 18px;
    border-radius: var(--radius-small);
    color: #ffffff;
    font-weight: 700;
    font-size: 11px;
  }

  .k.created,
  .k.untracked {
    background: var(--tertiary);
  }

  .k.modified,
  .k.renamed {
    background: var(--primary);
  }

  .k.deleted {
    background: var(--critical);
  }

  .fname {
    overflow: hidden;
    font-family: var(--font-monospace);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* `flex: none` matters: the panel's `.scroll` column would otherwise shrink
     this section (overflow: hidden zeroes its automatic minimum size) to a
     2px sliver whenever the file list + commits fill the viewport. */
  .diff {
    flex: none;
    overflow: hidden;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
  }

  .difftitle {
    margin: 0;
    padding: 8px 12px;
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-weight: 600;
    font-size: 12px;
    letter-spacing: 0;
    text-transform: none;
  }

  /* Scroll container around the shared DiffView. */
  .diffbody {
    overflow: auto;
    max-block-size: 300px;
    padding-block: 8px;
    background: var(--code-background);
  }
</style>
