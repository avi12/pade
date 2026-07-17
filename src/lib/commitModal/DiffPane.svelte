<script lang="ts">
  import type { DiffLine } from "@/lib/diff";
  import DiffView from "@/lib/DiffView.svelte";
  import Icon from "@/lib/Icon.svelte";

  // The commit modal's right pane: path bar + the selected file's unified diff,
  // with the loading / failed / large-file states. Presentation only — the
  // modal owns the fetch, cache and state machine and hands the results down.
  const {
    name,
    diffLines,
    loading,
    loadFailed,
    isBigFile,
    commitUrl,
    onopengithub
  }: {
    /** File name shown in the path bar ("" while the commit has no files). */
    name: string;
    diffLines: DiffLine[];
    loading: boolean;
    /** A fetch/parse failure — distinct from a big file, never conflated. */
    loadFailed: boolean;
    /** Binary / very large: git emits no textual diff, so we show the note. */
    isBigFile: boolean;
    /** Browsable remote commit URL, or null when there's no remote. */
    commitUrl: string | null;
    onopengithub: () => void;
  } = $props();

  const diffAria = $derived(name ? `Diff for ${name}` : "Diff");
</script>

<div class="pane">
  <div class="pane-bar">
    <span class="pane-path">{name}</span>
    {#if isBigFile}
      <span class="big-note">large file · diff omitted</span>
    {/if}
  </div>
  <div id="commit-diff" class="diff" aria-label={diffAria} role="tabpanel" tabindex="0">
    {#if loading}
      <p class="state">Loading diff…</p>
    {:else if loadFailed}
      <p class="state">Couldn't load this diff</p>
    {:else if isBigFile}
      <div class="omit">
        <span class="omit-text">
          This file is too large to render inline. The rest of the diff is available on GitHub.
        </span>
        <button class="omit-btn" disabled={!commitUrl} onclick={onopengithub}>
          <Icon name="github" size={14} /> View full diff
        </button>
      </div>
    {:else}
      <DiffView {diffLines} />
    {/if}
  </div>
</div>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    min-block-size: 0;
    background: var(--code-background);
  }

  .pane-bar {
    display: flex;
    flex: none;
    gap: 8px;
    align-items: center;
    padding-block: 9px;
    padding-inline: 14px;
    border-block-end: 1px solid var(--outline);
    background: var(--surface-2);

    .pane-path {
      flex: 1;
      overflow: hidden;
      min-inline-size: 0;
      color: var(--on-surface);
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 12px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .big-note {
      flex: none;
      padding-block: 3px;
      padding-inline: 9px;
      border-radius: 999px;
      background: var(--warning-wash);
      color: var(--warning);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.04em;
      text-transform: uppercase;
      white-space: nowrap;
    }
  }

  /* Scroll container around the shared DiffView; the wider gutter matches the
     pane bar's 14px inline padding. */
  .diff {
    --diff-line-padding: 14px;

    flex: 1;
    overflow: auto;
    min-block-size: 0;
    padding-block: 10px;
  }

  .state {
    margin: 0;
    padding: 14px;
    color: var(--on-surface-variant);
    font-size: 12px;
  }

  .omit {
    display: flex;
    gap: 12px;
    align-items: center;
    margin-block: 12px 4px;
    margin-inline: 14px;
    padding: 14px;
    border: 1px dashed var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);

    .omit-text {
      flex: 1;
      color: var(--on-surface-variant);
      font-size: 12px;
      line-height: 1.5;
    }

    .omit-btn {
      display: inline-flex;
      flex: none;
      gap: 7px;
      align-items: center;
      padding-block: 8px;
      padding-inline: 15px;
      border: none;
      border-radius: 999px;
      background: var(--primary);
      color: var(--on-primary);
      font: inherit;
      font-weight: 700;
      font-size: 12px;
      cursor: pointer;
      transition: filter 120ms var(--ease);

      &:hover:not(:disabled) {
        filter: brightness(1.06);
      }

      &:disabled {
        opacity: 55%;
        cursor: default;
      }
    }
  }
</style>
