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

  let listElement = $state<HTMLElement | null>(null);

  function badge(kind: VcsKind): string {
    return KIND_BADGE[kind];
  }

  async function focusTab(path: string) {
    await tick();
    listElement?.querySelector<HTMLElement>(`[data-file="${CSS.escape(path)}"]`)?.focus();
  }
</script>

<nav class="files" aria-label="Changed files">
  <h3 id="commit-file-list-label" class="files-eyebrow">Files</h3>
  <ul
    bind:this={listElement}
    aria-labelledby="commit-file-list-label"
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
      focusTab(target.path);
    }}
    role="tablist"
  >
    {#each files as file (file.path)}
      {@const isSelected = file.path === selectedPath}
      <li role="presentation">
        <button
          class="file {file.kind}"
          class:selected={isSelected}
          aria-controls="commit-diff"
          aria-label="{baseName(file.path)}, {file.kind}, +{formatCount(file.additions)} −{formatCount(file.deletions)}"
          aria-selected={isSelected}
          data-file={file.path}
          onclick={() => onpick(file.path)}
          role="tab"
          tabindex={isSelected ? 0 : -1}
        >
          <span class="file-top">
            <span class="kind" aria-hidden="true">{badge(file.kind)}</span>
            <span class="file-name">{baseName(file.path)}</span>
          </span>
          <span class="file-statistics" aria-hidden="true">
            {#if file.additions}
              <span class="add">+{formatCount(file.additions)}</span>
            {/if}
            {#if file.deletions}
              <span class="deletion">−{formatCount(file.deletions)}</span>
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

    /* Inset primary ring (canon) — an outer outline would be clipped by the
       file list's own overflow. */
    &:focus-visible {
      outline: none;
      box-shadow: inset 0 0 0 2px var(--primary);
    }

    &.selected {
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

    .file-name {
      flex: 1;
      overflow: hidden;
      min-inline-size: 0;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 12px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .file-statistics {
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

    .deletion {
      color: var(--critical);
    }
  }
</style>
