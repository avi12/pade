<script lang="ts">
  import { createFeedPreviews } from "@/lib/feed/preview/feed-previews";
  import Icon from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { effective } from "@/lib/settings/prefs.svelte";
  import type { CommitDetail } from "@/lib/types";
  import { commitPreviewSource } from "@/lib/vcs/commit-preview-source";
  import FileList from "@/lib/vcs/commitModal/FileList.svelte";
  import FilePreviewPane from "@/lib/vcs/commitModal/FilePreviewPane.svelte";
  import CommitStats from "@/lib/vcs/CommitStats.svelte";
  import { repositoryCommitUrl } from "@/lib/vcs/repository-links";
  import { untrack } from "svelte";

  const { project, commit, remoteUrl, onclose }: {
    /** The repository containing `commit`; windows never share a global cwd. */
    project: string;
    commit: CommitDetail;
    /** Browsable remote base (e.g. https://github.com/o/r), or null when there's no remote. */
    remoteUrl: string | null;
    onclose: () => void;
  } = $props();

  // Selected file within the commit; seeded to the first file so its tab is
  // selected and focusable from the first render, before its preview loads.
  // Seeded once from the prop (the modal is remounted per commit) - untrack makes
  // the intentional initial-only read explicit.
  let selectedPath = $state(untrack(() => commit.files[0]?.path ?? ""));
  const selectedFile = $derived(
    commit.files.find(file => file.path === selectedPath) ?? commit.files.at(0)
  );

  // Each file's diff, picture, clip or rendered document as this commit left
  // it - the Change Feed's previews, read out of the commit and cached by path
  // so re-selecting never refetches.
  const previews = createFeedPreviews(
    untrack(() => commitPreviewSource({
      cwd: project,
      sha: commit.id
    }))
  );
  let diffMode = $state(effective.diffStyle);

  const commitUrl = $derived(
    repositoryCommitUrl({
      remoteUrl,
      commit: commit.id
    })
  );

  async function selectFile(path: string) {
    selectedPath = path;
    await previews.loadCardPreview({
      id: path,
      path
    });
  }

  async function openOnRemote() {
    if (!commitUrl) {
      return;
    }

    try {
      await sendMessage(Command.openUrl, { url: commitUrl });
    } catch {
    // Opening the remote is best-effort; a failed browser launch is silent.
    }
  }

  // ── Modal plumbing ─────────────────────────────────────────────────────────
  // A native <dialog> opened with showModal() gives Esc-to-close, a focus trap,
  // and the top-layer scrim for free (semantic HTML over a hand-rolled trap).
  let elDialog = $state<HTMLDialogElement | null>(null);

  // Open the modal on the top layer and kick off the first file's preview. Guard
  // against re-opening an already-open dialog (showModal() throws otherwise).
  $effect(() => {
    if (elDialog && !elDialog.open) {
      elDialog.showModal();
    }

    const first = commit.files[0];
    // Untracked: the fetch reads the preview caches, and their filling must not
    // re-run this effect and snap the selection back to the first file.
    if (first) {
      untrack(() => selectFile(first.path));
    }
  });
</script>

<!-- Native modal <dialog>: Esc + focus-trap + scrim handled by the platform;
     backdrop-click closes when the hit lands on the ::backdrop, not the content. -->
<dialog
  bind:this={elDialog}
  class="dialog"
  aria-describedby="commit-metadata"
  aria-labelledby="commit-title"
  oncancel={e => {
    e.preventDefault();
    onclose();
  }}
  onclick={e => {
    if (e.target === elDialog) {
      onclose();
    }
  }}
>
  <header class="head">
    <div class="lockup">
      <div class="chips">
        <code class="commit-hash-chip">{commit.short}</code>
        {#if commit.branch}
          <span class="branch"><span class="branch-dot" aria-hidden="true"></span>{commit.branch}</span>
        {/if}
      </div>
      <h2 id="commit-title">{commit.summary}</h2>
      <p id="commit-metadata" class="metadata">
        <span>{commit.author} · {commit.when}</span>
        <span class="separator" aria-hidden="true"></span>
        <CommitStats additions={commit.additions} deletions={commit.deletions} files={commit.files.length} />
      </p>
    </div>
    <div class="actions">
      <button
        class="remote-button"
        data-tooltip={commitUrl ? "Open this commit on its remote" : "No supported remote configured"}
        disabled={!commitUrl}
        onclick={openOnRemote}
      >
        <Icon name="external" size={15} /> Open on remote
      </button>
      <button class="close" aria-label="Close commit details" data-tooltip="Close" onclick={onclose}>
        <Icon name="close" size={16} />
      </button>
    </div>
  </header>

  <div class="body">
    <FileList files={commit.files} onpick={path => selectFile(path)} {selectedPath} />

    {#if selectedFile}
      <FilePreviewPane
        card={{
          id: selectedFile.path,
          path: selectedFile.path
        }}
        {commitUrl}
        isBinary={selectedFile.binary}
        onopenremote={openOnRemote}
        {previews}
        bind:diffMode
      />
    {/if}
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

    .commit-hash-chip {
      padding-block: 3px;
      padding-inline: 9px;
      border-radius: var(--radius-small);
      background: var(--primary-container);
      color: var(--on-primary-container);
      font-family: var(--font-monospace);
      font-weight: 700;
      font-size: 0.8125rem;
    }

    .branch {
      display: inline-flex;
      gap: 5px;
      align-items: center;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 0.6875rem;
    }

    .branch-dot {
      block-size: 6px;
      inline-size: 6px;
      border-radius: var(--radius-full);
      background: var(--tertiary);
    }

    h2 {
      margin-block: 9px 0;
      font-weight: 700;
      font-size: 1.0625rem;
      line-height: 1.3;
      text-wrap: balance;
    }

    .metadata {
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      align-items: center;
      margin-block: 7px 0;
      color: var(--on-surface-variant);
      font-size: 0.75rem;
    }

    .separator {
      block-size: 4px;
      inline-size: 4px;
      border-radius: var(--radius-full);
      background: var(--outline);
    }
  }

  .actions {
    display: flex;
    flex: none;
    gap: 8px;
    align-items: center;
  }

  .remote-button {
    display: inline-flex;
    gap: 7px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 14px;
    border: none;
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 0.8125rem;
    cursor: pointer;
    transition: background 120ms var(--ease);

    &:disabled {
      opacity: 55%;
      cursor: default;
    }

    &:hover:not(:disabled) {
      background: var(--surface-3);
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
    border-radius: var(--radius-full);
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

  /* The changed-files tablist lives in commitModal/FileList.svelte; the
     preview pane (path bar + states + the file's preview) in
     commitModal/FilePreviewPane.svelte. */
</style>
