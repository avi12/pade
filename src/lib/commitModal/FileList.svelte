<script lang="ts">
  import { formatCount } from "@/lib/format";
  import { baseName } from "@/lib/paths";
  import { VcsKind } from "@/lib/types";
  import type { CommitFileEntry } from "@/lib/types";
  import { tick } from "svelte";

  // The commit's changed-files tablist: one-letter kind badges with the panel's
  // status tints, per-file +/− stats, and roving-tabindex arrow-key navigation.
  // Selection state lives with the modal; picks go back through `onpick`.
  const { files, selectedPath, onpick }: {
    files: CommitFileEntry[];
    selectedPath: string;
    onpick: (path: string) => void;
  } = $props();

  // A one-letter kind badge with its own tint — reuse the panel's status colors.
  const KIND_BADGE: Record<VcsKind, string> = {
    [VcsKind.enum.created]: "A",
    [VcsKind.enum.untracked]: "A",
    [VcsKind.enum.modified]: "M",
    [VcsKind.enum.renamed]: "R",
    [VcsKind.enum.deleted]: "D"
  };

  let listEl = $state<HTMLElement | null>(null);

  function badge(kind: VcsKind): string {
    return KIND_BADGE[kind];
  }

  async function focusTab(path: string) {
    await tick();
    listEl?.querySelector<HTMLElement>(`[data-file="${CSS.escape(path)}"]`)?.focus();
  }
</script>

<nav class="files" aria-label="Changed files">
  <h3 id="commit-files-label" class="files-eyebrow">Files</h3>
  <ul
    bind:this={listEl}
    aria-labelledby="commit-files-label"
    aria-orientation="vertical"
    onkeydown={e => {
      const isVertical = e.key === "ArrowDown" || e.key === "ArrowUp";
      const isEdge = e.key === "Home" || e.key === "End";
      if (!isVertical && !isEdge) {
        return;
      }

      e.preventDefault();
      const count = files.length;
      if (count === 0) {
        return;
      }

      const current = files.findIndex(file => file.path === selectedPath);
      let next = current;
      if (e.key === "ArrowDown") {
        next = (current + 1) % count;
      } else if (e.key === "ArrowUp") {
        next = (current - 1 + count) % count;
      } else if (e.key === "Home") {
        next = 0;
      } else {
        next = count - 1;
      }

      const target = files[next];
      onpick(target.path);
      void focusTab(target.path);
    }}
    role="tablist"
  >
    {#each files as f (f.path)}
      {@const isSel = f.path === selectedPath}
      <li role="presentation">
        <button
          class="file {f.kind}"
          class:sel={isSel}
          aria-controls="commit-diff"
          aria-label="{baseName(f.path)}, {f.kind}, +{formatCount(f.additions)} −{formatCount(f.deletions)}"
          aria-selected={isSel}
          data-file={f.path}
          onclick={() => onpick(f.path)}
          role="tab"
          tabindex={isSel ? 0 : -1}
        >
          <span class="file-top">
            <span class="kind" aria-hidden="true">{badge(f.kind)}</span>
            <span class="fname">{baseName(f.path)}</span>
          </span>
          <span class="file-stat" aria-hidden="true">
            {#if f.additions}
              <span class="add">+{formatCount(f.additions)}</span>
            {/if}
            {#if f.deletions}
              <span class="del">−{formatCount(f.deletions)}</span>
            {/if}
          </span>
        </button>
      </li>
    {/each}
  </ul>
</nav>

<style>
  .files {
    overflow-y: auto;
    min-block-size: 0;
    padding: 8px;
    border-inline-end: 1px solid var(--outline);
    background: var(--surface);

    .files-eyebrow {
      margin: 0;
      padding-block: 6px 4px;
      padding-inline: 8px;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    ul {
      display: flex;
      flex-direction: column;
      gap: 2px;
      margin: 0;
      padding: 0;
      list-style: none;
    }
  }

  .file {
    display: flex;
    flex-direction: column;
    gap: 2px;
    inline-size: 100%;
    padding-block: 7px;
    padding-inline: 9px;
    border: none;
    border-radius: 9px;
    background: transparent;
    color: var(--on-surface);
    text-align: start;
    cursor: pointer;
    transition: background 120ms var(--ease);

    &:hover {
      background: var(--surface-2);
    }

    &.sel {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    .file-top {
      display: flex;
      gap: 7px;
      align-items: center;
      min-inline-size: 0;
    }

    /* Per-kind tint on the one-letter badge — mirrors the panel status squares. */
    .kind {
      flex: none;
      padding-block: 1px;
      padding-inline: 5px;
      border-radius: 5px;
      font-family: var(--font-monospace);
      font-weight: 700;
      font-size: 10px;
    }

    &.created .kind,
    &.untracked .kind {
      background: var(--tertiary-wash);
      color: var(--tertiary);
    }

    &.modified .kind,
    &.renamed .kind {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    &.deleted .kind {
      background: var(--critical-wash);
      color: var(--critical);
    }

    .fname {
      flex: 1;
      overflow: hidden;
      min-inline-size: 0;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 12px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .file-stat {
      display: flex;
      gap: 8px;
      padding-inline-start: 2px;
      font-weight: 600;
      font-size: 10px;
      font-variant-numeric: tabular-nums;
    }

    .add {
      color: var(--tertiary);
    }

    .del {
      color: var(--critical);
    }
  }
</style>
