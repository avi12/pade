<script lang="ts">
  import { os, vcs } from "@/lib/bridge";
  import ColorText from "@/lib/ColorText.svelte";
  import FileList from "@/lib/commitModal/FileList.svelte";
  import { DiffKind, parseDiff } from "@/lib/diff";
  import type { DiffLine } from "@/lib/diff";
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import { baseName } from "@/lib/paths";
  import type { CommitDetail, CommitFileEntry } from "@/lib/types";
  import { untrack } from "svelte";

  const { commit, remoteUrl, onclose }: {
    commit: CommitDetail;
    /** Browsable remote base (e.g. https://github.com/o/r), or null when there's no remote. */
    remoteUrl: string | null;
    onclose: () => void;
  } = $props();

  // Selected file within the commit; seeded to the first file so its tab is
  // selected and focusable from the first render, before the diff loads.
  // Seeded once from the prop (the modal is remounted per commit) — untrack makes
  // the intentional initial-only read explicit.
  let selectedPath = $state<string>(untrack(() => commit.files[0]?.path ?? ""));
  const selectedFile = $derived<CommitFileEntry | undefined>(
    commit.files.find(file => file.path === selectedPath) ?? commit.files[0]
  );

  // One file's diff, fetched on demand and cached so re-selecting never refetches.
  // The cache is only ever read imperatively inside loadDiff — never in reactive
  // markup — so a plain Map is correct; a SvelteMap would add needless reactivity
  // tracking for a value the template never observes.
  let diffLines = $state<DiffLine[]>([]);
  let loading = $state(false);
  let loadFailed = $state(false);
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- read imperatively, never reactively (see comment above)
  const cache = new Map<string, DiffLine[]>();

  // Binary / very large files: git emits no textual diff, so we show the note.
  // A load error is a distinct state and must not read as a big file.
  const isBigFile = $derived(
    !loadFailed && (!!selectedFile?.binary || (!loading && diffLines.length === 0))
  );

  const fileCountLabel = $derived(`${formatCount(commit.files.length)} file${commit.files.length === 1 ? "" : "s"}`);
  const commitUrl = $derived(remoteUrl ? `${remoteUrl}/commit/${commit.id}` : null);
  const paneName = $derived(selectedFile ? baseName(selectedFile.path) : "");
  const diffAria = $derived(paneName ? `Diff for ${paneName}` : "Diff");

  async function loadDiff(path: string) {
    selectedPath = path;
    loadFailed = false;
    const cached = cache.get(path);
    if (cached) {
      diffLines = cached;
      return;
    }

    loading = true;
    try {
      const raw = await vcs.commitDiff({
        sha: commit.id,
        path
      });
      // Stale-response guard: a rapid A→B switch can land A's late diff under B;
      // bail before touching shared state if the selection moved on.
      if (path !== selectedPath) {
        return;
      }

      const parsed = parseDiff(raw);
      cache.set(path, parsed);
      diffLines = parsed;
    } catch {
      // A fetch/parse failure is an error, not a big file — surface it as such
      // and never cache, so re-selecting retries instead of showing stale empty.
      if (path !== selectedPath) {
        return;
      }

      diffLines = [];
      loadFailed = true;
    } finally {
      if (path === selectedPath) {
        loading = false;
      }
    }
  }

  function openOnGithub() {
    if (commitUrl) {
      void os.openUrl(commitUrl);
    }
  }

  // ── Modal plumbing ─────────────────────────────────────────────────────────
  // A native <dialog> opened with showModal() gives Esc-to-close, a focus trap,
  // and the top-layer scrim for free (semantic HTML over a hand-rolled trap).
  let dialogEl = $state<HTMLDialogElement | null>(null);

  // The native "cancel" event fires on Esc (and backdrop dismiss on supporting
  // engines); route it to the parent so state stays the single source of truth.
  function onCancel(event: Event) {
    event.preventDefault();
    onclose();
  }

  // Backdrop-click: a modal <dialog>'s hit area covers the whole viewport, so a
  // click whose target is the dialog element itself landed on the ::backdrop.
  function onBackdropClick(event: MouseEvent) {
    if (event.target === dialogEl) {
      onclose();
    }
  }

  // Open the modal on the top layer and kick off the first file's diff. Guard
  // against re-opening an already-open dialog (showModal() throws otherwise).
  $effect(() => {
    if (dialogEl && !dialogEl.open) {
      dialogEl.showModal();
    }

    const first = commit.files[0];
    if (first) {
      void loadDiff(first.path);
    }
  });
</script>

<!-- Native modal <dialog>: Esc + focus-trap + scrim handled by the platform;
     backdrop-click closes when the hit lands on the ::backdrop, not the content. -->
<dialog
  bind:this={dialogEl}
  class="dialog"
  aria-describedby="commit-meta"
  aria-labelledby="commit-title"
  oncancel={onCancel}
  onclick={onBackdropClick}
>
  <header class="head">
    <div class="lockup">
      <div class="chips">
        <code class="sha-chip">{commit.short}</code>
        {#if commit.branch}
          <span class="branch"><span class="bdot" aria-hidden="true"></span>{commit.branch}</span>
        {/if}
      </div>
      <h2 id="commit-title">{commit.summary}</h2>
      <p id="commit-meta" class="meta">
        <span>{commit.author} · {commit.when}</span>
        <span class="sep" aria-hidden="true"></span>
        <span class="files-n">{fileCountLabel}</span>
        {#if commit.additions}
          <span class="add">+{formatCount(commit.additions)}</span>
        {/if}
        {#if commit.deletions}
          <span class="del">−{formatCount(commit.deletions)}</span>
        {/if}
      </p>
    </div>
    <div class="actions">
      <button
        class="ghbtn"
        data-tooltip={commitUrl ? "Open this commit on GitHub" : "No remote configured"}
        disabled={!commitUrl}
        onclick={openOnGithub}
      >
        <Icon name="github" size={15} /> Open on GitHub
      </button>
      <button class="close" aria-label="Close commit details" data-tooltip="Close" onclick={onclose}>
        <Icon name="close" size={16} />
      </button>
    </div>
  </header>

  <div class="body">
    <FileList files={commit.files} onpick={path => void loadDiff(path)} {selectedPath} />

    <div class="pane">
      <div class="pane-bar">
        <span class="pane-path">{paneName}</span>
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
            <button class="omit-btn" disabled={!commitUrl} onclick={openOnGithub}>
              <Icon name="github" size={14} /> View full diff
            </button>
          </div>
        {:else}
          <pre class="unified">{#each diffLines as line, i (i)}<code
              class="dl"
              class:add={line.kind === DiffKind.add}
              class:del={line.kind === DiffKind.del}
              class:metaline={line.kind === DiffKind.meta}
            ><ColorText text={line.text} />
</code>{/each}</pre>
        {/if}
      </div>
    </div>
  </div>
</dialog>

<style>
  /* The <dialog> is the centered card; showModal() puts it on the top layer and
     draws ::backdrop as the scrim (no separate scrim element needed). */
  .dialog {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    block-size: min(80vh, 760px);
    max-block-size: none;
    inline-size: min(1040px, calc(100% - 64px));
    max-inline-size: none;
    margin: auto;
    padding: 0;
    border: 1px solid var(--outline);
    border-radius: var(--radius-large);
    background: var(--surface-1);
    color: var(--on-surface);
    outline: none;
    box-shadow: 0 32px 80px var(--shadow-color);
    animation: pop-in 260ms var(--spring);

    &::backdrop {
      background: color-mix(in sRGB, var(--shadow-color) 70%, hsl(214deg 40% 4% / 55%));
      animation: fadein 160ms var(--ease);
    }
  }

  .head {
    display: flex;
    flex: none;
    gap: 14px;
    align-items: flex-start;
    padding-block: 18px;
    padding-inline: 20px;
    border-block-end: 1px solid var(--outline);
    background: var(--surface);

    .lockup {
      flex: 1;
      min-inline-size: 0;
    }

    .chips {
      display: flex;
      flex-wrap: wrap;
      gap: 10px;
      align-items: center;
    }

    .sha-chip {
      padding-block: 3px;
      padding-inline: 9px;
      border-radius: var(--radius-small);
      background: var(--primary-container);
      color: var(--on-primary-container);
      font-family: var(--font-monospace);
      font-weight: 700;
      font-size: 13px;
    }

    .branch {
      display: inline-flex;
      gap: 5px;
      align-items: center;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 11px;
    }

    .bdot {
      block-size: 6px;
      inline-size: 6px;
      border-radius: 999px;
      background: var(--tertiary);
    }

    h2 {
      margin-block: 9px 0;
      font-weight: 700;
      font-size: 17px;
      line-height: 1.3;
      text-wrap: balance;
    }

    .meta {
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      align-items: center;
      margin-block: 7px 0;
      color: var(--on-surface-variant);
      font-size: 12px;
      font-variant-numeric: tabular-nums;
    }

    .files-n {
      font-variant-numeric: tabular-nums;
    }

    .sep {
      block-size: 4px;
      inline-size: 4px;
      border-radius: 999px;
      background: var(--outline);
    }

    .add {
      color: var(--tertiary);
      font-weight: 600;
      font-variant-numeric: tabular-nums;
    }

    .del {
      color: var(--critical);
      font-weight: 600;
      font-variant-numeric: tabular-nums;
    }
  }

  .actions {
    display: flex;
    flex: none;
    gap: 8px;
    align-items: center;
  }

  .ghbtn {
    display: inline-flex;
    gap: 7px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 14px;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition: background 120ms var(--ease);

    &:hover:not(:disabled) {
      background: var(--surface-3);
    }

    &:disabled {
      opacity: 55%;
      cursor: default;
    }
  }

  .close {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 34px;
    inline-size: 34px;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-variant);
    cursor: pointer;
    transition:
      background 120ms var(--ease),
      color 120ms var(--ease);

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
    }
  }

  .body {
    display: grid;
    flex: 1;
    grid-template-columns: 264px 1fr;
    min-block-size: 0;
  }

  /* The changed-files tablist lives in commitModal/FileList.svelte. */

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

  .diff {
    flex: 1;
    overflow: auto;
    min-block-size: 0;
    padding-block: 10px 0;
    line-height: 1.55;
  }

  .state {
    margin: 0;
    padding: 14px;
    color: var(--on-surface-variant);
    font-size: 12px;
  }

  .unified {
    margin: 0;
    font-family: var(--font-monospace);
    font-size: 12px;

    .dl {
      display: block;
      padding-inline: 14px;
      color: var(--code-foreground);
      white-space: pre;
    }

    .dl.add {
      background: var(--tertiary-wash);
    }

    .dl.del {
      background: var(--critical-wash);
    }

    .dl.metaline {
      color: var(--on-surface-variant);
    }
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
